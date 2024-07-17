/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Error;
use bookmark_renaming::get_bookmark_renamers;
use bookmark_renaming::BookmarkRenamer;
use bookmark_renaming::BookmarkRenamers;
use bookmarks::BookmarkKey;
use context::CoreContext;
use live_commit_sync_config::LiveCommitSyncConfig;
use metaconfig_types::CommitSyncConfig;
use metaconfig_types::CommitSyncConfigVersion;
use metaconfig_types::CommitSyncDirection;
use metaconfig_types::CommonCommitSyncConfig;
use metaconfig_types::GitSubmodulesChangesAction;
use mononoke_types::RepositoryId;
use movers::get_movers;
use movers::Mover;
use movers::Movers;

use crate::reporting::log_warning;

// TODO(T169306120): rename this module

pub async fn get_git_submodule_action_by_version(
    ctx: &CoreContext,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    version: &CommitSyncConfigVersion,
    // TODO(T179530927): stop treating source repo always as small repo
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<GitSubmodulesChangesAction, Error> {
    let commit_sync_config = live_commit_sync_config
        .get_commit_sync_config_by_version(source_repo_id, version)
        .await?;

    let small_repo_configs = commit_sync_config.small_repos;
    if let Some(small_repo_config) = small_repo_configs.get(&source_repo_id) {
        // If sync direction is forward, small repo would be the source
        return Ok(small_repo_config
            .submodule_config
            .git_submodules_action
            .clone());
    };

    if let Some(small_repo_config) = small_repo_configs.get(&target_repo_id) {
        // If sync direction is backwards, small repo would be the target
        let small_repo_action = small_repo_config
            .submodule_config
            .git_submodules_action
            .clone();

        return Ok(small_repo_action);
    };

    // If the action if not found above, there might be something wrong. We
    // should still fallback to the default action to avoid ever syncing submodule
    // file changes to the large repo, but let's at least log a warning.
    log_warning(
        ctx,
        format!(
            "Couldn't find git submodule action for source repo id {}, target repo id {} and commit sync version {}",
            source_repo_id, target_repo_id, version,
        ),
    );
    Ok(GitSubmodulesChangesAction::default())
}

pub async fn get_mover(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    version: &CommitSyncConfigVersion,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<Mover, Error> {
    let commit_sync_config = live_commit_sync_config
        .get_commit_sync_config_by_version(source_repo_id, version)
        .await?;
    let common_config = live_commit_sync_config.get_common_config(source_repo_id)?;

    let Movers { mover, .. } = get_movers_from_config(
        &common_config,
        &commit_sync_config,
        source_repo_id,
        target_repo_id,
    )?;
    Ok(mover)
}

pub async fn get_reverse_mover(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    version: &CommitSyncConfigVersion,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<Mover, Error> {
    let commit_sync_config = live_commit_sync_config
        .get_commit_sync_config_by_version(source_repo_id, version)
        .await?;
    let common_config = live_commit_sync_config.get_common_config(source_repo_id)?;

    let Movers { reverse_mover, .. } = get_movers_from_config(
        &common_config,
        &commit_sync_config,
        source_repo_id,
        target_repo_id,
    )?;
    Ok(reverse_mover)
}

pub async fn get_bookmark_renamer(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<BookmarkRenamer, Error> {
    let commit_sync_config = live_commit_sync_config.get_common_config(source_repo_id)?;

    let BookmarkRenamers {
        bookmark_renamer, ..
    } = get_bookmark_renamers_from_config(&commit_sync_config, source_repo_id, target_repo_id)?;
    Ok(bookmark_renamer)
}

pub async fn get_reverse_bookmark_renamer(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<BookmarkRenamer, Error> {
    let commit_sync_config = live_commit_sync_config.get_common_config(source_repo_id)?;

    let BookmarkRenamers {
        reverse_bookmark_renamer,
        ..
    } = get_bookmark_renamers_from_config(&commit_sync_config, source_repo_id, target_repo_id)?;
    Ok(reverse_bookmark_renamer)
}

pub async fn get_small_repos_for_version(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    repo_id: RepositoryId,
    version: &CommitSyncConfigVersion,
) -> Result<HashSet<RepositoryId>, Error> {
    let commit_sync_config = live_commit_sync_config
        .get_commit_sync_config_by_version(repo_id, version)
        .await?;

    Ok(commit_sync_config.small_repos.keys().cloned().collect())
}

pub async fn version_exists(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    repo_id: RepositoryId,
    version: &CommitSyncConfigVersion,
) -> Result<bool, Error> {
    let maybe_version = live_commit_sync_config
        .get_commit_sync_config_by_version_if_exists(repo_id, version)
        .await?;
    Ok(maybe_version.is_some())
}

pub async fn get_common_pushrebase_bookmarks(
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    repo_id: RepositoryId,
) -> Result<Vec<BookmarkKey>, Error> {
    let common_sync_config = live_commit_sync_config.get_common_config(repo_id)?;
    Ok(common_sync_config.common_pushrebase_bookmarks)
}

fn get_movers_from_config(
    common_config: &CommonCommitSyncConfig,
    commit_sync_config: &CommitSyncConfig,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<Movers, Error> {
    let (direction, small_repo_id) =
        get_direction_and_small_repo_id(common_config, source_repo_id, target_repo_id)?;
    get_movers(commit_sync_config, small_repo_id, direction)
}

fn get_bookmark_renamers_from_config(
    common_config: &CommonCommitSyncConfig,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<BookmarkRenamers, Error> {
    let (direction, small_repo_id) =
        get_direction_and_small_repo_id(common_config, source_repo_id, target_repo_id)?;
    get_bookmark_renamers(common_config, small_repo_id, direction)
}

fn get_direction_and_small_repo_id(
    common_config: &CommonCommitSyncConfig,
    source_repo_id: RepositoryId,
    target_repo_id: RepositoryId,
) -> Result<(CommitSyncDirection, RepositoryId), Error> {
    let small_repo_id = if common_config.large_repo_id == source_repo_id
        && common_config.small_repos.contains_key(&target_repo_id)
    {
        target_repo_id
    } else if common_config.large_repo_id == target_repo_id
        && common_config.small_repos.contains_key(&source_repo_id)
    {
        source_repo_id
    } else {
        return Err(anyhow!(
            "CommitSyncMapping incompatible with source repo {:?} and target repo {:?}",
            source_repo_id,
            target_repo_id,
        ));
    };

    let direction = if source_repo_id == small_repo_id {
        CommitSyncDirection::SmallToLarge
    } else {
        CommitSyncDirection::LargeToSmall
    };

    Ok((direction, small_repo_id))
}
