/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::format_err;
use anyhow::Context;
use bookmarks::BookmarkKey;
use bookmarks::BookmarkUpdateReason;
use bookmarks::BookmarksRef;
use bookmarks_movement::BookmarkUpdatePolicy;
use bookmarks_movement::BookmarkUpdateTargets;
use bookmarks_movement::UpdateBookmarkOp;
use bytes::Bytes;
use cross_repo_sync::CandidateSelectionHint;
use cross_repo_sync::CommitSyncContext;
use hook_manager::manager::HookManagerRef;
use mononoke_types::ChangesetId;

use crate::errors::MononokeError;
use crate::repo::RepoContext;

impl RepoContext {
    /// Move a bookmark.
    pub async fn move_bookmark(
        &self,
        bookmark: &BookmarkKey,
        target: ChangesetId,
        old_target: Option<ChangesetId>,
        allow_non_fast_forward: bool,
        pushvars: Option<&HashMap<String, Bytes>>,
        affected_changesets_limit: Option<usize>,
    ) -> Result<(), MononokeError> {
        self.start_write()?;

        // We need to find out where the bookmark currently points to in order
        // to move it.  Make sure to bypass any out-of-date caches.
        let old_target = match old_target {
            Some(old_target) => old_target,
            None => self
                .blob_repo()
                .bookmarks()
                .get(self.ctx().clone(), bookmark)
                .await
                .context("Failed to fetch old bookmark target")?
                .ok_or_else(|| {
                    MononokeError::InvalidRequest(format!("bookmark '{}' does not exist", bookmark))
                })?,
        };

        fn make_move_op<'a>(
            bookmark: &'a BookmarkKey,
            target: ChangesetId,
            old_target: ChangesetId,
            allow_non_fast_forward: bool,
            pushvars: Option<&'a HashMap<String, Bytes>>,
            affected_changesets_limit: Option<usize>,
        ) -> UpdateBookmarkOp<'a> {
            let op = UpdateBookmarkOp::new(
                bookmark,
                BookmarkUpdateTargets {
                    old: old_target,
                    new: target,
                },
                if allow_non_fast_forward {
                    BookmarkUpdatePolicy::AnyPermittedByConfig
                } else {
                    BookmarkUpdatePolicy::FastForwardOnly
                },
                BookmarkUpdateReason::ApiRequest,
                affected_changesets_limit,
            )
            .with_pushvars(pushvars);
            op.log_new_public_commits_to_scribe()
        }
        if let Some(redirector) = self.push_redirector.as_ref() {
            let large_bookmark = redirector.small_to_large_bookmark(bookmark).await?;
            if &large_bookmark == bookmark {
                return Err(MononokeError::InvalidRequest(format!(
                    "Cannot move shared bookmark '{}' from small repo",
                    bookmark
                )));
            }
            let ctx = self.ctx();
            let target = redirector
                .small_to_large_commit_syncer
                .sync_commit(
                    ctx,
                    target,
                    CandidateSelectionHint::Only,
                    CommitSyncContext::PushRedirector,
                    false,
                )
                .await?
                .ok_or_else(|| {
                    format_err!(
                        "Error in move_bookmark absence of corresponding commit in target repo for {}",
                        target,
                    )
                })?;
            let old_target = redirector
                .get_small_to_large_commit_equivalent(ctx, old_target)
                .await?;
            let log_id = make_move_op(
                &large_bookmark,
                target,
                old_target,
                allow_non_fast_forward,
                pushvars,
                affected_changesets_limit,
            )
            .run(
                self.ctx(),
                self.authorization_context(),
                redirector.repo.inner_repo(),
                redirector.repo.hook_manager(),
            )
            .await?;
            // Wait for bookmark to catch up on small repo
            redirector.ensure_backsynced(ctx, log_id).await?;
        } else {
            make_move_op(
                bookmark,
                target,
                old_target,
                allow_non_fast_forward,
                pushvars,
                affected_changesets_limit,
            )
            .run(
                self.ctx(),
                self.authorization_context(),
                self.inner_repo(),
                self.hook_manager().as_ref(),
            )
            .await?;
        }

        Ok(())
    }
}
