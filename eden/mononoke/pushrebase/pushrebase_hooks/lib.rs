/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use bonsai_git_mapping::BonsaiGitMappingArc;
use bonsai_globalrev_mapping::BonsaiGlobalrevMappingArc;
use bookmarks_types::BookmarkKey;
use context::CoreContext;
use git_mapping_pushrebase_hook::GitMappingPushrebaseHook;
use globalrev_pushrebase_hook::GlobalrevPushrebaseHook;
use metaconfig_types::PushrebaseParams;
use pushrebase_hook::PushrebaseHook;
use pushrebase_mutation_mapping::PushrebaseMutationMappingRef;
use repo_bookmark_attrs::RepoBookmarkAttrsRef;
use repo_cross_repo::RepoCrossRepoRef;
use repo_identity::RepoIdentityRef;
use synced_commit_mapping_pushrebase_hook::CrossRepoSyncPushrebaseHook;
use synced_commit_mapping_pushrebase_hook::ForwardSyncedCommitInfo;
use thiserror::Error;

/// An error encountered during an attempt to move a bookmark.
#[derive(Debug, Error)]
pub enum PushrebaseHooksError {
    #[error(
        "This repository uses Globalrevs. Pushrebase is only allowed onto the bookmark '{}', this push was for '{}'",
        .globalrevs_publishing_bookmark,
        .bookmark
    )]
    PushrebaseInvalidGlobalrevsBookmark {
        bookmark: BookmarkKey,
        globalrevs_publishing_bookmark: BookmarkKey,
    },

    #[error(
        "Pushrebase is not allowed onto the bookmark '{}', because this bookmark is required to be an ancestor of '{}'",
        .bookmark,
        .descendant_bookmark,
    )]
    PushrebaseNotAllowedRequiresAncestorsOf {
        bookmark: BookmarkKey,
        descendant_bookmark: BookmarkKey,
    },

    #[error(transparent)]
    Error(#[from] anyhow::Error),
}

/// Get a Vec of the relevant pushrebase hooks for PushrebaseParams, using this repo when
/// required by those hooks.
pub async fn get_pushrebase_hooks(
    ctx: &CoreContext,
    repo: &(
         impl BonsaiGitMappingArc
         + BonsaiGlobalrevMappingArc
         + PushrebaseMutationMappingRef
         + RepoBookmarkAttrsRef
         + RepoCrossRepoRef
         + RepoIdentityRef
     ),
    bookmark: &BookmarkKey,
    pushrebase_params: &PushrebaseParams,
    forward_synced_commit_info: Option<ForwardSyncedCommitInfo>,
) -> Result<Vec<Box<dyn PushrebaseHook>>, PushrebaseHooksError> {
    let mut pushrebase_hooks = Vec::new();
    let repo_id = repo.repo_identity().id();

    match pushrebase_params.globalrev_config.as_ref() {
        Some(config) if config.publishing_bookmark == *bookmark => {
            let add_hook = if let Some(small_repo_id) = config.globalrevs_small_repo_id {
                // Only add hook if pushes are being redirected in the small
                // repo that has globalrevs being enabled.  This means that
                // the source of truth for that repo is the large repo, so the
                // large repo must assign globalrevs.  If pushredirection in
                // the small repo is *not* enabled, then that means globalrevs
                // are being assigned there, and we must not do it in the
                // large repo.
                repo.repo_cross_repo()
                    .live_commit_sync_config()
                    .push_redirector_enabled_for_public(ctx, small_repo_id)
                    .await?
            } else {
                true
            };
            if add_hook {
                let hook = GlobalrevPushrebaseHook::new(
                    ctx.clone(),
                    repo.bonsai_globalrev_mapping_arc().clone(),
                    repo_id,
                    config.globalrevs_small_repo_id,
                );
                pushrebase_hooks.push(hook);
            }
        }
        Some(config) if config.globalrevs_small_repo_id.is_none() => {
            return Err(PushrebaseHooksError::PushrebaseInvalidGlobalrevsBookmark {
                bookmark: bookmark.clone(),
                globalrevs_publishing_bookmark: config.publishing_bookmark.clone(),
            });
        }
        _ => {
            // No hook necessary
        }
    };

    for attr in repo.repo_bookmark_attrs().select(bookmark) {
        if let Some(descendant_bookmark) = &attr.params().ensure_ancestor_of {
            return Err(
                PushrebaseHooksError::PushrebaseNotAllowedRequiresAncestorsOf {
                    bookmark: bookmark.clone(),
                    descendant_bookmark: descendant_bookmark.clone(),
                },
            );
        }
    }

    if pushrebase_params.populate_git_mapping {
        let hook = GitMappingPushrebaseHook::new(repo.bonsai_git_mapping_arc().clone());
        pushrebase_hooks.push(hook);
    }
    if let Some(config) = repo
        .repo_cross_repo()
        .live_commit_sync_config()
        .get_common_config_if_exists(repo_id)?
    {
        // The || forward_synced_commit_info.is_some() shouldn't be necessary but
        // some tests are doing this in large->small direction.
        if forward_synced_commit_info.is_some()
            || (config.large_repo_id == repo_id
                && config.common_pushrebase_bookmarks.contains(bookmark))
        {
            let hook = CrossRepoSyncPushrebaseHook::new(
                repo.repo_cross_repo().synced_commit_mapping().clone(),
                // We are assuming that pushrebase is always small to large.
                repo.repo_identity().id(),
                forward_synced_commit_info,
            );
            pushrebase_hooks.push(hook);
        }
    }

    match repo.pushrebase_mutation_mapping().get_hook() {
        Some(hook) => pushrebase_hooks.push(hook),
        None => {}
    }
    Ok(pushrebase_hooks)
}
