/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Mononoke -> hg sync job
//!
//! It's a special job that is used to synchronize Mononoke to Mercurial when Mononoke is a source
//! of truth. All writes to Mononoke are replayed to Mercurial using this job. That can be used
//! to verify Mononoke's correctness and/or use hg as a disaster recovery mechanism.

#![feature(auto_traits)]
#![feature(async_closure)]

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::bail;
use anyhow::format_err;
use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use assembly_line::TryAssemblyLine;
use async_trait::async_trait;
use bonsai_globalrev_mapping::BonsaiGlobalrevMapping;
use bonsai_hg_mapping::BonsaiHgMapping;
use bonsai_hg_mapping::BonsaiHgMappingRef;
use bookmarks::BookmarkKey;
use bookmarks::BookmarkUpdateLog;
use bookmarks::BookmarkUpdateLogEntry;
use bookmarks::BookmarkUpdateLogId;
use bookmarks::Freshness;
use borrowed::borrowed;
use changeset_fetcher::ChangesetFetcher;
use changesets::Changesets;
use clap_old::Arg;
use clap_old::ArgGroup;
use clap_old::SubCommand;
use clientinfo::ClientEntryPoint;
use clientinfo::ClientInfo;
use cloned::cloned;
use cmdlib::args;
use cmdlib::args::MononokeMatches;
use cmdlib::helpers::block_execute;
use commit_graph::CommitGraph;
use context::CoreContext;
use darkstorm_verifier::DarkstormVerifier;
use dbbookmarks::SqlBookmarksBuilder;
use executor_lib::RepoShardedProcess;
use executor_lib::RepoShardedProcessExecutor;
use executor_lib::ShardedProcessExecutor;
use fbinit::FacebookInit;
use filestore::FilestoreConfig;
use filestore::FilestoreConfigRef;
use futures::future;
use futures::future::try_join;
use futures::future::try_join3;
use futures::future::BoxFuture;
use futures::future::FutureExt as _;
use futures::future::TryFutureExt;
use futures::lock::Mutex;
use futures::stream;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use futures::Stream;
use futures_stats::futures03::TimedFutureExt;
use futures_stats::FutureStats;
use futures_watchdog::WatchdogExt;
use http::Uri;
use lfs_verifier::LfsVerifier;
use mercurial_types::HgChangesetId;
use metaconfig_types::RepoConfig;
use mononoke_types::ChangesetId;
use mononoke_types::RepositoryId;
use mutable_counters::ArcMutableCounters;
use mutable_counters::MutableCounters;
use mutable_counters::MutableCountersArc;
use regex::Regex;
use repo_blobstore::RepoBlobstore;
use repo_blobstore::RepoBlobstoreRef;
use repo_derived_data::RepoDerivedData;
use repo_identity::RepoIdentity;
use repo_lock::MutableRepoLock;
use repo_lock::RepoLock;
use repo_lock::RepoLockState;
use repo_lock::SqlRepoLock;
use retry::retry_always;
use retry::RetryAttemptsCount;
use scuba_ext::MononokeScubaSampleBuilder;
use sharding_ext::RepoShard;
use slog::error;
use slog::info;
use sql_construct::SqlConstructFromMetadataDatabaseConfig;
use tempfile::NamedTempFile;
use tokio::runtime::Runtime;

use crate::bundle_generator::FilterExistingChangesets;

mod bundle_generator;
mod bundle_preparer;
mod darkstorm_verifier;
mod errors;
mod globalrev_syncer;
mod hgrepo;
mod lfs_verifier;

use crate::bundle_generator::FilenodeVerifier;
use crate::bundle_preparer::BundlePreparer;
use crate::errors::ErrorKind::SyncFailed;
use crate::errors::PipelineError;
use crate::errors::PipelineError::AnonymousError;
use crate::errors::PipelineError::EntryError;
use crate::globalrev_syncer::GlobalrevSyncer;
use crate::hgrepo::list_hg_server_bookmarks;
use crate::hgrepo::HgRepo;

const ARG_BOOKMARK_REGEX_FORCE_GENERATE_LFS: &str = "bookmark-regex-force-generate-lfs";
const ARG_BOOKMARK_MOVE_ANY_DIRECTION: &str = "bookmark-move-any-direction";
const ARG_DARKSTORM_BACKUP_REPO_GROUP: &str = "darkstorm-backup-repo";
const ARG_DARKSTORM_BACKUP_REPO_ID: &str = "darkstorm-backup-repo-id";
const ARG_DARKSTORM_BACKUP_REPO_NAME: &str = "darkstorm-backup-repo-name";
const ARG_BYPASS_READONLY: &str = "bypass-readonly";
const MODE_SYNC_ONCE: &str = "sync-once";
const MODE_SYNC_LOOP: &str = "sync-loop";
const JOB_TYPE: &str = "job-type";
const JOB_TYPE_PROD: &str = "prod";
const JOB_TYPE_BACKUP: &str = "backup";
const LATEST_REPLAYED_REQUEST_KEY: &str = "latest-replayed-request";
const SLEEP_SECS: u64 = 1;
const SCUBA_TABLE: &str = "mononoke_hg_sync";
const LOCK_REASON: &str = "Locked due to sync failure, check Source Control @ Meta";

const HGSQL_GLOBALREVS_USE_SQLITE: &str = "hgsql-globalrevs-use-sqlite";
const HGSQL_GLOBALREVS_DB_ADDR: &str = "hgsql-globalrevs-db-addr";

// Since the job is sharded with shard-level health check, keep the app
// level retries to a minimum. The manager will auto-restart the failing
// repo.
const DEFAULT_RETRY_NUM: usize = 1;
const DEFAULT_BATCH_SIZE: usize = 10;
const DEFAULT_CONFIGERATOR_BATCH_SIZE: usize = 5;
const DEFAULT_SINGLE_BUNDLE_TIMEOUT_MS: u64 = 5 * 60 * 1000;

#[derive(Copy, Clone)]
struct QueueSize(usize);

const SM_SERVICE_SCOPE: &str = "global";
const SM_CLEANUP_TIMEOUT_SECS: u64 = 120;

/// Struct representing the Hg Sync BP.
pub struct HgSyncProcess {
    matches: Arc<MononokeMatches<'static>>,
    fb: FacebookInit,
    _runtime: Runtime,
}

#[facet::container]
#[derive(Clone)]
pub struct Repo {
    #[facet]
    pub changeset_fetcher: dyn ChangesetFetcher,

    #[facet]
    pub changesets: dyn Changesets,

    #[facet]
    pub commit_graph: CommitGraph,

    #[facet]
    pub bonsai_hg_mapping: dyn BonsaiHgMapping,

    #[facet]
    pub mutable_counters: dyn MutableCounters,

    #[facet]
    pub bonsai_globalrev_mapping: dyn BonsaiGlobalrevMapping,

    #[facet]
    pub repo_identity: RepoIdentity,

    #[init(repo_identity.id())]
    pub repoid: RepositoryId,

    #[init(repo_identity.name().to_string())]
    pub repo_name: String,

    #[facet]
    pub repo_blobstore: RepoBlobstore,

    #[facet]
    pub filestore_config: FilestoreConfig,

    #[facet]
    pub repo_derived_data: RepoDerivedData,

    #[facet]
    pub repo_config: RepoConfig,
}

impl HgSyncProcess {
    fn new(fb: FacebookInit) -> Result<Self> {
        let app = args::MononokeAppBuilder::new("Mononoke -> hg sync job")
        .with_advanced_args_hidden()
        .with_fb303_args()
        .with_dynamic_repos()
        .build()
        .arg(
            Arg::with_name(JOB_TYPE)
            .long(JOB_TYPE)
            .takes_value(true)
            .help("The type of hg-sync job. Allowed values: prod, backup")
        )
        .arg(
            Arg::with_name("hg-repo-ssh-path")
                .takes_value(true)
                .help("Remote path to hg repo to replay to. Example: ssh://hg.vip.facebook.com//data/scm/fbsource"),
        )
        .arg(
            Arg::with_name("log-to-scuba")
                .long("log-to-scuba")
                .takes_value(false)
                .required(false)
                .help("If set job will log individual bundle sync states to Scuba"),
        )
        .arg(
            Arg::with_name("lock-on-failure")
                .long("lock-on-failure")
                .takes_value(false)
                .required(false)
                .help("If set, mononoke repo will be locked on sync failure"),
        )
        .arg(
            Arg::with_name("base-retry-delay-ms")
                .long("base-retry-delay-ms")
                .takes_value(true)
                .required(false)
                .help("initial delay between failures. It will be increased on the successive attempts")
        )
        .arg(
            Arg::with_name("retry-num")
                .long("retry-num")
                .takes_value(true)
                .required(false)
                .help("how many times to retry to sync a single bundle")
        )
        .arg(
            Arg::with_name("batch-size")
                .long("batch-size")
                .takes_value(true)
                .required(false)
                .help("maximum number of bundles allowed over a single hg peer")
        )
        .arg(
            Arg::with_name("configerator-batch-size")
                .long("configerator-batch-size")
                .takes_value(true)
                .required(false)
                .help("maximum number of bundles allowed over a single hg peer for the configerator repo")
        )
        .arg(
            Arg::with_name("single-bundle-timeout-ms")
                .long("single-bundle-timeout-ms")
                .takes_value(true)
                .required(false)
                .help("a timeout to send a single bundle to (if exceeded, the peer is restarted)")
        )
        .arg(
            Arg::with_name("verify-server-bookmark-on-failure")
                .long("verify-server-bookmark-on-failure")
                .takes_value(false)
                .required(false)
                .help("if present, check after a failure whether a server bookmark is already in the expected location")
        )
        .arg(
            Arg::with_name(HGSQL_GLOBALREVS_USE_SQLITE)
                .long(HGSQL_GLOBALREVS_USE_SQLITE)
                .takes_value(false)
                .required(false)
                .help("Use sqlite for hgsql globalrev sync (use for testing)."),
        )
        .arg(
            Arg::with_name(HGSQL_GLOBALREVS_DB_ADDR)
                .long(HGSQL_GLOBALREVS_DB_ADDR)
                .takes_value(true)
                .required(false)
                .help("unused"),
        )
        .arg(
            Arg::with_name(ARG_BOOKMARK_REGEX_FORCE_GENERATE_LFS)
                .long(ARG_BOOKMARK_REGEX_FORCE_GENERATE_LFS)
                .takes_value(true)
                .required(false)
                .help("force generation of lfs bundles for bookmarks that match regex"),
        )
        .arg(
            Arg::with_name("verify-lfs-blob-presence")
                .long("verify-lfs-blob-presence")
                .takes_value(true)
                .required(false)
                .help("If generating bundles, verify lfs blob presence at this batch endpoint"),
        )
        .arg(
            Arg::with_name(ARG_BOOKMARK_MOVE_ANY_DIRECTION)
                .long(ARG_BOOKMARK_MOVE_ANY_DIRECTION)
                .takes_value(false)
                .required(false)
                .help("This flag controls whether we tell the server to allow \
                the bookmark movement in any direction (adding pushvar NON_FAST_FORWARD=true). \
                However, the server checks its per bookmark configuration before move."),
        )
        .arg(
            Arg::with_name(ARG_DARKSTORM_BACKUP_REPO_ID)
            .long(ARG_DARKSTORM_BACKUP_REPO_ID)
            .takes_value(true)
            .required(false)
            .help("Start hg-sync-job for syncing prod repo and darkstorm backup mononoke repo \
            and use darkstorm-backup-repo-id value as a target for sync."),
        )
        .arg(
            Arg::with_name(ARG_DARKSTORM_BACKUP_REPO_NAME)
            .long(ARG_DARKSTORM_BACKUP_REPO_NAME)
            .takes_value(true)
            .required(false)
            .help("Start hg-sync-job for syncing prod repo and darkstorm backup mononoke repo \
            and use darkstorm-backup-repo-name as a target for sync."),
        )
        .group(
            ArgGroup::with_name(ARG_DARKSTORM_BACKUP_REPO_GROUP)
                .args(&[ARG_DARKSTORM_BACKUP_REPO_ID, ARG_DARKSTORM_BACKUP_REPO_NAME])
        )
        .arg(
            Arg::with_name(ARG_BYPASS_READONLY)
                .long(ARG_BYPASS_READONLY)
                .takes_value(false)
                .required(false)
                .help("This flag make it possible to push bundle into readonly repos \
                (by adding pushvar BYPASS_READONLY=true)."),
        )
        .about(
            "Special job that takes bundles that were sent to Mononoke and \
             applies them to mercurial",
        );

        let sync_once = SubCommand::with_name(MODE_SYNC_ONCE)
            .about("Syncs a single bundle")
            .arg(
                Arg::with_name("start-id")
                    .long("start-id")
                    .takes_value(true)
                    .required(true)
                    .help("id in the database table to start sync with"),
            );
        let sync_loop = SubCommand::with_name(MODE_SYNC_LOOP)
            .about("Syncs bundles one by one")
            .arg(
                Arg::with_name("start-id")
                    .long("start-id")
                    .takes_value(true)
                    .required(true)
                    .help("if current counter is not set then `start-id` will be used"),
            )
            .arg(
                Arg::with_name("loop-forever")
                    .long("loop-forever")
                    .takes_value(false)
                    .required(false)
                    .help(
                        "If set job will loop forever even if there are no new entries in db or \
                     if there was an error",
                    ),
            )
            .arg(
                Arg::with_name("bundle-prefetch")
                    .long("bundle-prefetch")
                    .takes_value(true)
                    .required(false)
                    .help("How many bundles to prefetch (NOOP left for backwards compat)"),
            )
            .arg(
                Arg::with_name("bundle-buffer-size")
                    .long("bundle-buffer-size")
                    .takes_value(true)
                    .required(false)
                    .help(
                        "How many bundles should be gnererated and buffered in \
                        advance of replaying (min 1, default 5)",
                    ),
            )
            .arg(
                Arg::with_name("exit-file")
                    .long("exit-file")
                    .takes_value(true)
                    .required(false)
                    .help(
                        "If you provide this argument, the sync loop will gracefully exit \
                     once this file exists",
                    ),
            )
            .arg(
                Arg::with_name("skip-bookmark")
                    .long("skip-bookmark")
                    .takes_value(true)
                    .required(false)
                    .multiple(true)
                    .help("skip entries about particular bookmark from bookmark update log")
            )
            .arg(
                Arg::with_name("replace-changeset")
                    .long("replace-changeset")
                    .takes_value(true)
                    .required(false)
                    .multiple(true)
                    .help("replace particular changeset mentioned in bookmark update log with a new commit hash, \
                           the arg should be formatted like bonsai_csid:replacement_bonsai_csid")
            )
            .arg(
                Arg::with_name("combine-bundles")
                    .long("combine-bundles")
                    .takes_value(true)
                    .required(false)
                    .help("How many bundles to combine into a single bundle before sending to hg"),
            )
            .arg(
                Arg::with_name("max-commits-per-combined-bundle")
                    .long("max-commits-per-combined-bundle")
                    .takes_value(true)
                    .required(false)
                    .help("How many commits should be maximally put in combined bundle (takes precednce over the number of bundles)"),
            );
        let app = app.subcommand(sync_once).subcommand(sync_loop);

        let (matches, _runtime) = app.get_matches(fb)?;
        let matches = Arc::new(matches);
        Ok(Self {
            matches,
            fb,
            _runtime,
        })
    }
}

#[async_trait]
impl RepoShardedProcess for HgSyncProcess {
    async fn setup(&self, repo: &RepoShard) -> anyhow::Result<Arc<dyn RepoShardedProcessExecutor>> {
        let repo_name = repo.repo_name.as_str();
        info!(
            self.matches.logger(),
            "Setting up hg sync command for repo {}", repo_name
        );
        let executor =
            HgSyncProcessExecutor::new(self.fb, Arc::clone(&self.matches), repo_name.to_string());
        info!(
            self.matches.logger(),
            "Completed hg sync command setup for repo {}", repo_name
        );
        Ok(Arc::new(executor))
    }
}

/// Struct representing the execution of the Hg Sync
/// BP over the context of a provided repo.
pub struct HgSyncProcessExecutor {
    matches: Arc<MononokeMatches<'static>>,
    ctx: CoreContext,
    cancellation_requested: Arc<AtomicBool>,
    repo_name: String,
}

impl HgSyncProcessExecutor {
    fn new(fb: FacebookInit, matches: Arc<MononokeMatches<'static>>, repo_name: String) -> Self {
        let ctx = CoreContext::new_with_logger_and_client_info(
            fb,
            matches.logger().clone(),
            ClientInfo::default_with_entry_point(ClientEntryPoint::MononokeHgSync),
        )
        .clone_with_repo_name(&repo_name);
        Self {
            matches,
            ctx,
            repo_name,
            cancellation_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl RepoShardedProcessExecutor for HgSyncProcessExecutor {
    async fn execute(&self) -> anyhow::Result<()> {
        info!(
            self.ctx.logger(),
            "Initiating hg sync command execution for repo {}", &self.repo_name,
        );
        let base_retry_delay_ms = 1000;
        retry_always(
            self.ctx.logger(),
            |attempt| async move {
                // Once cancellation is requested, do not retry even if its
                // a retryable error.
                if self.cancellation_requested.load(Ordering::Relaxed) {
                    info!(
                        self.ctx.logger(),
                        "sync stopping due to cancellation request at attempt {}", attempt
                    );
                    anyhow::Ok(())
                } else {
                    run(
                        &self.ctx,
                        &self.matches,
                        self.repo_name.clone(),
                        Arc::clone(&self.cancellation_requested),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Error during hg sync command execution for repo {}. Attempt number {}",
                            &self.repo_name, attempt
                        )
                    })?;
                    anyhow::Ok(())
                }
            },
            base_retry_delay_ms,
            DEFAULT_RETRY_NUM,
        )
        .await?;
        info!(
            self.ctx.logger(),
            "Finished hg sync command execution for repo {}", &self.repo_name,
        );
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!(
            self.ctx.logger(),
            "Terminating hg sync command execution for repo {}", &self.repo_name,
        );
        self.cancellation_requested.store(true, Ordering::Relaxed);
        Ok(())
    }
}

struct PipelineState<T> {
    entries: Vec<BookmarkUpdateLogEntry>,
    data: T,
}

type OutcomeWithStats =
    Result<(FutureStats, PipelineState<RetryAttemptsCount>), (Option<FutureStats>, PipelineError)>;

type Outcome = Result<PipelineState<RetryAttemptsCount>, PipelineError>;

fn get_id_to_search_after(entries: &[BookmarkUpdateLogEntry]) -> BookmarkUpdateLogId {
    entries
        .iter()
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0.into())
}

fn bind_sync_err(entries: &[BookmarkUpdateLogEntry], cause: Error) -> PipelineError {
    let ids: Vec<_> = entries.iter().map(|entry| entry.id).collect();
    let entries = entries.to_vec();
    EntryError {
        entries,
        cause: (SyncFailed { ids, cause }).into(),
    }
}

fn bind_sync_result<T>(
    entries: &[BookmarkUpdateLogEntry],
    res: Result<T>,
) -> Result<PipelineState<T>, PipelineError> {
    match res {
        Ok(data) => Ok(PipelineState {
            entries: entries.to_vec(),
            data,
        }),
        Err(cause) => Err(bind_sync_err(entries, cause)),
    }
}

fn drop_outcome_stats(o: OutcomeWithStats) -> Outcome {
    o.map(|(_, r)| r).map_err(|(_, e)| e)
}

fn build_reporting_handler<'a, B>(
    ctx: &'a CoreContext,
    scuba_sample: &'a MononokeScubaSampleBuilder,
    retry_num: usize,
    bookmarks: &'a B,
) -> impl Fn(OutcomeWithStats) -> BoxFuture<'a, Result<PipelineState<RetryAttemptsCount>, PipelineError>>
where
    B: BookmarkUpdateLog,
{
    move |res| {
        async move {
            let log_entries = match &res {
                Ok((_, pipeline_state, ..)) => Some(pipeline_state.entries.clone()),
                Err((_, EntryError { entries, .. })) => Some(entries.clone()),
                Err((_, AnonymousError { .. })) => None,
            };

            let maybe_stats = match &res {
                Ok((stats, _)) => Some(stats),
                Err((stats, _)) => stats.as_ref(),
            };

            // TODO: (torozco) T43766262 We should embed attempts in retry()'s Error type and use it
            // here instead of receiving a plain ErrorKind and implicitly assuming retry_num attempts.
            let attempts = match &res {
                Ok((_, PipelineState { data: attempts, .. })) => attempts.clone(),
                Err(..) => RetryAttemptsCount(retry_num),
            };

            let maybe_error = match &res {
                Ok(..) => None,
                Err((_, EntryError { cause, .. })) => Some(cause),
                Err((_, AnonymousError { cause, .. })) => Some(cause),
            };

            let f = async {
                if let Some(log_entries) = log_entries {
                    let duration =
                        maybe_stats.map_or_else(|| Duration::from_secs(0), |s| s.completion_time);

                    let error = maybe_error.map(|e| format!("{:?}", e));
                    let next_id = get_id_to_search_after(&log_entries);

                    let n = bookmarks
                        .count_further_bookmark_log_entries(ctx.clone(), next_id, None)
                        .await?;
                    let queue_size = QueueSize(n as usize);
                    info!(
                        ctx.logger(),
                        "queue size after processing: {}", queue_size.0
                    );
                    log_processed_entries_to_scuba(
                        &log_entries,
                        scuba_sample.clone(),
                        error,
                        attempts,
                        duration,
                        queue_size,
                    );
                }
                Result::<_, Error>::Ok(())
            };

            // Ignore result from future that did the logging
            let _ = f.await;
            drop_outcome_stats(res)
        }
        .boxed()
    }
}

async fn unlock_repo_if_locked(ctx: &CoreContext, repo_lock: &dyn RepoLock) -> Result<(), Error> {
    let repo_state = repo_lock.check_repo_lock().await?;

    match repo_state {
        RepoLockState::Locked(ref lock_msg) if lock_msg == LOCK_REASON => {
            let updated = repo_lock.set_repo_lock(RepoLockState::Unlocked).await?;

            if updated {
                info!(ctx.logger(), "repo is unlocked");
            }

            Ok(())
        }
        RepoLockState::Locked(..) | RepoLockState::Unlocked => Ok(()),
    }
}

async fn lock_repo_if_unlocked(ctx: &CoreContext, repo_lock: &dyn RepoLock) -> Result<(), Error> {
    info!(ctx.logger(), "locking repo...");
    let repo_state = repo_lock.check_repo_lock().await?;

    match repo_state {
        RepoLockState::Unlocked => {
            let updated = repo_lock
                .set_repo_lock(RepoLockState::Locked(LOCK_REASON.to_string()))
                .await?;

            if updated {
                info!(ctx.logger(), "repo is locked now");
            }

            Ok(())
        }

        RepoLockState::Locked(ref lock_msg) => {
            info!(ctx.logger(), "repo is locked already: {}", lock_msg);
            Ok(())
        }
    }
}

fn build_outcome_handler<'a>(
    ctx: &'a CoreContext,
    lock_via: Option<&'a dyn RepoLock>,
) -> impl Fn(Outcome) -> BoxFuture<'a, Result<Vec<BookmarkUpdateLogEntry>, Error>> {
    move |res| {
        async move {
            match res {
                Ok(PipelineState { entries, .. }) => {
                    info!(
                        ctx.logger(),
                        "successful sync of entries {:?}",
                        entries.iter().map(|c| c.id).collect::<Vec<_>>()
                    );
                    Ok(entries)
                }
                Err(AnonymousError { cause: e }) => {
                    error!(ctx.logger(), "Error without entry: {:?}", e);
                    Err(e)
                }
                Err(EntryError { cause: e, .. }) => match lock_via {
                    Some(repo_lock) => {
                        let _ = lock_repo_if_unlocked(ctx, repo_lock).await;
                        Err(e)
                    }
                    None => Err(e),
                },
            }
        }
        .boxed()
    }
}

#[derive(Clone)]
pub struct CombinedBookmarkUpdateLogEntry {
    components: Vec<BookmarkUpdateLogEntry>,
    bundle_file: Arc<NamedTempFile>,
    timestamps_file: Arc<NamedTempFile>,
    cs_id: Option<(ChangesetId, HgChangesetId)>,
    bookmark: BookmarkKey,
    // List of commits in a bundle in case they are known
    commits: CommitsInBundle,
}

#[derive(Clone)]
pub enum CommitsInBundle {
    Commits(Vec<(HgChangesetId, ChangesetId)>),
    Unknown,
}

/// Sends a downloaded bundle to hg
async fn try_sync_single_combined_entry(
    ctx: &CoreContext,
    attempt: usize,
    combined_entry: &CombinedBookmarkUpdateLogEntry,
    hg_repo: &HgRepo,
) -> Result<(), Error> {
    let ids: Vec<_> = combined_entry
        .components
        .iter()
        .map(|entry| entry.id)
        .collect();
    info!(ctx.logger(), "syncing log entries {:?} ...", ids);

    let bundle_path = get_path(&combined_entry.bundle_file)?;
    let timestamps_path = get_path(&combined_entry.timestamps_file)?;

    hg_repo
        .apply_bundle(
            bundle_path,
            timestamps_path,
            combined_entry.bookmark.clone(),
            combined_entry.cs_id.map(|(_bcs_id, hg_cs_id)| hg_cs_id),
            attempt,
            ctx.logger(),
            &combined_entry.commits,
        )
        .watched(ctx.logger())
        .await?;

    Ok(())
}

async fn sync_single_combined_entry(
    ctx: &CoreContext,
    combined_entry: &CombinedBookmarkUpdateLogEntry,
    hg_repo: &HgRepo,
    base_retry_delay_ms: u64,
    retry_num: usize,
    globalrev_syncer: &GlobalrevSyncer,
) -> Result<RetryAttemptsCount, Error> {
    if combined_entry.cs_id.is_some() {
        globalrev_syncer
            .sync(ctx, &combined_entry.commits)
            .watched(ctx.logger())
            .await?
    }

    let (_, attempts) = retry_always(
        ctx.logger(),
        |attempt| try_sync_single_combined_entry(ctx, attempt, combined_entry, hg_repo),
        base_retry_delay_ms,
        retry_num,
    )
    .watched(ctx.logger())
    .await?;

    Ok(attempts)
}

/// Logs to Scuba information about a single bundle sync event
fn log_processed_entry_to_scuba(
    log_entry: &BookmarkUpdateLogEntry,
    mut scuba_sample: MononokeScubaSampleBuilder,
    error: Option<String>,
    attempts: RetryAttemptsCount,
    duration: Duration,
    queue_size: QueueSize,
    combined_from: Option<i64>,
) {
    let entry = log_entry.id;
    let book = format!("{}", log_entry.bookmark_name);
    let reason = format!("{}", log_entry.reason);
    let delay = log_entry.timestamp.since_seconds();

    scuba_sample
        .add("entry", u64::from(entry))
        .add("bookmark", book)
        .add("reason", reason)
        .add("attempts", attempts.0)
        .add("duration", duration.as_millis() as i64);

    if let Some(combined_from) = combined_from {
        scuba_sample.add("combined_from", combined_from);
    }

    match error {
        Some(error) => {
            scuba_sample.add("success", 0).add("err", error);
        }
        None => {
            scuba_sample.add("success", 1).add("delay", delay);
            scuba_sample.add("queue_size", queue_size.0);
        }
    };

    scuba_sample.log();
}

fn log_processed_entries_to_scuba(
    entries: &[BookmarkUpdateLogEntry],
    scuba_sample: MononokeScubaSampleBuilder,
    error: Option<String>,
    attempts: RetryAttemptsCount,
    duration: Duration,
    queue_size: QueueSize,
) {
    let n: f64 = entries.len() as f64;
    let individual_duration = duration.div_f64(n);

    let combined_from = if entries.len() == 1 {
        // Set combined_from to None if we synced a single entry
        // This will make it easier to find entries that were batched
        None
    } else {
        entries
            .first()
            .map(|entry| i64::try_from(entry.id).expect("Entry id must be positive"))
    };
    entries.iter().for_each(|entry| {
        log_processed_entry_to_scuba(
            entry,
            scuba_sample.clone(),
            error.clone(),
            attempts,
            individual_duration,
            queue_size,
            combined_from,
        )
    });
}

fn get_path(f: &NamedTempFile) -> Result<String> {
    f.path()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::msg("non-utf8 file"))
}

fn loop_over_log_entries<'a, B>(
    ctx: &'a CoreContext,
    bookmarks: &'a B,
    start_id: BookmarkUpdateLogId,
    loop_forever: bool,
    scuba_sample: &'a MononokeScubaSampleBuilder,
    fetch_up_to_bundles: u64,
    repo_lock: Arc<dyn RepoLock>,
) -> impl Stream<Item = Result<Vec<BookmarkUpdateLogEntry>, Error>> + 'a
where
    B: BookmarkUpdateLog + Clone,
{
    stream::try_unfold(Some(start_id), {
        move |maybe_id| {
            cloned!(ctx, bookmarks, repo_lock);
            async move {
                match maybe_id {
                    Some(current_id) => {
                        let entries = bookmarks
                            .read_next_bookmark_log_entries_same_bookmark_and_reason(
                                ctx.clone(),
                                current_id,
                                fetch_up_to_bundles,
                            )
                            .try_collect::<Vec<_>>()
                            .watched(ctx.logger())
                            .await?;

                        match entries.iter().last().cloned() {
                            None => {
                                if loop_forever {
                                    info!(ctx.logger(), "id: {}, no new entries found", current_id);
                                    scuba_sample.clone().add("success", 1).add("delay", 0).log();

                                    // First None means that no new entries will be added to the stream,
                                    // Some(current_id) means that bookmarks will be fetched again
                                    tokio::time::sleep(Duration::new(SLEEP_SECS, 0)).await;

                                    unlock_repo_if_locked(&ctx, repo_lock.as_ref())
                                        .watched(ctx.logger())
                                        .await?;

                                    Ok(Some((vec![], Some(current_id))))
                                } else {
                                    Ok(Some((vec![], None)))
                                }
                            }
                            Some(last_entry) => Ok(Some((entries, Some(last_entry.id)))),
                        }
                    }
                    None => Ok(None),
                }
            }
        }
    })
}

#[derive(Clone)]
pub struct BookmarkOverlay {
    bookmarks: Arc<HashMap<BookmarkKey, ChangesetId>>,
    overlay: HashMap<BookmarkKey, Option<ChangesetId>>,
}

impl BookmarkOverlay {
    fn new(bookmarks: Arc<HashMap<BookmarkKey, ChangesetId>>) -> Self {
        Self {
            bookmarks,
            overlay: HashMap::new(),
        }
    }

    fn update(&mut self, book: BookmarkKey, val: Option<ChangesetId>) {
        self.overlay.insert(book, val);
    }

    fn all_values(&self) -> Vec<ChangesetId> {
        self.bookmarks
            .values()
            .chain(self.overlay.values().flatten())
            .copied()
            .collect()
    }

    fn get(&self, bookmark: &BookmarkKey) -> Option<ChangesetId> {
        if let Some(value) = self.overlay.get(bookmark) {
            return value.clone();
        }
        self.bookmarks.get(bookmark).cloned()
    }
}

struct LatestReplayedSyncCounter {
    mutable_counters: ArcMutableCounters,
}

impl LatestReplayedSyncCounter {
    fn new(source_repo: &Repo, darkstorm_backup_repo: Option<&Repo>) -> Result<Self, Error> {
        if let Some(backup_repo) = darkstorm_backup_repo {
            let mutable_counters = backup_repo.mutable_counters_arc();
            Ok(Self { mutable_counters })
        } else {
            let mutable_counters = source_repo.mutable_counters_arc();
            Ok(Self { mutable_counters })
        }
    }

    async fn get_counter(&self, ctx: &CoreContext) -> Result<Option<i64>, Error> {
        self.mutable_counters
            .get_counter(ctx, LATEST_REPLAYED_REQUEST_KEY)
            .await
    }

    async fn set_counter(&self, ctx: &CoreContext, value: i64) -> Result<bool, Error> {
        self.mutable_counters
            .set_counter(ctx, LATEST_REPLAYED_REQUEST_KEY, value, None)
            .await
    }
}

struct NoopChangesetsFilter;

#[async_trait]
impl FilterExistingChangesets for NoopChangesetsFilter {
    async fn filter(
        &self,
        _ctx: &CoreContext,
        cs_ids: Vec<(ChangesetId, HgChangesetId)>,
    ) -> Result<Vec<(ChangesetId, HgChangesetId)>, Error> {
        Ok(cs_ids)
    }
}

struct DarkstormBackupChangesetsFilter {
    repo: Arc<Repo>,
}

#[async_trait]
impl FilterExistingChangesets for DarkstormBackupChangesetsFilter {
    async fn filter(
        &self,
        ctx: &CoreContext,
        mut cs_ids: Vec<(ChangesetId, HgChangesetId)>,
    ) -> Result<Vec<(ChangesetId, HgChangesetId)>, Error> {
        let bcs_ids: Vec<_> = cs_ids.iter().map(|(cs_id, _hg_cs_id)| *cs_id).collect();
        let existing_bcs_ids: Vec<_> = self
            .repo
            .changesets
            .get_many(ctx, bcs_ids)
            .await?
            .into_iter()
            .map(|entry| entry.cs_id)
            .collect();
        let existing_hg_cs_id: BTreeSet<_> = self
            .repo
            .bonsai_hg_mapping
            .get(ctx, existing_bcs_ids.clone().into())
            .await?
            .into_iter()
            .map(|entry| entry.hg_cs_id)
            .collect();

        let existing_bcs_ids: BTreeSet<_> = existing_bcs_ids.into_iter().collect();

        let count = cs_ids.len();
        cs_ids.retain(|(cs_id, hg_cs_id)| {
            !existing_bcs_ids.contains(cs_id) || !existing_hg_cs_id.contains(hg_cs_id)
        });
        if count != cs_ids.len() {
            info!(
                ctx.logger(),
                "{} of {} commits already in the darkstorm backup repo, not including them in the bundle",
                count - cs_ids.len(),
                count,
            );
        }

        Ok(cs_ids)
    }
}

async fn run<'a>(
    ctx: &CoreContext,
    matches: &'a MononokeMatches<'a>,
    repo_name: String,
    cancellation_requested: Arc<AtomicBool>,
) -> Result<(), Error> {
    let config_store = matches.config_store();
    let resolved_repo = args::resolve_repo_by_name(config_store, matches, &repo_name)
        .with_context(|| format!("Invalid repo name provided: {}", &repo_name))?;
    let repo_config = resolved_repo.config.clone();
    let repo_id = resolved_repo.id;
    let job_config = match matches.value_of(JOB_TYPE) {
        Some(JOB_TYPE_PROD) => repo_config.hg_sync_config,
        Some(JOB_TYPE_BACKUP) => repo_config.backup_hg_sync_config,
        _ => None,
    };

    let hg_repo_path = match (&job_config, matches.value_of("hg-repo-ssh-path")) {
        (Some(config), _) => config.hg_repo_ssh_path.to_string(),
        (_, Some(hg_repo_path)) => hg_repo_path.to_string(),
        _ => {
            error!(ctx.logger(), "Path to hg repository must be specified");
            std::process::exit(1);
        }
    };

    let log_to_scuba = matches.is_present("log-to-scuba");
    let mut scuba_sample = if log_to_scuba {
        MononokeScubaSampleBuilder::new(ctx.fb, SCUBA_TABLE)?
    } else {
        MononokeScubaSampleBuilder::with_discard()
    };
    scuba_sample.add_common_server_data();

    let mysql_options = matches.mysql_options();
    let readonly_storage = matches.readonly_storage();
    let config_store = matches.config_store();

    let base_retry_delay_ms = args::get_u64_opt(matches, "base-retry-delay-ms").unwrap_or(1000);
    let retry_num = args::get_usize(matches, "retry-num", DEFAULT_RETRY_NUM);

    let bookmark_regex_force_lfs = matches
        .value_of(ARG_BOOKMARK_REGEX_FORCE_GENERATE_LFS)
        .map(Regex::new)
        .transpose()?;

    let mut vars = HashMap::new();
    if matches.is_present(ARG_BOOKMARK_MOVE_ANY_DIRECTION) {
        vars.insert("NON_FAST_FORWARD".to_string(), bytes::Bytes::from("true"));
    }
    if matches.is_present(ARG_BYPASS_READONLY) {
        vars.insert("BYPASS_READONLY".to_string(), bytes::Bytes::from("true"));
    }

    let push_vars = if vars.is_empty() { None } else { Some(vars) };

    let verify_lfs_blob_presence = matches
        .value_of("verify-lfs-blob-presence")
        .map(|s| s.to_string());

    let maybe_darkstorm_backup_repo = match (
        (matches.is_present(ARG_DARKSTORM_BACKUP_REPO_ID)
            || matches.is_present(ARG_DARKSTORM_BACKUP_REPO_NAME)),
        job_config.as_ref().and_then(|c| c.darkstorm_backup_repo_id),
    ) {
        (_, Some(backup_repo_id)) => {
            let backup_repo_id =
                args::resolve_repo_by_id(config_store, matches, backup_repo_id)?.id;
            let backup_repo: Repo =
                args::open_repo_by_id_unredacted(ctx.fb, ctx.logger(), matches, backup_repo_id)
                    .await?;

            scuba_sample.add("repo", backup_repo.repoid.id());
            scuba_sample.add("reponame", backup_repo.repo_name.clone());
            Some(Arc::new(backup_repo))
        }
        (true, _) => {
            let backup_repo_id = args::not_shardmanager_compatible::get_repo_id_from_value(
                config_store,
                matches,
                ARG_DARKSTORM_BACKUP_REPO_ID,
                ARG_DARKSTORM_BACKUP_REPO_NAME,
            )?;
            let backup_repo: Repo =
                args::open_repo_by_id_unredacted(ctx.fb, ctx.logger(), matches, backup_repo_id)
                    .await?;

            scuba_sample.add("repo", backup_repo.repoid.id());
            scuba_sample.add("reponame", backup_repo.repo_name.clone());
            Some(Arc::new(backup_repo))
        }
        _ => {
            scuba_sample.add("repo", repo_id.id());
            scuba_sample.add("reponame", repo_name.clone());
            None
        }
    };

    let (repo, filter_changesets, repo_parts) = {
        borrowed!(ctx);
        // FIXME: this cloned! will go away once HgRepo is asyncified
        cloned!(hg_repo_path);

        let (repo, preparer): (Repo, BoxFuture<Result<Arc<BundlePreparer>, Error>>) = {
            let repo: Repo =
                args::open_repo_by_id_unredacted(ctx.fb, ctx.logger(), matches, repo_id).await?;
            let filenode_verifier = match verify_lfs_blob_presence {
                Some(uri) => {
                    let uri = uri.parse::<Uri>()?;
                    let verifier = LfsVerifier::new(uri, Arc::new(repo.repo_blobstore().clone()))?;
                    FilenodeVerifier::LfsVerifier(verifier)
                }
                None => match maybe_darkstorm_backup_repo {
                    Some(ref backup_repo) => {
                        let verifier = DarkstormVerifier::new(
                            Arc::new(repo.repo_blobstore().clone()),
                            Arc::new(backup_repo.repo_blobstore().clone()),
                            backup_repo.filestore_config().clone(),
                        );
                        FilenodeVerifier::DarkstormVerifier(verifier)
                    }
                    None => FilenodeVerifier::NoopVerifier,
                },
            };
            (
                repo.clone(),
                BundlePreparer::new_generate_bundles(
                    repo,
                    base_retry_delay_ms,
                    retry_num,
                    filenode_verifier,
                    bookmark_regex_force_lfs,
                    push_vars,
                )
                .map_ok(Arc::new)
                .boxed(),
            )
        };

        let overlay = {
            cloned!(repo);
            async move {
                let bookmarks = list_hg_server_bookmarks(hg_repo_path).await?;

                let bookmarks = stream::iter(bookmarks.into_iter())
                    .map(|(book, hg_cs_id)| {
                        cloned!(repo);
                        async move {
                            let maybe_bcs_id = repo
                                .bonsai_hg_mapping()
                                .get_bonsai_from_hg(ctx, hg_cs_id)
                                .await?;
                            Result::<_, Error>::Ok(maybe_bcs_id.map(|bcs_id| (book, bcs_id)))
                        }
                    })
                    .buffered(100)
                    .try_filter_map(|x| future::ready(Ok(x)))
                    .try_collect::<HashMap<_, _>>()
                    .await?;

                Ok(BookmarkOverlay::new(Arc::new(bookmarks)))
            }
        };

        let globalrev_config = repo_config.pushrebase.globalrev_config.as_ref();

        let filter_changesets: Arc<dyn FilterExistingChangesets> =
            if let Some(backup_repo) = maybe_darkstorm_backup_repo.clone() {
                Arc::new(DarkstormBackupChangesetsFilter { repo: backup_repo })
            } else {
                Arc::new(NoopChangesetsFilter {})
            };

        borrowed!(maybe_darkstorm_backup_repo);
        let globalrev_syncer = {
            cloned!(repo);
            async move {
                match globalrev_config {
                    Some(config) if config.small_repo_id.is_none() => {
                        match maybe_darkstorm_backup_repo {
                            Some(darkstorm_backup_repo) => {
                                Ok(GlobalrevSyncer::darkstorm(&repo, darkstorm_backup_repo))
                            }
                            None => Ok(GlobalrevSyncer::Noop),
                        }
                    }
                    _ => Ok(GlobalrevSyncer::Noop),
                }
            }
        };

        (
            repo,
            filter_changesets,
            try_join3(preparer, overlay, globalrev_syncer),
        )
    };

    let batch_size = if repo_name == "configerator" {
        args::get_usize(
            matches,
            "configerator-batch-size",
            DEFAULT_CONFIGERATOR_BATCH_SIZE,
        )
    } else {
        match job_config.as_ref().map(|c| c.batch_size) {
            Some(size) => size as usize,
            None => args::get_usize(matches, "batch-size", DEFAULT_BATCH_SIZE),
        }
    };

    let single_bundle_timeout_ms = args::get_u64(
        matches,
        "single-bundle-timeout-ms",
        DEFAULT_SINGLE_BUNDLE_TIMEOUT_MS,
    );
    let verify_server_bookmark_on_failure = matches.is_present("verify-server-bookmark-on-failure");
    let hg_repo = hgrepo::HgRepo::new(
        hg_repo_path,
        batch_size,
        single_bundle_timeout_ms,
        verify_server_bookmark_on_failure,
    )?;
    let bookmarks =
        args::open_sql_with_config::<SqlBookmarksBuilder>(ctx.fb, matches, &resolved_repo.config)
            .await?;

    let bookmarks = bookmarks.with_repo_id(repo_id);
    let reporting_handler = build_reporting_handler(ctx, &scuba_sample, retry_num, &bookmarks);
    let lock_on_failure = match job_config.as_ref().map(|c| c.lock_on_failure) {
        Some(lock_on_failure) => lock_on_failure,
        None => matches.is_present("lock-on-failure"),
    };

    let sql_repo_lock = SqlRepoLock::with_metadata_database_config(
        ctx.fb,
        &repo_config.storage_config.metadata,
        mysql_options,
        readonly_storage.0,
    )
    .await?;

    let repo_lock: Arc<dyn RepoLock> = Arc::new(MutableRepoLock::new(sql_repo_lock, repo_id));

    let unlock_via = repo_lock.clone();

    let lock_via = if lock_on_failure {
        Some(repo_lock.as_ref())
    } else {
        None
    };

    // Before beginning any actual processing, check if cancellation has been requested.
    // If yes, then lets return early.
    if cancellation_requested.load(Ordering::Relaxed) {
        info!(ctx.logger(), "sync stopping due to cancellation request");
        return Ok(());
    }
    match matches.subcommand() {
        // In sync-mode, the command will auto-terminate after one iteration.
        // Hence, no need to check cancellation flag.
        (MODE_SYNC_ONCE, Some(sub_m)) => {
            let start_id = args::get_u64_opt(&sub_m, "start-id")
                .ok_or_else(|| Error::msg("--start-id must be specified"))?
                .into();

            let (maybe_log_entry, (bundle_preparer, overlay, globalrev_syncer)) = try_join(
                bookmarks
                    .read_next_bookmark_log_entries(
                        ctx.clone(),
                        start_id,
                        1u64,
                        Freshness::MaybeStale,
                    )
                    .try_next(),
                repo_parts,
            )
            .await?;
            if let Some(log_entry) = maybe_log_entry {
                let (stats, res) = async {
                    let batches = bundle_preparer
                        .prepare_batches(
                            &ctx.clone(),
                            Arc::new(Mutex::new(overlay)),
                            vec![log_entry.clone()],
                            1, // Won't be respected anyway since we're batching a single entry.
                        )
                        .await?;
                    let mut combined_entries = bundle_preparer
                        .prepare_bundles(ctx.clone(), batches, filter_changesets)
                        .await?;

                    let combined_entry = combined_entries.remove(0);
                    sync_single_combined_entry(
                        ctx,
                        &combined_entry,
                        &hg_repo,
                        base_retry_delay_ms,
                        retry_num,
                        &globalrev_syncer,
                    )
                    .await
                }
                .timed()
                .await;

                let res = bind_sync_result(&[log_entry], res);
                let res = match res {
                    Ok(ok) => Ok((stats, ok)),
                    Err(err) => Err((Some(stats), err)),
                };
                let res = reporting_handler(res).await;
                let _ = build_outcome_handler(ctx, lock_via)(res).await?;
                Ok(())
            } else {
                info!(ctx.logger(), "no log entries found");
                Ok(())
            }
        }
        (MODE_SYNC_LOOP, Some(sub_m)) => {
            let start_id: Option<BookmarkUpdateLogId> =
                args::get_u64_opt(&sub_m, "start-id").map(|id| id.into());
            let bundle_buffer_size = args::get_usize_opt(&sub_m, "bundle-buffer-size").unwrap_or(5);
            let combine_bundles = args::get_u64_opt(&sub_m, "combine-bundles").unwrap_or(1);
            let max_commits_per_combined_bundle =
                args::get_u64_opt(&sub_m, "max-commits-per-combined-bundle").unwrap_or(100);
            let skip_bookmarks = sub_m
                .values_of("skip-bookmark")
                .map_or(Vec::new(), |v| v.collect());
            let replace_changesets: HashMap<Option<ChangesetId>, Option<ChangesetId>> = sub_m
                .values_of("replace-changeset")
                .map_or(HashMap::new(), |v| {
                    v.map(|cs_pair| {
                        let mut split = cs_pair.split(':');
                        (
                            Some(split.next().unwrap().parse().unwrap()),
                            Some(split.next().unwrap().parse().unwrap()),
                        )
                    })
                    .collect()
                });
            let loop_forever = sub_m.is_present("loop-forever");
            let replayed_sync_counter = LatestReplayedSyncCounter::new(
                &repo,
                maybe_darkstorm_backup_repo.as_ref().map(|r| r.as_ref()),
            )?;
            let exit_path: Option<PathBuf> = sub_m
                .value_of("exit-file")
                .map(|name| Path::new(name).to_path_buf());

            borrowed!(ctx);
            let can_continue = move || {
                let exit_file_exists = match exit_path {
                    Some(ref exit_path) if exit_path.exists() => {
                        info!(ctx.logger(), "path {:?} exists: exiting ...", exit_path);
                        true
                    }
                    _ => false,
                };
                let cancelled = if cancellation_requested.load(Ordering::Relaxed) {
                    info!(ctx.logger(), "sync stopping due to cancellation request");
                    true
                } else {
                    false
                };
                !exit_file_exists && !cancelled
            };
            let counter = replayed_sync_counter
                .get_counter(ctx)
                .and_then(move |maybe_counter| {
                    future::ready(maybe_counter.map(|counter| counter.try_into().expect("Counter must be positive")).or(start_id).ok_or_else(|| {
                        format_err!(
                            "{} counter not found. Pass `--start-id` flag to set the counter",
                            LATEST_REPLAYED_REQUEST_KEY
                        )
                    }))
                });

            let (start_id, (bundle_preparer, overlay, globalrev_syncer)) =
                try_join(counter, repo_parts).watched(ctx.logger()).await?;

            let outcome_handler = build_outcome_handler(ctx, lock_via);
            let overlay = &Arc::new(Mutex::new(overlay));
            borrowed!(
                outcome_handler,
                can_continue,
                hg_repo,
                reporting_handler,
                globalrev_syncer,
                replayed_sync_counter,
                bundle_preparer: &Arc<BundlePreparer>,
                filter_changesets: &Arc<dyn FilterExistingChangesets>,
            );

            let mut seen_first = false;
            loop_over_log_entries(
                ctx,
                &bookmarks,
                start_id,
                loop_forever,
                &scuba_sample,
                combine_bundles,
                unlock_via,
            )
            .try_filter(|entries| future::ready(!entries.is_empty()))
            .map_ok(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| !skip_bookmarks.contains(&entry.bookmark_name.as_str()))
                    .map(|entry| {
                        let from_changeset_id = replace_changesets
                            .get(&entry.from_changeset_id)
                            .cloned()
                            .unwrap_or(entry.from_changeset_id);
                        let to_changeset_id = replace_changesets
                            .get(&entry.to_changeset_id)
                            .cloned()
                            .unwrap_or(entry.to_changeset_id);
                        BookmarkUpdateLogEntry {
                            from_changeset_id,
                            to_changeset_id,
                            ..entry
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .fuse()
            .try_next_step(|entries| async move {
                bundle_preparer
                    .prepare_batches(
                        ctx,
                        overlay.clone(),
                        entries,
                        max_commits_per_combined_bundle,
                    )
                    .await
            })
            // Compiler bug: Prevents higher-ranked lifetime error
            .boxed()
            .try_filter(|batches| future::ready(!batches.is_empty()))
            .map_ok(|mut batches| {
                if !seen_first {
                    // In the case that the sync job failed to update the
                    // "latest-replayed-request" counter during its previous
                    // run, the first batch might contain entries that were
                    // already synced to the hg server. Syncing them again
                    // would result in an error. Let's try to detect this case
                    // and adjust the first batch to skip the already-synced
                    // commits, if possible.
                    if let Some(first) = batches.first_mut() {
                        first.maybe_adjust(ctx);
                        seen_first = true;
                    }
                }
                batches
            })
            .map_ok(|batches| async move {
                anyhow::Ok(
                    bundle_preparer
                        .prepare_bundles(ctx.clone(), batches, filter_changesets.clone())
                        .watched(ctx.logger())
                        .await?,
                )
            })
            .try_buffered(bundle_buffer_size)
            .fuse()
            .try_next_step(|bundles: Vec<CombinedBookmarkUpdateLogEntry>| async move {
                for bundle in bundles {
                    if !can_continue() {
                        break;
                    }
                    let (stats, res) = sync_single_combined_entry(
                        ctx,
                        &bundle,
                        hg_repo,
                        base_retry_delay_ms,
                        retry_num,
                        globalrev_syncer,
                    )
                    .watched(ctx.logger())
                    .timed()
                    .await;
                    let res = bind_sync_result(&bundle.components, res);
                    let res = match res {
                        Ok(ok) => Ok((stats, ok)),
                        Err(err) => Err((Some(stats), err)),
                    };
                    let res = reporting_handler(res).watched(ctx.logger()).await;
                    let entry = outcome_handler(res).watched(ctx.logger()).await?;
                    let next_id = get_id_to_search_after(&entry);

                    retry_always(
                        ctx.logger(),
                        |_| async {
                            let success = replayed_sync_counter
                                .set_counter(ctx, next_id.try_into()?)
                                .watched(ctx.logger())
                                .await?;

                            if success {
                                Ok(())
                            } else {
                                bail!("failed to update counter")
                            }
                        },
                        base_retry_delay_ms,
                        retry_num,
                    )
                    .watched(ctx.logger())
                    .await?;
                }
                Ok(())
            })
            .try_collect::<()>()
            .await
        }
        _ => bail!("incorrect mode of operation is specified"),
    }
}

#[fbinit::main]
fn main(fb: FacebookInit) -> Result<()> {
    let process = HgSyncProcess::new(fb)?;
    match process.matches.value_of("sharded-service-name") {
        Some(service_name) => {
            // The service name needs to be 'static to satisfy SM contract
            static SM_SERVICE_NAME: OnceLock<String> = OnceLock::new();
            let logger = process.matches.logger().clone();
            let matches = Arc::clone(&process.matches);
            let mut executor = ShardedProcessExecutor::new(
                process.fb,
                process.matches.runtime().clone(),
                &logger,
                SM_SERVICE_NAME.get_or_init(|| service_name.to_string()),
                SM_SERVICE_SCOPE,
                SM_CLEANUP_TIMEOUT_SECS,
                Arc::new(process),
                true, // enable shard (repo) level healing
            )?;
            block_execute(
                executor.block_and_execute(&logger, Arc::new(AtomicBool::new(false))),
                fb,
                &std::env::var("TW_JOB_NAME").unwrap_or_else(|_| "hg_sync_job".to_string()),
                matches.logger(),
                &matches,
                cmdlib::monitoring::AliveService,
            )?;
        }
        None => {
            let matches = process.matches.clone();
            let config_store = matches.config_store();
            let (repo_name, _) =
                args::not_shardmanager_compatible::get_config(config_store, &matches)?;
            let ctx = CoreContext::new_with_logger_and_client_info(
                fb,
                matches.logger().clone(),
                ClientInfo::default_with_entry_point(ClientEntryPoint::MononokeHgSync),
            )
            .clone_with_repo_name(&repo_name);
            let fut = run(&ctx, &matches, repo_name, Arc::new(AtomicBool::new(false)));
            block_execute(
                fut,
                fb,
                "hg_sync_job",
                matches.logger(),
                &matches,
                cmdlib::monitoring::AliveService,
            )?;
        }
    }
    Ok(())
}
