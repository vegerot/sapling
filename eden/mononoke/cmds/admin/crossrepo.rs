/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::format_err;
use anyhow::Context;
use anyhow::Error;
use backsyncer::format_counter as format_backsyncer_counter;
use blobrepo::save_bonsai_changesets;
use blobstore::Loadable;
use bookmarks::BookmarkKey;
use bookmarks::BookmarkUpdateLogRef;
use bookmarks::BookmarkUpdateReason;
use bookmarks::BookmarksRef;
use bookmarks::Freshness;
use cached_config::ConfigStore;
use clap_old::App;
use clap_old::Arg;
use clap_old::ArgMatches;
use clap_old::SubCommand;
use cmdlib::args;
use cmdlib::args::MononokeMatches;
use cmdlib::helpers;
use context::CoreContext;
use cross_repo_sync::create_commit_syncer_lease;
use cross_repo_sync::create_commit_syncers;
use cross_repo_sync::types::Large;
use cross_repo_sync::types::Small;
use cross_repo_sync::validation;
use cross_repo_sync::validation::BookmarkDiff;
use cross_repo_sync::CommitSyncContext;
use cross_repo_sync::CommitSyncRepos;
use cross_repo_sync::CommitSyncer;
use cross_repo_sync::PluralCommitSyncOutcome;
use cross_repo_sync::Syncers;
use cross_repo_sync::CHANGE_XREPO_MAPPING_EXTRA;
use derived_data_utils::derive_all_enabled_datatypes_for_csids;
use fbinit::FacebookInit;
use filestore::FilestoreConfigRef;
use futures::stream;
use futures::try_join;
use futures::TryFutureExt;
use itertools::Itertools;
use live_commit_sync_config::CfgrLiveCommitSyncConfig;
use live_commit_sync_config::LiveCommitSyncConfig;
use maplit::btreemap;
use maplit::hashmap;
use maplit::hashset;
use metaconfig_types::CommitSyncConfig;
use metaconfig_types::CommitSyncConfigVersion;
use metaconfig_types::CommonCommitSyncConfig;
use metaconfig_types::DefaultSmallToLargeCommitSyncPathAction;
use mononoke_api_types::InnerRepo;
use mononoke_types::BonsaiChangesetMut;
use mononoke_types::ChangesetId;
use mononoke_types::DateTime;
use mononoke_types::FileChange;
use mononoke_types::FileType;
use mononoke_types::NonRootMPath;
use mononoke_types::RepositoryId;
use mutable_counters::MutableCountersRef;
use pushrebase::do_pushrebase_bonsai;
use pushrebase::FAIL_PUSHREBASE_EXTRA;
use repo_blobstore::RepoBlobstoreRef;
use repo_identity::RepoIdentityRef;
use slog::info;
use slog::warn;
use slog::Logger;
use sorted_vector_map::sorted_vector_map;
use synced_commit_mapping::EquivalentWorkingCopyEntry;
use synced_commit_mapping::SqlSyncedCommitMapping;
use synced_commit_mapping::SyncedCommitMapping;
use synced_commit_mapping::SyncedCommitMappingEntry;

use crate::common::get_source_target_repos_and_mapping;
use crate::error::SubcommandError;

pub const CROSSREPO: &str = "crossrepo";
const AUTHOR_ARG: &str = "author";
const ONCALL_ARG: &str = "oncall";
const DUMP_MAPPING_LARGE_REPO_PATH_ARG: &str = "dump-mapping-large-repo-path";
const MAP_SUBCOMMAND: &str = "map";
const PREPARE_ROLLOUT_SUBCOMMAND: &str = "prepare-rollout";
const PUSHREDIRECTION_SUBCOMMAND: &str = "pushredirection";
const VERIFY_WC_SUBCOMMAND: &str = "verify-wc";
const VERIFY_BOOKMARKS_SUBCOMMAND: &str = "verify-bookmarks";
const HASH_ARG: &str = "HASH";
const LARGE_REPO_HASH_ARG: &str = "large-repo-hash";
const UPDATE_LARGE_REPO_BOOKMARKS: &str = "update-large-repo-bookmarks";
const LIMIT_ARG: &str = "limit";
const NO_BOOKMARK_UPDATES: &str = "no-bookmark-updates";
const LARGE_REPO_BOOKMARK_ARG: &str = "large-repo-bookmark";
const CHANGE_MAPPING_VERSION_SUBCOMMAND: &str = "change-mapping-version";
const INSERT_SUBCOMMAND: &str = "insert";
const REWRITTEN_SUBCOMMAND: &str = "rewritten";
const EQUIVALENT_WORKING_COPY_SUBCOMMAND: &str = "equivalent-working-copy";
const NOT_SYNC_CANDIDATE_SUBCOMMAND: &str = "not-sync-candidate";
const SOURCE_HASH_ARG: &str = "source-hash";
const TARGET_HASH_ARG: &str = "target-hash";
const VIA_EXTRAS_ARG: &str = "via-extra";

const SUBCOMMAND_CONFIG: &str = "config";
const SUBCOMMAND_BY_VERSION: &str = "by-version";
const SUBCOMMAND_LIST: &str = "list";
const ARG_VERSION_NAME: &str = "version-name";
const ARG_WITH_CONTENTS: &str = "with-contents";

// TODO: Move to its own repo type to deblobrepoify
type CrossRepo = InnerRepo;

#[derive(Debug, Clone, Copy, PartialEq)]
enum UpdateLargeRepoBookmarksMode {
    Real,
    DryRun,
}

#[derive(Debug, Clone, PartialEq)]
enum VerifyRunMode {
    JustVerify,
    UpdateLargeRepoBookmarks {
        limit: Option<usize>,
        mode: UpdateLargeRepoBookmarksMode,
    },
}

pub async fn subcommand_crossrepo<'a>(
    fb: FacebookInit,
    logger: Logger,
    matches: &'a MononokeMatches<'_>,
    sub_m: &'a ArgMatches<'_>,
) -> Result<(), SubcommandError> {
    let config_store = matches.config_store();
    let live_commit_sync_config = CfgrLiveCommitSyncConfig::new(&logger, config_store)?;

    let ctx = CoreContext::new_with_logger(fb, logger.clone());
    match sub_m.subcommand() {
        (MAP_SUBCOMMAND, Some(sub_sub_m)) => {
            let (source_repo, target_repo, mapping) =
                get_source_target_repos_and_mapping::<CrossRepo>(fb, logger, matches).await?;

            let common_config =
                live_commit_sync_config.get_common_config(source_repo.repo_identity().id())?;
            let commit_sync_repos = CommitSyncRepos::new(source_repo, target_repo, &common_config)?;
            let live_commit_sync_config: Arc<dyn LiveCommitSyncConfig> =
                Arc::new(live_commit_sync_config);

            let caching = matches.caching();
            let x_repo_syncer_lease = create_commit_syncer_lease(ctx.fb, caching)?;

            let commit_syncer = CommitSyncer::new(
                &ctx,
                mapping,
                commit_sync_repos,
                live_commit_sync_config,
                x_repo_syncer_lease,
            );
            let hash = sub_sub_m.value_of(HASH_ARG).unwrap().to_owned();
            subcommand_map(ctx, commit_syncer, hash).await
        }
        (VERIFY_WC_SUBCOMMAND, Some(sub_sub_m)) => {
            let (source_repo, target_repo, mapping) =
                get_source_target_repos_and_mapping(fb, logger, matches).await?;

            let live_commit_sync_config: Arc<dyn LiveCommitSyncConfig> =
                Arc::new(live_commit_sync_config);
            let commit_syncer = get_large_to_small_commit_syncer(
                &ctx,
                source_repo,
                target_repo,
                live_commit_sync_config.clone(),
                mapping,
                matches,
            )
            .await?;

            let large_hash = {
                let large_hash = sub_sub_m.value_of(LARGE_REPO_HASH_ARG).unwrap().to_owned();
                let large_repo = commit_syncer.get_large_repo();
                helpers::csid_resolve(&ctx, large_repo, large_hash).await?
            };

            validation::verify_working_copy_fast_path(
                &ctx,
                &commit_syncer,
                large_hash,
                live_commit_sync_config,
            )
            .await
            .map_err(|e| e.into())
        }
        (VERIFY_BOOKMARKS_SUBCOMMAND, Some(sub_sub_m)) => {
            let (source_repo, target_repo, mapping) =
                get_source_target_repos_and_mapping(fb, logger, matches).await?;

            let mode = if sub_sub_m.is_present(UPDATE_LARGE_REPO_BOOKMARKS) {
                VerifyRunMode::UpdateLargeRepoBookmarks {
                    mode: if sub_sub_m.is_present(NO_BOOKMARK_UPDATES) {
                        UpdateLargeRepoBookmarksMode::DryRun
                    } else {
                        UpdateLargeRepoBookmarksMode::Real
                    },
                    limit: sub_sub_m
                        .value_of(LIMIT_ARG)
                        .map(str::parse::<usize>)
                        .transpose()
                        .map_err(anyhow::Error::msg)?,
                }
            } else {
                VerifyRunMode::JustVerify
            };

            subcommand_verify_bookmarks(
                ctx,
                source_repo,
                target_repo,
                mapping,
                mode,
                Arc::new(live_commit_sync_config),
                matches,
            )
            .await
        }
        (SUBCOMMAND_CONFIG, Some(sub_sub_m)) => {
            run_config_sub_subcommand(matches, sub_sub_m, live_commit_sync_config).await
        }
        (PUSHREDIRECTION_SUBCOMMAND, Some(sub_sub_m)) => {
            run_pushredirection_subcommand(
                fb,
                ctx,
                matches,
                sub_sub_m,
                config_store,
                live_commit_sync_config,
            )
            .await
        }
        (INSERT_SUBCOMMAND, Some(sub_sub_m)) => {
            run_insert_subcommand(ctx, matches, sub_sub_m, live_commit_sync_config).await
        }
        _ => Err(SubcommandError::InvalidArgs),
    }
}

async fn run_config_sub_subcommand<'a>(
    matches: &'a MononokeMatches<'_>,
    config_subcommand_matches: &'a ArgMatches<'a>,
    live_commit_sync_config: CfgrLiveCommitSyncConfig,
) -> Result<(), SubcommandError> {
    let config_store = matches.config_store();

    let repo_id = args::not_shardmanager_compatible::get_repo_id(config_store, matches)?;

    match config_subcommand_matches.subcommand() {
        (SUBCOMMAND_BY_VERSION, Some(sub_m)) => {
            let version_name: String = sub_m.value_of(ARG_VERSION_NAME).unwrap().to_string();
            subcommand_by_version(repo_id, live_commit_sync_config, version_name)
                .await
                .map_err(|e| e.into())
        }
        (SUBCOMMAND_LIST, Some(sub_m)) => {
            let with_contents = sub_m.is_present(ARG_WITH_CONTENTS);
            subcommand_list(repo_id, live_commit_sync_config, with_contents)
                .await
                .map_err(|e| e.into())
        }
        _ => Err(SubcommandError::InvalidArgs),
    }
}

async fn run_pushredirection_subcommand<'a>(
    fb: FacebookInit,
    ctx: CoreContext,
    matches: &'a MononokeMatches<'_>,
    config_subcommand_matches: &'a ArgMatches<'a>,
    config_store: &ConfigStore,
    live_commit_sync_config: CfgrLiveCommitSyncConfig,
) -> Result<(), SubcommandError> {
    let (source_repo, target_repo, mapping) =
        get_source_target_repos_and_mapping(fb, ctx.logger().clone(), matches).await?;

    let live_commit_sync_config: Arc<dyn LiveCommitSyncConfig> = Arc::new(live_commit_sync_config);

    match config_subcommand_matches.subcommand() {
        (PREPARE_ROLLOUT_SUBCOMMAND, Some(_sub_m)) => {
            let commit_syncer = get_large_to_small_commit_syncer(
                &ctx,
                source_repo,
                target_repo,
                live_commit_sync_config.clone(),
                mapping,
                matches,
            )
            .await?;

            if live_commit_sync_config.push_redirector_enabled_for_public(
                commit_syncer.get_small_repo().repo_identity().id(),
            ) {
                return Err(format_err!(
                    "not allowed to run {} if pushredirection is enabled",
                    PREPARE_ROLLOUT_SUBCOMMAND
                )
                .into());
            }

            let small_repo = commit_syncer.get_small_repo();
            let large_repo = commit_syncer.get_large_repo();
            let largest_id = large_repo
                .bookmark_update_log()
                .get_largest_log_id(ctx.clone(), Freshness::MostRecent)
                .await?
                .ok_or_else(|| anyhow!("No bookmarks update log entries for large repo"))?;

            let counter = format_backsyncer_counter(&large_repo.repo_identity().id());
            info!(
                ctx.logger(),
                "setting value {} to counter {} for repo {}",
                largest_id,
                counter,
                small_repo.repo_identity().id()
            );
            let res = small_repo
                .mutable_counters()
                .set_counter(
                    &ctx,
                    &counter,
                    largest_id.try_into().unwrap(),
                    None, // prev_value
                )
                .await?;

            if !res {
                return Err(anyhow!("failed to set backsyncer counter").into());
            }
            info!(ctx.logger(), "successfully updated the counter");

            Ok(())
        }
        (CHANGE_MAPPING_VERSION_SUBCOMMAND, Some(sub_m)) => {
            let commit_syncer = get_large_to_small_commit_syncer(
                &ctx,
                source_repo,
                target_repo,
                live_commit_sync_config.clone(),
                mapping,
                matches,
            )
            .await?;

            if sub_m.is_present(VIA_EXTRAS_ARG) {
                change_mapping_via_extras(
                    &ctx,
                    matches,
                    sub_m,
                    &commit_syncer,
                    config_store,
                    &live_commit_sync_config,
                )
                .await?;
                return Ok(());
            }

            if live_commit_sync_config.push_redirector_enabled_for_public(
                commit_syncer.get_small_repo().repo_identity().id(),
            ) {
                return Err(format_err!(
                    "not allowed to run {} if pushredirection is enabled",
                    CHANGE_MAPPING_VERSION_SUBCOMMAND
                )
                .into());
            }

            let large_bookmark = Large(
                sub_m
                    .value_of(LARGE_REPO_BOOKMARK_ARG)
                    .map(BookmarkKey::new)
                    .transpose()?
                    .ok_or_else(|| format_err!("{} is not specified", LARGE_REPO_BOOKMARK_ARG))?,
            );
            let small_bookmark = Small(
                commit_syncer.get_bookmark_renamer().await?(&large_bookmark).ok_or_else(|| {
                    format_err!("{} bookmark doesn't remap to small repo", large_bookmark)
                })?,
            );

            let large_repo = Large(commit_syncer.get_large_repo());
            let small_repo = Small(commit_syncer.get_small_repo());
            let large_bookmark_value =
                Large(get_bookmark_value(&ctx, &large_repo, &large_bookmark).await?);
            let small_bookmark_value =
                Small(get_bookmark_value(&ctx, &small_repo, &small_bookmark).await?);

            let mapping_version = sub_m
                .value_of(ARG_VERSION_NAME)
                .ok_or_else(|| format_err!("{} is not specified", ARG_VERSION_NAME))?;
            let mapping_version = CommitSyncConfigVersion(mapping_version.to_string());
            if !commit_syncer.version_exists(&mapping_version).await? {
                return Err(format_err!("{} version does not exist", mapping_version).into());
            }

            let dump_mapping_file = sub_m
                .value_of(DUMP_MAPPING_LARGE_REPO_PATH_ARG)
                .map(NonRootMPath::new)
                .transpose()?;

            let large_cs_id = create_commit_for_mapping_change(
                &ctx,
                sub_m,
                &large_repo,
                &small_repo,
                &large_bookmark_value,
                &mapping_version,
                MappingCommitOptions {
                    add_mapping_change_extra: false,
                    dump_mapping_file,
                },
                &commit_syncer,
                &live_commit_sync_config,
            )
            .await?;

            let maybe_rewritten_small_cs_id = commit_syncer
                .unsafe_always_rewrite_sync_commit(
                    &ctx,
                    large_cs_id.0,
                    Some(hashmap! {
                      large_bookmark_value.0.clone() => small_bookmark_value.0.clone(),
                    }),
                    &mapping_version,
                    CommitSyncContext::AdminChangeMapping,
                )
                .await?;

            let rewritten_small_cs_id = Small(maybe_rewritten_small_cs_id.ok_or_else(|| {
                format_err!("{} was rewritten into non-existent commit", large_cs_id)
            })?);

            let f1 = move_bookmark(
                &ctx,
                &large_repo,
                &large_bookmark,
                *large_bookmark_value,
                *large_cs_id,
            );

            let f2 = move_bookmark(
                &ctx,
                &small_repo,
                &small_bookmark,
                *small_bookmark_value,
                *rewritten_small_cs_id,
            );

            try_join!(f1, f2)?;

            Ok(())
        }
        _ => Err(SubcommandError::InvalidArgs),
    }
}

async fn change_mapping_via_extras<'a>(
    ctx: &CoreContext,
    matches: &'a MononokeMatches<'a>,
    sub_m: &'a ArgMatches<'a>,
    commit_syncer: &'a CommitSyncer<SqlSyncedCommitMapping, CrossRepo>,
    config_store: &ConfigStore,
    live_commit_sync_config: &Arc<dyn LiveCommitSyncConfig>,
) -> Result<(), Error> {
    if !live_commit_sync_config
        .push_redirector_enabled_for_public(commit_syncer.get_small_repo().repo_identity().id())
    {
        return Err(format_err!(
            "not allowed to run {} if pushredirection is not enabled",
            CHANGE_MAPPING_VERSION_SUBCOMMAND
        ));
    }

    let small_repo = commit_syncer.get_small_repo();
    let large_repo = commit_syncer.get_large_repo();

    let (_, repo_config) =
        args::get_config_by_repoid(config_store, matches, large_repo.repo_identity().id())?;

    let large_bookmark = Large(
        sub_m
            .value_of(LARGE_REPO_BOOKMARK_ARG)
            .map(BookmarkKey::new)
            .transpose()?
            .ok_or_else(|| format_err!("{} is not specified", LARGE_REPO_BOOKMARK_ARG))?,
    );
    let large_bookmark_value = Large(get_bookmark_value(ctx, large_repo, &large_bookmark).await?);

    let mapping_version = sub_m
        .value_of(ARG_VERSION_NAME)
        .ok_or_else(|| format_err!("{} is not specified", ARG_VERSION_NAME))?;
    let mapping_version = CommitSyncConfigVersion(mapping_version.to_string());
    if !commit_syncer.version_exists(&mapping_version).await? {
        return Err(format_err!("{} version does not exist", mapping_version));
    }

    let dump_mapping_file = sub_m
        .value_of(DUMP_MAPPING_LARGE_REPO_PATH_ARG)
        .map(NonRootMPath::new)
        .transpose()?;
    let large_cs_id = create_commit_for_mapping_change(
        ctx,
        sub_m,
        &Large(large_repo),
        &Small(small_repo),
        &large_bookmark_value,
        &mapping_version,
        MappingCommitOptions {
            add_mapping_change_extra: true,
            dump_mapping_file,
        },
        commit_syncer,
        live_commit_sync_config,
    )
    .await?;

    let pushrebase_flags = &repo_config.pushrebase.flags;
    let pushrebase_hooks = bookmarks_movement::get_pushrebase_hooks(
        ctx,
        large_repo,
        &large_bookmark,
        &repo_config.pushrebase,
    )?;

    let bcs = large_cs_id
        .load(ctx, &large_repo.repo_blobstore().clone())
        .map_err(Error::from)
        .await?;
    let pushrebase_res = do_pushrebase_bonsai(
        ctx,
        large_repo,
        pushrebase_flags,
        &large_bookmark,
        &hashset![bcs],
        &pushrebase_hooks,
    )
    .map_err(Error::from)
    .await?;

    println!("{}", pushrebase_res.head);

    Ok(())
}

async fn run_insert_subcommand<'a>(
    ctx: CoreContext,
    matches: &'a MononokeMatches<'_>,
    insert_subcommand_matches: &'a ArgMatches<'a>,
    live_commit_sync_config: CfgrLiveCommitSyncConfig,
) -> Result<(), SubcommandError> {
    let (source_repo, target_repo, mapping) =
        get_source_target_repos_and_mapping::<CrossRepo>(ctx.fb, ctx.logger().clone(), matches)
            .await?;

    let live_commit_sync_config: Arc<dyn LiveCommitSyncConfig> = Arc::new(live_commit_sync_config);
    let commit_syncer = get_large_to_small_commit_syncer(
        &ctx,
        source_repo.clone(),
        target_repo.clone(),
        live_commit_sync_config.clone(),
        mapping.clone(),
        matches,
    )
    .await?;

    match insert_subcommand_matches.subcommand() {
        (REWRITTEN_SUBCOMMAND, Some(sub_m)) => {
            let (source_cs_id, target_cs_id, mapping_version) =
                get_source_target_cs_ids_and_version(&ctx, sub_m, &commit_syncer).await?;
            let small_repo_id = commit_syncer.get_small_repo().repo_identity().id();
            let large_repo_id = commit_syncer.get_large_repo().repo_identity().id();

            let mapping_entry = if small_repo_id == source_repo.repo_identity().id() {
                SyncedCommitMappingEntry {
                    large_repo_id,
                    small_repo_id,
                    small_bcs_id: source_cs_id,
                    large_bcs_id: target_cs_id,
                    version_name: Some(mapping_version),
                    source_repo: Some(commit_syncer.get_source_repo_type()),
                }
            } else {
                SyncedCommitMappingEntry {
                    large_repo_id,
                    small_repo_id,
                    small_bcs_id: target_cs_id,
                    large_bcs_id: source_cs_id,
                    version_name: Some(mapping_version),
                    source_repo: Some(commit_syncer.get_source_repo_type()),
                }
            };

            let res = mapping.add(&ctx, mapping_entry).await?;
            if res {
                info!(
                    ctx.logger(),
                    "successfully inserted rewritten mapping entry"
                );
                Ok(())
            } else {
                Err(anyhow!("failed to insert entry").into())
            }
        }
        (EQUIVALENT_WORKING_COPY_SUBCOMMAND, Some(sub_m)) => {
            let (source_cs_id, target_cs_id, mapping_version) =
                get_source_target_cs_ids_and_version(&ctx, sub_m, &commit_syncer).await?;
            let small_repo_id = commit_syncer.get_small_repo().repo_identity().id();
            let large_repo_id = commit_syncer.get_large_repo().repo_identity().id();

            let mapping_entry = if small_repo_id == source_repo.repo_identity().id() {
                EquivalentWorkingCopyEntry {
                    large_repo_id,
                    small_repo_id,
                    small_bcs_id: Some(source_cs_id),
                    large_bcs_id: target_cs_id,
                    version_name: Some(mapping_version),
                }
            } else {
                EquivalentWorkingCopyEntry {
                    large_repo_id,
                    small_repo_id,
                    small_bcs_id: Some(target_cs_id),
                    large_bcs_id: source_cs_id,
                    version_name: Some(mapping_version),
                }
            };

            let res = mapping
                .insert_equivalent_working_copy(&ctx, mapping_entry)
                .await?;
            if res {
                info!(
                    ctx.logger(),
                    "successfully inserted equivalent working copy"
                );
                Ok(())
            } else {
                Err(anyhow!("failed to insert entry").into())
            }
        }
        (NOT_SYNC_CANDIDATE_SUBCOMMAND, Some(sub_m)) => {
            let large_repo = commit_syncer.get_large_repo();
            let large_repo_hash = sub_m
                .value_of(LARGE_REPO_HASH_ARG)
                .ok_or_else(|| anyhow!("{} is not specified", LARGE_REPO_HASH_ARG))?;
            let large_repo_cs_id = helpers::csid_resolve(&ctx, large_repo, large_repo_hash).await?;

            let small_repo_id = commit_syncer.get_small_repo().repo_identity().id();
            let large_repo_id = commit_syncer.get_large_repo().repo_identity().id();

            let maybe_mapping_version = sub_m.value_of(ARG_VERSION_NAME);
            let maybe_mapping_version = match maybe_mapping_version {
                Some(mapping_version) => {
                    let mapping_version = CommitSyncConfigVersion(mapping_version.to_string());
                    if !commit_syncer.version_exists(&mapping_version).await? {
                        return Err(
                            format_err!("{} version does not exist", mapping_version).into()
                        );
                    }
                    Some(mapping_version)
                }
                None => None,
            };

            let mapping_entry = EquivalentWorkingCopyEntry {
                large_repo_id,
                small_repo_id,
                small_bcs_id: None,
                large_bcs_id: large_repo_cs_id,
                version_name: maybe_mapping_version,
            };

            let res = mapping
                .insert_equivalent_working_copy(&ctx, mapping_entry)
                .await?;
            if res {
                info!(
                    ctx.logger(),
                    "successfully inserted not sync candidate entry"
                );
                Ok(())
            } else {
                Err(anyhow!("failed to insert entry").into())
            }
        }
        _ => Err(SubcommandError::InvalidArgs),
    }
}

async fn get_source_target_cs_ids_and_version(
    ctx: &CoreContext,
    sub_m: &ArgMatches<'_>,
    commit_syncer: &CommitSyncer<SqlSyncedCommitMapping, CrossRepo>,
) -> Result<(ChangesetId, ChangesetId, CommitSyncConfigVersion), Error> {
    async fn fetch_cs_id(
        ctx: &CoreContext,
        sub_m: &ArgMatches<'_>,
        repo: &CrossRepo,
        arg: &str,
    ) -> Result<ChangesetId, Error> {
        let hash = sub_m
            .value_of(arg)
            .ok_or_else(|| anyhow!("{} is not specified", arg))?;
        helpers::csid_resolve(ctx, repo, hash).await
    }

    let source_cs_id = fetch_cs_id(ctx, sub_m, commit_syncer.get_source_repo(), SOURCE_HASH_ARG);
    let target_cs_id = fetch_cs_id(ctx, sub_m, commit_syncer.get_target_repo(), TARGET_HASH_ARG);

    let (source_cs_id, target_cs_id) = try_join!(source_cs_id, target_cs_id)?;
    let mapping_version = sub_m
        .value_of(ARG_VERSION_NAME)
        .ok_or_else(|| format_err!("{} is not specified", ARG_VERSION_NAME))?;

    let mapping_version = CommitSyncConfigVersion(mapping_version.to_string());
    if !commit_syncer.version_exists(&mapping_version).await? {
        return Err(format_err!("{} version does not exist", mapping_version));
    }

    Ok((source_cs_id, target_cs_id, mapping_version))
}

struct MappingCommitOptions {
    add_mapping_change_extra: bool,
    dump_mapping_file: Option<NonRootMPath>,
}

async fn create_commit_for_mapping_change(
    ctx: &CoreContext,
    sub_m: &ArgMatches<'_>,
    large_repo: &Large<&CrossRepo>,
    small_repo: &Small<&CrossRepo>,
    parent: &Large<ChangesetId>,
    mapping_version: &CommitSyncConfigVersion,
    options: MappingCommitOptions,
    commit_syncer: &CommitSyncer<SqlSyncedCommitMapping, CrossRepo>,
    live_commit_sync_config: &Arc<dyn LiveCommitSyncConfig>,
) -> Result<Large<ChangesetId>, Error> {
    let author = sub_m
        .value_of(AUTHOR_ARG)
        .ok_or_else(|| format_err!("{} is not specified", AUTHOR_ARG))?;

    let oncall = sub_m.value_of(ONCALL_ARG);
    let oncall_msg_part = oncall.map(|o| format!("\n\nOncall Short Name: {}\n", o));

    let commit_msg = format!(
        "Changing synced mapping version to {} for {}->{} sync{}",
        mapping_version,
        large_repo.repo_identity().name(),
        small_repo.repo_identity().name(),
        oncall_msg_part.as_deref().unwrap_or("")
    );

    let mut extras = sorted_vector_map! {
        FAIL_PUSHREBASE_EXTRA.to_string() => b"1".to_vec(),
    };
    if options.add_mapping_change_extra {
        extras.insert(
            CHANGE_XREPO_MAPPING_EXTRA.to_string(),
            mapping_version.0.clone().into_bytes(),
        );
    }

    let file_changes = create_file_changes(
        ctx,
        small_repo,
        large_repo,
        mapping_version,
        options,
        commit_syncer,
        live_commit_sync_config,
    )
    .await?;

    // Create an empty commit on top of large bookmark
    let bcs = BonsaiChangesetMut {
        parents: vec![parent.0.clone()],
        author: author.to_string(),
        author_date: DateTime::now(),
        committer: None,
        committer_date: None,
        message: commit_msg,
        hg_extra: extras,
        git_extra_headers: None,
        git_tree_hash: None,
        file_changes: file_changes.into(),
        is_snapshot: false,
        git_annotated_tag: None,
    }
    .freeze()?;

    let large_cs_id = bcs.get_changeset_id();
    save_bonsai_changesets(vec![bcs], ctx.clone(), &large_repo.0).await?;

    Ok(Large(large_cs_id))
}

async fn create_file_changes(
    ctx: &CoreContext,
    small_repo: &Small<&CrossRepo>,
    large_repo: &Large<&CrossRepo>,
    mapping_version: &CommitSyncConfigVersion,
    options: MappingCommitOptions,
    commit_syncer: &CommitSyncer<SqlSyncedCommitMapping, CrossRepo>,
    live_commit_sync_config: &Arc<dyn LiveCommitSyncConfig>,
) -> Result<BTreeMap<NonRootMPath, FileChange>, Error> {
    let mut file_changes = btreemap! {};
    if let Some(path) = options.dump_mapping_file {
        // This "dump-mapping-file" is going to be created in the large repo,
        // but this file needs to rewrite to a small repo as well. If it doesn't
        // rewrite to a small repo, then the whole mapping change commit isn't
        // going to exist in the small repo.

        let mover = if commit_syncer.get_source_repo().repo_identity().id()
            == large_repo.repo_identity().id()
        {
            commit_syncer.get_mover_by_version(mapping_version).await?
        } else {
            commit_syncer
                .get_reverse_mover_by_version(mapping_version)
                .await?
        };

        if mover(&path)?.is_none() {
            return Err(anyhow!(
                "cannot dump mapping to a file because path doesn't rewrite to a small repo"
            ));
        }

        // Now get the mapping and create json with it
        let commit_sync_config = live_commit_sync_config
            .get_commit_sync_config_by_version(large_repo.repo_identity().id(), mapping_version)
            .await?;

        let small_repo_sync_config = commit_sync_config
            .small_repos
            .get(&small_repo.repo_identity().id())
            .ok_or_else(|| {
                format_err!(
                    "small repo {} not found in {} mapping",
                    small_repo.repo_identity().id(),
                    mapping_version
                )
            })?;

        let default_prefix = match &small_repo_sync_config.default_action {
            DefaultSmallToLargeCommitSyncPathAction::Preserve => String::new(),
            DefaultSmallToLargeCommitSyncPathAction::PrependPrefix(prefix) => prefix.to_string(),
        };

        let mut map = serde_json::Map::new();
        map.insert("default_prefix".to_string(), default_prefix.into());
        let mut map_overrides = serde_json::Map::new();
        for (key, value) in &small_repo_sync_config.map {
            map_overrides.insert(key.to_string(), value.to_string().into());
        }
        map.insert("overrides".to_string(), map_overrides.into());

        let content = (get_generated_string() + &serde_json::to_string_pretty(&map)?).into_bytes();
        let content = bytes::Bytes::from(content);
        let size = content.len() as u64;
        let content_metadata = filestore::store(
            large_repo.repo_blobstore(),
            *large_repo.filestore_config(),
            ctx,
            &filestore::StoreRequest::new(size),
            stream::once(async move { Ok(content) }),
        )
        .await?;

        let file_change =
            FileChange::tracked(content_metadata.content_id, FileType::Regular, size, None);

        file_changes.insert(path, file_change);
    }

    Ok(file_changes)
}

// Mark content as (at)generated to discourage people from modifying it
// manually.
// However split this so that this source file is not marked as generated
fn get_generated_string() -> String {
    "\x40generated by the megarepo bind, reach out to Source Control @ FB with any questions\n"
        .to_owned()
}

async fn get_bookmark_value(
    ctx: &CoreContext,
    repo: &CrossRepo,
    bookmark: &BookmarkKey,
) -> Result<ChangesetId, Error> {
    let maybe_bookmark_value = repo.bookmarks().get(ctx.clone(), bookmark).await?;

    maybe_bookmark_value.ok_or_else(|| {
        format_err!(
            "{} is not found in {}",
            bookmark,
            repo.repo_identity().name()
        )
    })
}

async fn move_bookmark(
    ctx: &CoreContext,
    repo: &CrossRepo,
    bookmark: &BookmarkKey,
    prev_value: ChangesetId,
    new_value: ChangesetId,
) -> Result<(), Error> {
    let mut book_txn = repo.bookmarks().create_transaction(ctx.clone());

    info!(
        ctx.logger(),
        "moving {} to {} in {}",
        bookmark,
        new_value,
        repo.repo_identity().name()
    );
    book_txn.update(
        bookmark,
        new_value,
        prev_value,
        BookmarkUpdateReason::ManualMove,
    )?;

    let res = book_txn.commit().await?;

    if res {
        Ok(())
    } else {
        Err(format_err!(
            "failed to move bookmark {} in {}",
            bookmark,
            repo.repo_identity().name()
        ))
    }
}

fn print_commit_sync_config(csc: CommitSyncConfig, line_prefix: &str) {
    println!("{}large repo: {}", line_prefix, csc.large_repo_id);
    println!(
        "{}common pushrebase bookmarks: {:?}",
        line_prefix, csc.common_pushrebase_bookmarks
    );
    println!("{}version name: {}", line_prefix, csc.version_name);
    for (small_repo_id, small_repo_config) in csc
        .small_repos
        .into_iter()
        .sorted_by_key(|(small_repo_id, _)| *small_repo_id)
    {
        println!("{}  small repo: {}", line_prefix, small_repo_id);
        println!(
            "{}  default action: {:?}",
            line_prefix, small_repo_config.default_action
        );
        println!("{}  prefix map:", line_prefix);
        for (from, to) in small_repo_config
            .map
            .into_iter()
            .sorted_by_key(|(from, _)| from.clone())
        {
            println!("{}    {}->{}", line_prefix, from, to);
        }
    }
}

async fn subcommand_list<'a, L: LiveCommitSyncConfig>(
    repo_id: RepositoryId,
    live_commit_sync_config: L,
    with_contents: bool,
) -> Result<(), Error> {
    let all = live_commit_sync_config
        .get_all_commit_sync_config_versions(repo_id)
        .await?;
    for (version_name, csc) in all.into_iter().sorted_by_key(|(vn, _)| vn.clone()) {
        if with_contents {
            println!("{}:", version_name);
            print_commit_sync_config(csc, "  ");
            println!("\n");
        } else {
            println!("{}", version_name);
        }
    }

    Ok(())
}

async fn subcommand_by_version<'a, L: LiveCommitSyncConfig>(
    repo_id: RepositoryId,
    live_commit_sync_config: L,
    version_name: String,
) -> Result<(), Error> {
    let csc = live_commit_sync_config
        .get_commit_sync_config_by_version(repo_id, &CommitSyncConfigVersion(version_name))
        .await?;
    print_commit_sync_config(csc, "");
    Ok(())
}

async fn subcommand_map(
    ctx: CoreContext,
    commit_syncer: CommitSyncer<SqlSyncedCommitMapping, CrossRepo>,
    hash: String,
) -> Result<(), SubcommandError> {
    let source_repo = commit_syncer.get_source_repo();
    let source_cs_id = helpers::csid_resolve(&ctx, source_repo, &hash).await?;

    let plural_commit_sync_outcome = commit_syncer
        .get_plural_commit_sync_outcome(&ctx, source_cs_id)
        .await?;
    match plural_commit_sync_outcome {
        Some(plural_commit_sync_outcome) => {
            println!("{:?}", plural_commit_sync_outcome);
        }
        None => {
            println!("{} is not remapped", hash);
        }
    }

    Ok(())
}

async fn subcommand_verify_bookmarks(
    ctx: CoreContext,
    source_repo: CrossRepo,
    target_repo: CrossRepo,
    mapping: SqlSyncedCommitMapping,
    run_mode: VerifyRunMode,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    matches: &MononokeMatches<'_>,
) -> Result<(), SubcommandError> {
    let common_config =
        live_commit_sync_config.get_common_config(target_repo.repo_identity().id())?;
    let syncers = get_syncers(
        &ctx,
        source_repo.clone(),
        target_repo.clone(),
        live_commit_sync_config,
        mapping.clone(),
        matches,
    )
    .await?;

    let diff = validation::find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;

    if diff.is_empty() {
        info!(ctx.logger(), "all is well!");
        return Ok(());
    }

    match run_mode {
        VerifyRunMode::UpdateLargeRepoBookmarks { mode, limit } => {
            update_large_repo_bookmarks(ctx.clone(), &diff, &syncers, &common_config, mode, limit)
                .await?;
            Ok(())
        }
        VerifyRunMode::JustVerify => {
            for d in &diff {
                use BookmarkDiff::*;
                match d {
                    InconsistentValue {
                        target_bookmark,
                        target_cs_id,
                        source_cs_id,
                    } => {
                        warn!(
                            ctx.logger(),
                            "inconsistent value of {}: '{}' has {}, but '{}' bookmark points to {:?}",
                            target_bookmark,
                            target_repo.repo_identity().name(),
                            target_cs_id,
                            source_repo.repo_identity().name(),
                            source_cs_id,
                        );
                    }
                    MissingInTarget {
                        target_bookmark,
                        source_cs_id,
                    } => {
                        warn!(
                            ctx.logger(),
                            "'{}' doesn't have bookmark {} but '{}' has it and it points to {}",
                            target_repo.repo_identity().name(),
                            target_bookmark,
                            source_repo.repo_identity().name(),
                            source_cs_id,
                        );
                    }
                    NoSyncOutcome { target_bookmark } => {
                        warn!(
                            ctx.logger(),
                            "'{}' has a bookmark {} but it points to a commit that has no \
                            equivalent in '{}'. If it's a shared bookmark (e.g. master) \
                            that might mean that it points to a commit from another repository",
                            target_repo.repo_identity().name(),
                            target_bookmark,
                            source_repo.repo_identity().name(),
                        );
                    }
                }
            }
            Err(format_err!("found {} inconsistencies", diff.len()).into())
        }
    }
}

async fn update_large_repo_bookmarks(
    ctx: CoreContext,
    diff: &Vec<BookmarkDiff>,
    syncers: &Syncers<SqlSyncedCommitMapping, CrossRepo>,
    common_commit_sync_config: &CommonCommitSyncConfig,
    update_mode: UpdateLargeRepoBookmarksMode,
    limit: Option<usize>,
) -> Result<(), Error> {
    let large_repo = syncers.small_to_large.get_large_repo();
    let mut book_txn = large_repo.bookmarks().create_transaction(ctx.clone());

    let bookmark_renamer = syncers.small_to_large.get_bookmark_renamer().await?;

    let diff: Box<dyn Iterator<Item = &BookmarkDiff>> = match limit {
        Some(limit) => {
            warn!(
                ctx.logger(),
                "found {} inconsistencies, will update at most {} of them...",
                diff.len(),
                limit
            );
            Box::new(diff.iter().take(limit))
        }
        None => {
            warn!(
                ctx.logger(),
                "found {} inconsistencies, trying to update them...",
                diff.len()
            );
            Box::new(diff.iter())
        }
    };
    for d in diff {
        if common_commit_sync_config
            .common_pushrebase_bookmarks
            .contains(d.target_bookmark())
        {
            info!(
                ctx.logger(),
                "skipping {} because it's a common bookmark",
                d.target_bookmark()
            );
            continue;
        }

        use validation::BookmarkDiff::*;
        match d {
            InconsistentValue {
                target_bookmark,
                target_cs_id,
                ..
            } => {
                let outcomes = syncers
                    .small_to_large
                    .get_plural_commit_sync_outcome(&ctx, *target_cs_id)
                    .await?
                    .with_context(|| {
                        format!("Missing outcome for {} from small repo", target_cs_id)
                    })?;

                use PluralCommitSyncOutcome::*;
                let new_value = match outcomes {
                    NotSyncCandidate(..) => {
                        warn!(
                            ctx.logger(),
                            "{} from small repo doesn't remap to large repo", target_cs_id,
                        );
                        None
                    }
                    EquivalentWorkingCopyAncestor(large_cs_id, _) => Some(large_cs_id),
                    RewrittenAs(rewritten_commits) if rewritten_commits.len() == 1 => {
                        Some(rewritten_commits.into_iter().next().unwrap().0)
                    }
                    RewrittenAs(rewritten_commits) => {
                        return Err(format_err!(
                            "multiple remappings of {} in {}: {:?}",
                            *target_cs_id,
                            large_repo.repo_identity().name(),
                            rewritten_commits,
                        ));
                    }
                };

                if let Some(large_cs_id) = new_value {
                    derive_all_enabled_datatypes_for_csids(&ctx, large_repo, vec![large_cs_id])?
                        .await?;
                    let reason = BookmarkUpdateReason::XRepoSync;
                    let large_bookmark = bookmark_renamer(target_bookmark).ok_or_else(|| {
                        format_err!("small bookmark {} remaps to nothing", target_bookmark)
                    })?;

                    info!(ctx.logger(), "setting {} {}", large_bookmark, large_cs_id);
                    if update_mode == UpdateLargeRepoBookmarksMode::Real {
                        book_txn.force_set(&large_bookmark, large_cs_id, reason)?;
                    }
                }
            }
            MissingInTarget {
                target_bookmark, ..
            } => {
                warn!(
                    ctx.logger(),
                    "large repo bookmark (renames to {}) not found in small repo", target_bookmark,
                );
                let large_bookmark = bookmark_renamer(target_bookmark).ok_or_else(|| {
                    format_err!("small bookmark {} remaps to nothing", target_bookmark)
                })?;
                let reason = BookmarkUpdateReason::XRepoSync;
                info!(ctx.logger(), "deleting {}", large_bookmark);
                if update_mode == UpdateLargeRepoBookmarksMode::Real {
                    book_txn.force_delete(&large_bookmark, reason)?;
                }
            }
            NoSyncOutcome { target_bookmark } => {
                warn!(
                    ctx.logger(),
                    "Not updating {} because it points to a commit that has no \
                     equivalent in source repo.",
                    target_bookmark,
                );
            }
        }
    }

    book_txn.commit().await?;
    Ok(())
}

pub fn build_subcommand<'a, 'b>() -> App<'a, 'b> {
    let map_subcommand = SubCommand::with_name(MAP_SUBCOMMAND)
        .about("Check cross-repo commit mapping")
        .arg(
            Arg::with_name(HASH_ARG)
                .required(true)
                .help("bonsai changeset hash to map"),
        );

    let verify_wc_subcommand = SubCommand::with_name(VERIFY_WC_SUBCOMMAND)
        .about("verify working copy")
        .arg(
            Arg::with_name(LARGE_REPO_HASH_ARG)
                .required(true)
                .help("bonsai changeset hash from large repo to verify"),
        );

    let verify_bookmarks_subcommand = SubCommand::with_name(VERIFY_BOOKMARKS_SUBCOMMAND).about(
        "verify that bookmarks are the same in small and large repo (subject to bookmark renames)",
    ).arg(
        Arg::with_name(UPDATE_LARGE_REPO_BOOKMARKS)
            .long(UPDATE_LARGE_REPO_BOOKMARKS)
            .required(false)
            .takes_value(false)
            .help("update any inconsistencies between bookmarks (except for the common bookmarks between large and small repo e.g. 'master')"),
    ).arg(
        Arg::with_name(LIMIT_ARG)
            .long(LIMIT_ARG)
            .required(false)
            .requires(UPDATE_LARGE_REPO_BOOKMARKS)
            .takes_value(true)
            .help("update up to N bookmarks in large repo. Default value is unlimited"),
    ).arg(
        Arg::with_name(NO_BOOKMARK_UPDATES)
            .long(NO_BOOKMARK_UPDATES)
            .required(false)
            .requires(UPDATE_LARGE_REPO_BOOKMARKS)
            .takes_value(false)
            .help("don't do actual bookmark updates, only print what would be done (deriving data is real!)"),
    );

    let commit_sync_config_subcommand = {
        let by_version_subcommand = SubCommand::with_name(SUBCOMMAND_BY_VERSION)
            .about("print info about a particular version of CommitSyncConfig")
            .arg(
                Arg::with_name(ARG_VERSION_NAME)
                    .required(true)
                    .takes_value(true)
                    .help("commit sync config version name to query"),
            );

        let list_subcommand = SubCommand::with_name(SUBCOMMAND_LIST)
            .about("list all available CommitSyncConfig versions for repo")
            .arg(
                Arg::with_name(ARG_WITH_CONTENTS)
                    .long(ARG_WITH_CONTENTS)
                    .required(false)
                    .takes_value(false)
                    .help("Do not just print version names, also include config bodies"),
            );

        SubCommand::with_name(SUBCOMMAND_CONFIG)
            .about("query available CommitSyncConfig versions for repo")
            .subcommand(list_subcommand)
            .subcommand(by_version_subcommand)
    };

    let prepare_rollout_subcommand = SubCommand::with_name(PREPARE_ROLLOUT_SUBCOMMAND)
        .about("command to prepare rollout of pushredirection");

    let change_mapping_version = SubCommand::with_name(CHANGE_MAPPING_VERSION_SUBCOMMAND)
        .about(
            "a command to change mapping version for a given bookmark. \
        Note that this command doesn't check that the working copies of source and target repo \
        are equivalent according to the new mapping. This needs to ensured before calling this command",
        )
        .arg(
            Arg::with_name(AUTHOR_ARG)
                .long(AUTHOR_ARG)
                .required(true)
                .takes_value(true)
                .help("Author of the commit that will change the mapping"),
        )
        .arg(
            Arg::with_name(ONCALL_ARG)
                .long(ONCALL_ARG)
                .required(false)
                .takes_value(true)
                .help("Oncall for the commit that will change the mapping"),
        )
        .arg(
            Arg::with_name(LARGE_REPO_BOOKMARK_ARG)
                .long(LARGE_REPO_BOOKMARK_ARG)
                .required(true)
                .takes_value(true)
                .help("bookmark in the large repo"),
        )
        .arg(
            Arg::with_name(ARG_VERSION_NAME)
                .long(ARG_VERSION_NAME)
                .required(true)
                .takes_value(true)
                .help("mapping version to change to"),
        )
        .arg(
            Arg::with_name(VIA_EXTRAS_ARG)
                .long(VIA_EXTRAS_ARG)
                .required(false)
                .takes_value(false)
                .help("change mapping via pushing a commit with a special extra set. \
                This should become a default method, but for now let's hide behind this arg")
        )
        .arg(
            Arg::with_name(DUMP_MAPPING_LARGE_REPO_PATH_ARG)
                .long(DUMP_MAPPING_LARGE_REPO_PATH_ARG)
                .required(false)
                .takes_value(true)
                .help("Path in the repo where new mapping version will be dumped.")
        );

    let pushredirection_subcommand = SubCommand::with_name(PUSHREDIRECTION_SUBCOMMAND)
        .about("helper commands to enable/disable pushredirection")
        .subcommand(prepare_rollout_subcommand)
        .subcommand(change_mapping_version);

    let rewritten_subcommand = SubCommand::with_name(REWRITTEN_SUBCOMMAND)
        .about("mark a pair of commits as rewritten")
        .arg(
            Arg::with_name(SOURCE_HASH_ARG)
                .long(SOURCE_HASH_ARG)
                .required(true)
                .takes_value(true)
                .help("hash in the source repo"),
        )
        .arg(
            Arg::with_name(TARGET_HASH_ARG)
                .long(TARGET_HASH_ARG)
                .required(true)
                .takes_value(true)
                .help("hash in the target repo"),
        )
        .arg(
            Arg::with_name(ARG_VERSION_NAME)
                .long(ARG_VERSION_NAME)
                .required(true)
                .takes_value(true)
                .help("mapping version to write to db"),
        );

    let equivalent_wc_subcommand = SubCommand::with_name(EQUIVALENT_WORKING_COPY_SUBCOMMAND)
        .about("mark a pair of commits as having an equivalent working copy")
        .arg(
            Arg::with_name(SOURCE_HASH_ARG)
                .long(SOURCE_HASH_ARG)
                .required(true)
                .takes_value(true)
                .help("hash in the source repo"),
        )
        .arg(
            Arg::with_name(TARGET_HASH_ARG)
                .long(TARGET_HASH_ARG)
                .required(true)
                .takes_value(true)
                .help("hash in the target repo"),
        )
        .arg(
            Arg::with_name(ARG_VERSION_NAME)
                .long(ARG_VERSION_NAME)
                .required(true)
                .takes_value(true)
                .help("mapping version to write to db"),
        );

    let not_sync_candidate_subcommand = SubCommand::with_name(NOT_SYNC_CANDIDATE_SUBCOMMAND)
        .about("mark a source commit as not having a synced commit in the target repo")
        .arg(
            Arg::with_name(LARGE_REPO_HASH_ARG)
                .long(LARGE_REPO_HASH_ARG)
                .required(true)
                .takes_value(true)
                .help("hash in the source repo"),
        )
        .arg(
            Arg::with_name(ARG_VERSION_NAME)
                .long(ARG_VERSION_NAME)
                .required(false)
                .takes_value(true)
                .help("optional mapping version to write to db"),
        );

    let insert_subcommand = SubCommand::with_name(INSERT_SUBCOMMAND)
        .about("helper commands to insert mappings directly into db")
        .subcommand(equivalent_wc_subcommand)
        .subcommand(rewritten_subcommand)
        .subcommand(not_sync_candidate_subcommand);

    SubCommand::with_name(CROSSREPO)
        .subcommand(map_subcommand)
        .subcommand(verify_wc_subcommand)
        .subcommand(verify_bookmarks_subcommand)
        .subcommand(commit_sync_config_subcommand)
        .subcommand(pushredirection_subcommand)
        .subcommand(insert_subcommand)
}

async fn get_syncers<'a>(
    ctx: &'a CoreContext,
    source_repo: CrossRepo,
    target_repo: CrossRepo,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    mapping: SqlSyncedCommitMapping,
    matches: &'a MononokeMatches<'a>,
) -> Result<Syncers<SqlSyncedCommitMapping, CrossRepo>, Error> {
    let caching = matches.caching();
    let x_repo_sync_lease = create_commit_syncer_lease(ctx.fb, caching)?;

    let common_sync_config =
        live_commit_sync_config.get_common_config(source_repo.repo_identity().id())?;

    let (large_repo, small_repo) = if common_sync_config.large_repo_id
        == source_repo.repo_identity().id()
        && common_sync_config
            .small_repos
            .contains_key(&target_repo.repo_identity().id())
    {
        (source_repo, target_repo)
    } else if common_sync_config.large_repo_id == target_repo.repo_identity().id()
        && common_sync_config
            .small_repos
            .contains_key(&source_repo.repo_identity().id())
    {
        (target_repo, source_repo)
    } else {
        return Err(format_err!(
            "CommitSyncMapping incompatible with source repo {:?} and target repo {:?}",
            source_repo.repo_identity().id(),
            target_repo.repo_identity().id()
        ));
    };

    create_commit_syncers(
        ctx,
        small_repo,
        large_repo,
        mapping,
        live_commit_sync_config,
        x_repo_sync_lease,
    )
}

async fn get_large_to_small_commit_syncer<'a>(
    ctx: &'a CoreContext,
    source_repo: CrossRepo,
    target_repo: CrossRepo,
    live_commit_sync_config: Arc<dyn LiveCommitSyncConfig>,
    mapping: SqlSyncedCommitMapping,
    matches: &'a MononokeMatches<'a>,
) -> Result<CommitSyncer<SqlSyncedCommitMapping, CrossRepo>, Error> {
    Ok(get_syncers(
        ctx,
        source_repo,
        target_repo,
        live_commit_sync_config,
        mapping,
        matches,
    )
    .await?
    .large_to_small)
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::sync::Arc;

    use ascii::AsciiString;
    use bookmarks::BookmarkKey;
    use cacheblob::InProcessLease;
    use changeset_fetcher::ChangesetFetcherArc;
    use cross_repo_sync::validation::find_bookmark_diff;
    use fixtures::set_bookmark;
    use fixtures::Linear;
    use fixtures::TestRepoFixture;
    use futures::compat::Stream01CompatExt;
    use futures::TryStreamExt;
    use live_commit_sync_config::TestLiveCommitSyncConfig;
    use maplit::hashmap;
    use maplit::hashset;
    use metaconfig_types::CommitSyncConfig;
    use metaconfig_types::CommitSyncConfigVersion;
    use metaconfig_types::CommonCommitSyncConfig;
    use metaconfig_types::SmallRepoCommitSyncConfig;
    use metaconfig_types::SmallRepoPermanentConfig;
    use mononoke_types::RepositoryId;
    use revset::AncestorsNodeStream;
    use sql_construct::SqlConstruct;
    use synced_commit_mapping::SyncedCommitMappingEntry;
    use synced_commit_mapping::SyncedCommitSourceRepo;

    use super::*;

    #[fbinit::test]
    fn test_bookmark_diff(fb: FacebookInit) -> Result<(), Error> {
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(test_bookmark_diff_impl(fb))
    }

    async fn test_bookmark_diff_impl(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);
        let syncers = init_syncers(fb).await?;

        let small_repo = syncers.small_to_large.get_small_repo();
        let large_repo = syncers.small_to_large.get_large_repo();

        let master = BookmarkKey::new("master")?;
        let maybe_master_val = small_repo.bookmarks().get(ctx.clone(), &master).await?;
        let master_val = maybe_master_val.ok_or_else(|| Error::msg("master not found"))?;

        // Everything is identical - no diff at all
        {
            let diff = find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;

            assert!(diff.is_empty());
        }

        // Move bookmark to another changeset
        let another_hash = "607314ef579bd2407752361ba1b0c1729d08b281";
        set_bookmark(fb, &small_repo, another_hash, master.clone()).await;
        let another_bcs_id =
            helpers::csid_resolve(&ctx, small_repo, another_hash.to_string()).await?;

        let actual_diff = find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;

        let mut expected_diff = hashset! {
            BookmarkDiff::InconsistentValue {
                target_bookmark: master.clone(),
                target_cs_id: another_bcs_id,
                source_cs_id: Some(master_val),
            }
        };
        assert!(!actual_diff.is_empty());
        assert_eq!(
            actual_diff.into_iter().collect::<HashSet<_>>(),
            expected_diff,
        );

        // Create another bookmark
        let another_book = BookmarkKey::new("newbook")?;
        set_bookmark(fb, &small_repo, another_hash, another_book.clone()).await;

        let actual_diff = find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;

        expected_diff.insert(BookmarkDiff::InconsistentValue {
            target_bookmark: another_book,
            target_cs_id: another_bcs_id,
            source_cs_id: None,
        });
        assert_eq!(
            actual_diff.clone().into_iter().collect::<HashSet<_>>(),
            expected_diff
        );

        // Update the bookmarks
        {
            let mut common_config = CommonCommitSyncConfig {
                common_pushrebase_bookmarks: vec![master.clone()],
                small_repos: hashmap! {
                    small_repo.repo_identity().id() => SmallRepoPermanentConfig {
                        bookmark_prefix: Default::default()
                    },
                },
                large_repo_id: large_repo.repo_identity().id(),
            };

            update_large_repo_bookmarks(
                ctx.clone(),
                &actual_diff,
                &syncers,
                &common_config,
                UpdateLargeRepoBookmarksMode::Real,
                None,
            )
            .await?;

            let actual_diff = find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;

            // Master bookmark hasn't been updated because it's a common pushrebase bookmark
            let expected_diff = hashset! {
                BookmarkDiff::InconsistentValue {
                    target_bookmark: master.clone(),
                    target_cs_id: another_bcs_id,
                    source_cs_id: Some(master_val),
                }
            };
            assert_eq!(
                actual_diff.clone().into_iter().collect::<HashSet<_>>(),
                expected_diff,
            );

            // Now remove master bookmark from common_pushrebase_bookmarks and update large repo
            // bookmarks again
            common_config.common_pushrebase_bookmarks = vec![];

            update_large_repo_bookmarks(
                ctx.clone(),
                &actual_diff,
                &syncers,
                &common_config,
                UpdateLargeRepoBookmarksMode::Real,
                None,
            )
            .await?;
            let actual_diff = find_bookmark_diff(ctx.clone(), &syncers.large_to_small).await?;
            assert!(actual_diff.is_empty());
        }
        Ok(())
    }

    async fn init_syncers(
        fb: FacebookInit,
    ) -> Result<Syncers<SqlSyncedCommitMapping, CrossRepo>, Error> {
        let ctx = CoreContext::test_mock(fb);
        let small_repo = Linear::get_inner_repo_with_id(fb, RepositoryId::new(0)).await;
        let large_repo = Linear::get_inner_repo_with_id(fb, RepositoryId::new(1)).await;

        let master = BookmarkKey::new("master")?;
        let maybe_master_val = small_repo.bookmarks().get(ctx.clone(), &master).await?;

        let master_val = maybe_master_val.ok_or_else(|| Error::msg("master not found"))?;
        let changesets: Vec<_> =
            AncestorsNodeStream::new(ctx.clone(), &small_repo.changeset_fetcher_arc(), master_val)
                .compat()
                .try_collect()
                .await?;

        let current_version = CommitSyncConfigVersion("TEST_VERSION_NAME".to_string());

        let mapping = SqlSyncedCommitMapping::with_sqlite_in_memory()?;
        for cs_id in changesets {
            mapping
                .add(
                    &ctx,
                    SyncedCommitMappingEntry {
                        large_repo_id: large_repo.repo_identity().id(),
                        small_repo_id: small_repo.repo_identity().id(),
                        small_bcs_id: cs_id,
                        large_bcs_id: cs_id,
                        version_name: Some(current_version.clone()),
                        source_repo: Some(SyncedCommitSourceRepo::Large),
                    },
                )
                .await?;
        }

        let (lv_cfg, lv_cfg_src) = TestLiveCommitSyncConfig::new_with_source();

        let common_config = CommonCommitSyncConfig {
            common_pushrebase_bookmarks: vec![BookmarkKey::new("master")?],
            small_repos: hashmap! {
                small_repo.repo_identity().id() => SmallRepoPermanentConfig {
                    bookmark_prefix: AsciiString::new(),
                }
            },
            large_repo_id: large_repo.repo_identity().id(),
        };

        let current_version_config = CommitSyncConfig {
            large_repo_id: large_repo.repo_identity().id(),
            common_pushrebase_bookmarks: vec![BookmarkKey::new("master")?],
            small_repos: hashmap! {
                small_repo.repo_identity().id() => SmallRepoCommitSyncConfig {
                    default_action: DefaultSmallToLargeCommitSyncPathAction::Preserve,
                    map: hashmap! { },
                },
            },
            version_name: current_version.clone(),
        };

        lv_cfg_src.add_common_config(common_config);
        lv_cfg_src.add_config(current_version_config);
        let x_repo_sync_lease = Arc::new(InProcessLease::new());
        let live_commit_sync_config = Arc::new(lv_cfg);

        create_commit_syncers(
            &ctx,
            small_repo,
            large_repo,
            mapping,
            live_commit_sync_config,
            x_repo_sync_lease,
        )
    }
}
