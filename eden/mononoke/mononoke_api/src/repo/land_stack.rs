/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;
use std::collections::HashSet;

use blobstore::Loadable;
use bookmarks::BookmarkKey;
use bookmarks_movement::BookmarkKindRestrictions;
pub use bookmarks_movement::PushrebaseOutcome;
use bytes::Bytes;
use cloned::cloned;
use commit_graph::CommitGraphRef;
use cross_repo_sync::CommitSyncOutcome;
use cross_repo_sync::Large;
use cross_repo_sync::Small;
use futures::future;
use futures::future::TryFutureExt;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use hook_manager::manager::HookManagerRef;
use hook_manager::CrossRepoPushSource;
use hook_manager::PushAuthoredBy;
use mononoke_types::ChangesetId;
use pushrebase_client::normal_pushrebase;
use repo_blobstore::RepoBlobstoreRef;
use unbundle::PushRedirector;

use crate::errors::MononokeError;
use crate::repo::RepoContext;
use crate::MononokeRepo;

impl<R: MononokeRepo> RepoContext<R> {
    async fn convert_old_bookmark_value(
        &self,
        redirector: &PushRedirector<R>,
        bookmark_value: Large<Option<ChangesetId>>,
    ) -> anyhow::Result<Small<Option<ChangesetId>>> {
        let large_cs_id = match bookmark_value {
            Large(Some(cs_id)) => cs_id,
            Large(None) => return Ok(Small(None)),
        };
        let syncer = &redirector.large_to_small_commit_syncer;
        match syncer
            .get_commit_sync_outcome(self.ctx(), large_cs_id)
            .await?
        {
            None => anyhow::bail!(
                "Bookmark: unexpected absence of CommitSyncOutcome for {} in {:?}",
                large_cs_id,
                syncer
            ),
            // EquivalentWorkingCopyAncestor is fine because the bookmark commit in the
            // large repo might not have come from the small repo
            Some(CommitSyncOutcome::RewrittenAs(small_cs_id, _))
            | Some(CommitSyncOutcome::EquivalentWorkingCopyAncestor(small_cs_id, _)) => {
                Ok(Small(Some(small_cs_id)))
            }
            Some(outcome) => anyhow::bail!(
                "Bookmark: unexpected CommitSyncOutcome for {} in {:?}: {:?}",
                large_cs_id,
                syncer,
                outcome
            ),
        }
    }
    async fn convert_outcome(
        &self,
        redirector: &PushRedirector<R>,
        outcome: Large<PushrebaseOutcome>,
        _bookmark: BookmarkKey,
    ) -> Result<Small<PushrebaseOutcome>, MononokeError> {
        let ctx = self.ctx();
        let Large(PushrebaseOutcome {
            old_bookmark_value,
            head,
            retry_num,
            rebased_changesets,
            pushrebase_distance,
            log_id,
        }) = outcome;
        redirector.ensure_backsynced(ctx, log_id).await?;

        // Convert all fields from large to small repo
        let (Small(old_bookmark_value), head, rebased_changesets) = futures::try_join!(
            self.convert_old_bookmark_value(redirector, Large(old_bookmark_value)),
            redirector.get_large_to_small_commit_equivalent(ctx, head),
            redirector.convert_pushrebased_changesets(ctx, rebased_changesets)
        )?;

        Ok(Small(PushrebaseOutcome {
            old_bookmark_value,
            head,
            retry_num,
            rebased_changesets,
            pushrebase_distance,
            log_id,
        }))
    }

    /// Land a stack of commits to a bookmark via pushrebase.
    pub async fn land_stack(
        &self,
        bookmark: impl AsRef<str>,
        head: ChangesetId,
        base: ChangesetId,
        pushvars: Option<&HashMap<String, Bytes>>,
        bookmark_restrictions: BookmarkKindRestrictions,
        push_authored_by: PushAuthoredBy,
        force_local_pushrebase: bool,
    ) -> Result<PushrebaseOutcome, MononokeError> {
        self.start_write()?;

        let bookmark = bookmark.as_ref();
        let bookmark = BookmarkKey::new(bookmark)?;

        // Check that base is an ancestor of the head commit, and fail with an
        // appropriate error message if that's not the case.
        if !self
            .repo()
            .commit_graph()
            .is_ancestor(self.ctx(), base, head)
            .await?
        {
            return Err(MononokeError::InvalidRequest(format!(
                "Not a stack: base commit {} is not an ancestor of head commit {}",
                base, head,
            )));
        }

        // Find the commits we are interested in, and load their bonsai
        // changesets.   These are the commits that are ancestors of the head
        // commit and descendants of the base commit.
        let ctx = self.ctx();
        let blobstore = self.repo().repo_blobstore();
        let changesets: HashSet<_> = self
            .repo()
            .commit_graph()
            .range_stream(ctx, base, head)
            .await?
            .filter(|cs_id| future::ready(*cs_id != base))
            .map(|cs_id| {
                cloned!(ctx);
                async move {
                    cs_id
                        .load(&ctx, blobstore)
                        .map_err(MononokeError::from)
                        .await
                }
            })
            .buffer_unordered(100)
            .try_collect()
            .await?;

        let outcome = if let Some(redirector) = self.push_redirector.as_ref() {
            // run hooks on small repo
            bookmarks_movement::run_changeset_hooks(
                ctx,
                self.hook_manager().as_ref(),
                &bookmark,
                changesets.iter(),
                pushvars,
                CrossRepoPushSource::NativeToThisRepo,
                push_authored_by,
            )
            .await?;
            // Convert changesets to large repo
            let large_bookmark = redirector.small_to_large_bookmark(&bookmark).await?;
            let small_to_large = redirector
                .sync_uploaded_changesets(ctx, changesets, Some(&large_bookmark))
                .await?;
            // Land the mapped changesets on the large repo
            let outcome = normal_pushrebase(
                self.ctx(),
                &redirector.repo,
                small_to_large.into_values().collect(),
                &large_bookmark,
                pushvars,
                redirector.repo.hook_manager(),
                CrossRepoPushSource::PushRedirected,
                bookmark_restrictions,
                self.authorization_context(),
                true, // log_new_public_commits_to_scribe
                force_local_pushrebase,
            )
            .await?;
            // Convert response back, finishing the land on the small repo
            self.convert_outcome(redirector, Large(outcome), bookmark)
                .await?
                .0
        } else {
            normal_pushrebase(
                self.ctx(),
                self.repo(),
                changesets,
                &bookmark,
                pushvars,
                self.hook_manager().as_ref(),
                CrossRepoPushSource::NativeToThisRepo,
                bookmark_restrictions,
                self.authorization_context(),
                true, // log_new_public_commits_to_scribe
                force_local_pushrebase,
            )
            .await?
        };

        Ok(outcome)
    }
}
