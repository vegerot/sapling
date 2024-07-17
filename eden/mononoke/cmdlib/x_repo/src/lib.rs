/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Scaffolding that's generally useful to build CLI tools on top of Mononoke.

#![feature(trait_alias)]

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Error;
use anyhow::Result;
use blobstore_factory::MetadataSqlFactory;
use cacheblob::LeaseOps;
use cmdlib::args;
use cmdlib::args::MononokeMatches;
use context::CoreContext;
use cross_repo_sync::create_commit_syncer_lease;
use cross_repo_sync::create_commit_syncers;
use cross_repo_sync::CommitSyncRepos;
use cross_repo_sync::CommitSyncer;
use cross_repo_sync::Source;
use cross_repo_sync::SubmoduleDeps;
use cross_repo_sync::Syncers;
use cross_repo_sync::Target;
use futures_util::stream;
use futures_util::try_join;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use live_commit_sync_config::CfgrLiveCommitSyncConfig;
use live_commit_sync_config::LiveCommitSyncConfig;
use mononoke_types::NonRootMPath;
use mononoke_types::RepositoryId;
use pushredirect::SqlPushRedirectionConfigBuilder;
use sql_construct::SqlConstructFromMetadataDatabaseConfig;
use synced_commit_mapping::SqlSyncedCommitMapping;

pub trait Repo =
    cross_repo_sync::Repo + for<'b> facet::AsyncBuildable<'b, repo_factory::RepoFactoryBuilder<'b>>;

/// Instantiate the `Syncers` struct by parsing `matches`
pub async fn create_commit_syncers_from_matches<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    repo_pair: Option<(RepositoryId, RepositoryId)>,
) -> Result<Syncers<SqlSyncedCommitMapping, R>, Error> {
    let (source_repo, target_repo, mapping, live_commit_sync_config) =
        get_things_from_matches::<R>(ctx, matches, repo_pair).await?;

    let common_config =
        live_commit_sync_config.get_common_config(source_repo.0.repo_identity().id())?;

    let caching = matches.caching();
    let x_repo_syncer_lease = create_commit_syncer_lease(ctx.fb, caching)?;

    let large_repo_id = common_config.large_repo_id;
    let source_repo_id = source_repo.0.repo_identity().id();
    let target_repo_id = target_repo.0.repo_identity().id();
    let (small_repo, large_repo) = if large_repo_id == source_repo_id {
        (target_repo.0, source_repo.0)
    } else if large_repo_id == target_repo_id {
        (source_repo.0, target_repo.0)
    } else {
        bail!(
            "Unexpectedly CommitSyncConfig {:?} has neither of {}, {} as a large repo",
            common_config,
            source_repo_id,
            target_repo_id
        );
    };

    let submodule_deps = get_all_possible_small_repo_submodule_deps_from_matches(
        ctx,
        matches,
        &small_repo,
        live_commit_sync_config.clone(),
    )
    .await?;

    create_commit_syncers(
        ctx,
        small_repo,
        large_repo,
        submodule_deps,
        mapping,
        live_commit_sync_config,
        x_repo_syncer_lease,
    )
}

/// Instantiate the source-target `CommitSyncer` struct by parsing `matches`
pub async fn create_commit_syncer_from_matches<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    repo_pair: Option<(RepositoryId, RepositoryId)>,
) -> Result<CommitSyncer<SqlSyncedCommitMapping, R>, Error> {
    create_commit_syncer_from_matches_impl(ctx, matches, false /* reverse */, repo_pair).await
}

/// Instantiate the target-source `CommitSyncer` struct by parsing `matches`
pub async fn create_reverse_commit_syncer_from_matches<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    repo_pair: Option<(RepositoryId, RepositoryId)>,
) -> Result<CommitSyncer<SqlSyncedCommitMapping, R>, Error> {
    create_commit_syncer_from_matches_impl(ctx, matches, true /* reverse */, repo_pair).await
}

/// Instantiate some auxiliary things from `matches`
/// Naming is hard.
async fn get_things_from_matches<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    repo_pair: Option<(RepositoryId, RepositoryId)>,
) -> Result<
    (
        Source<R>,
        Target<R>,
        SqlSyncedCommitMapping,
        Arc<dyn LiveCommitSyncConfig>,
    ),
    Error,
> {
    let fb = ctx.fb;
    let logger = ctx.logger();

    let config_store = matches.config_store();
    let (source_repo_id, target_repo_id) = match repo_pair {
        Some((source_repo_id, target_repo_id)) => (source_repo_id, target_repo_id),
        None => (
            args::not_shardmanager_compatible::get_source_repo_id(config_store, matches)?,
            args::not_shardmanager_compatible::get_target_repo_id(config_store, matches)?,
        ),
    };

    let (_, source_repo_config) =
        args::get_config_by_repoid(config_store, matches, source_repo_id)?;
    let (_, target_repo_config) =
        args::get_config_by_repoid(config_store, matches, target_repo_id)?;

    if source_repo_config.storage_config.metadata != target_repo_config.storage_config.metadata {
        return Err(Error::msg(
            "source repo and target repo have different metadata database configs!",
        ));
    }

    let mysql_options = matches.mysql_options();
    let readonly_storage = matches.readonly_storage();

    let mapping = SqlSyncedCommitMapping::with_metadata_database_config(
        ctx.fb,
        &source_repo_config.storage_config.metadata,
        mysql_options,
        readonly_storage.0,
    )
    .await?;

    let source_repo_fut = args::open_repo_with_repo_id(fb, logger, source_repo_id, matches);
    let target_repo_fut = args::open_repo_with_repo_id(fb, logger, target_repo_id, matches);

    let (source_repo, target_repo) = try_join!(source_repo_fut, target_repo_fut)?;

    let sql_factory: MetadataSqlFactory = MetadataSqlFactory::new(
        ctx.fb,
        source_repo_config.storage_config.metadata,
        mysql_options.clone(),
        blobstore_factory::ReadOnlyStorage(readonly_storage.0),
    )
    .await?;
    let builder = sql_factory
        .open::<SqlPushRedirectionConfigBuilder>()
        .await?;
    let push_redirection_config = builder.build();

    let live_commit_sync_config: Arc<dyn LiveCommitSyncConfig> =
        Arc::new(CfgrLiveCommitSyncConfig::new_with_xdb(
            ctx.logger(),
            config_store,
            Arc::new(push_redirection_config),
        )?);

    Ok((
        Source(source_repo),
        Target(target_repo),
        mapping,
        live_commit_sync_config,
    ))
}

fn flip_direction<T>(source_item: Source<T>, target_item: Target<T>) -> (Source<T>, Target<T>) {
    (Source(target_item.0), Target(source_item.0))
}

async fn create_commit_syncer_from_matches_impl<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    reverse: bool,
    repo_pair: Option<(RepositoryId, RepositoryId)>,
) -> Result<CommitSyncer<SqlSyncedCommitMapping, R>, Error> {
    let (source_repo, target_repo, mapping, live_commit_sync_config): (Source<R>, Target<R>, _, _) =
        get_things_from_matches(ctx, matches, repo_pair).await?;

    let (source_repo, target_repo) = if reverse {
        flip_direction(source_repo, target_repo)
    } else {
        (source_repo, target_repo)
    };

    let caching = matches.caching();
    let x_repo_syncer_lease = create_commit_syncer_lease(ctx.fb, caching)?;
    let common_config =
        live_commit_sync_config.get_common_config(source_repo.0.repo_identity().id())?;

    let large_repo_id = common_config.large_repo_id;
    let source_repo_id = source_repo.0.repo_identity().id();
    let target_repo_id = target_repo.0.repo_identity().id();
    let small_repo = if large_repo_id == source_repo_id {
        target_repo.0.clone()
    } else if large_repo_id == target_repo_id {
        source_repo.0.clone()
    } else {
        bail!(
            "Unexpectedly CommitSyncConfig {:?} has neither of {}, {} as a large repo",
            common_config,
            source_repo_id,
            target_repo_id
        );
    };
    let submodule_deps = get_all_possible_small_repo_submodule_deps_from_matches(
        ctx,
        matches,
        &small_repo,
        live_commit_sync_config.clone(),
    )
    .await?;

    create_commit_syncer(
        ctx,
        source_repo,
        target_repo,
        submodule_deps,
        mapping,
        live_commit_sync_config,
        x_repo_syncer_lease,
    )
    .await
}

async fn create_commit_syncer<'a, R: Repo>(
    ctx: &'a CoreContext,
    source_repo: Source<R>,
    target_repo: Target<R>,
    submodule_deps: SubmoduleDeps<R>,
    mapping: SqlSyncedCommitMapping,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    x_repo_syncer_lease: Arc<dyn LeaseOps>,
) -> Result<CommitSyncer<SqlSyncedCommitMapping, R>, Error> {
    let common_config =
        live_commit_sync_config.get_common_config(source_repo.0.repo_identity().id())?;

    let repos = CommitSyncRepos::new(source_repo.0, target_repo.0, submodule_deps, &common_config)?;
    let commit_syncer = CommitSyncer::new(
        ctx,
        mapping,
        repos,
        live_commit_sync_config,
        x_repo_syncer_lease,
    );
    Ok(commit_syncer)
}

/// Loads the Mononoke repos from the git submodules that the small repo depends.
///
/// These repos need to be loaded in order to be able to sync commits from the
/// small repo that have git submodule changes to the large repo.
///
/// Since the dependencies might change for each version, this eagerly loads
/// the dependencies from all versions, to guarantee that if we sync a sligthly
/// older commit, its dependencies will be loaded.
pub async fn get_all_possible_small_repo_submodule_deps_from_matches<R: Repo>(
    ctx: &CoreContext,
    matches: &MononokeMatches<'_>,
    source_repo: &R,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
) -> Result<SubmoduleDeps<R>> {
    let source_repo_id = source_repo.repo_identity().id();

    let source_repo_sync_configs = live_commit_sync_config
        .get_all_commit_sync_config_versions(source_repo_id)
        .await?;

    let small_repo_deps_ids = source_repo_sync_configs
        .into_values()
        .filter_map(|mut cfg| {
            cfg.small_repos
                .remove(&source_repo_id)
                .map(|small_repo_cfg| small_repo_cfg.submodule_config.submodule_dependencies)
        })
        .flatten()
        .collect::<HashSet<_>>();

    let submodule_deps_to_load = small_repo_deps_ids.len();

    let submodule_deps_map: HashMap<NonRootMPath, Arc<R>> = stream::iter(small_repo_deps_ids)
        .then(|(submodule_path, repo_id)| async move {
            let repo =
                args::open_repo_by_id_unredacted(ctx.fb, ctx.logger(), matches, repo_id).await?;
            anyhow::Ok((submodule_path, Arc::new(repo)))
        })
        .try_collect()
        .await?;

    if submodule_deps_map.len() < submodule_deps_to_load {
        return Ok(SubmoduleDeps::NotAvailable);
    }

    Ok(SubmoduleDeps::ForSync(submodule_deps_map))
}
