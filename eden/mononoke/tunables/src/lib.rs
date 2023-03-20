/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::thread_local;
use std::time::Duration;

use anyhow::Result;
use arc_swap::ArcSwap;
use arc_swap::ArcSwapOption;
use cached_config::ConfigHandle;
use futures::future::poll_fn;
use futures::Future;
use futures::FutureExt;
use once_cell::sync::OnceCell;
use slog::debug;
use slog::error;
use slog::warn;
use slog::Logger;
use stats::prelude::*;
use tokio::runtime::Handle;
use tunables_derive::Tunables;
use tunables_structs::Tunables as TunablesStruct;

define_stats! {
    prefix = "mononoke.tunables";
    refresh_failure_count: timeseries(Average, Sum, Count),
}

static TUNABLES: OnceCell<MononokeTunables> = OnceCell::new();
static TUNABLES_WORKER_STATE: OnceCell<TunablesWorkerState> = OnceCell::new();

thread_local! {
    static TUNABLES_OVERRIDE: RefCell<Option<Arc<MononokeTunables>>> = RefCell::new(None);
}

pub enum TunablesReference {
    Override(Arc<MononokeTunables>),
    Static(&'static MononokeTunables),
}

impl Deref for TunablesReference {
    type Target = MononokeTunables;

    fn deref(&self) -> &MononokeTunables {
        match self {
            Self::Override(r) => r.as_ref(),
            Self::Static(r) => r,
        }
    }
}

pub fn tunables() -> TunablesReference {
    TUNABLES_OVERRIDE.with(|tunables_override| match *tunables_override.borrow() {
        Some(ref arc) => TunablesReference::Override(arc.clone()),
        None => TunablesReference::Static(TUNABLES.get_or_init(MononokeTunables::default)),
    })
}

// These types exist to simplify code generation in tunables-derive
pub type TunableBool = ArcSwapOption<bool>;
pub type TunableI64 = ArcSwapOption<i64>;

pub type TunableString = ArcSwapOption<String>;
pub type TunableVecOfStrings = ArcSwapOption<Vec<String>>;

pub type TunableBoolByRepo = ArcSwap<HashMap<String, bool>>;
pub type TunableStringByRepo = ArcSwap<HashMap<String, String>>;
pub type TunableVecOfStringsByRepo = ArcSwap<HashMap<String, Vec<String>>>;
pub type TunableI64ByRepo = ArcSwap<HashMap<String, i64>>;

#[derive(Tunables, Default, Debug)]
pub struct MononokeTunables {
    mutation_advertise_for_infinitepush: TunableBool,
    mutation_accept_for_infinitepush: TunableBool,
    mutation_generate_for_draft: TunableBool,
    warm_bookmark_cache_poll_interval_ms: TunableI64,
    /// Don't read from the BookmarksSubscription when updating the WBC, and instead poll for the
    /// entire list of bookmarks on every iteration.
    warm_bookmark_cache_disable_subscription: TunableBool,
    /// Maximum age of bookmarks subscriptions.
    bookmark_subscription_max_age_ms: TunableI64,
    bookmark_subscription_protect_master: TunableBool,
    max_scuba_msg_length: TunableI64,
    wishlist_read_qps: TunableI64,
    wishlist_write_qps: TunableI64,
    edenapi_req_dumper_sample_ratio: TunableI64,
    command_monitor_interval: TunableI64,
    command_monitor_remote_logging: TunableI64,
    // Log all getfiles/gettreepack requests for paths that start with prefix
    // in a particular repo
    undesired_path_repo_name_to_log: TunableString,
    undesired_path_prefix_to_log: TunableString,
    undesired_path_regex_to_log: TunableString,
    pushrebase_disable_rebased_commit_validation: TunableBool,
    filenodes_disabled: TunableBool,
    filenodes_master_fallback_ratio: TunableI64,
    // Skiplist config
    skiplist_max_skips_without_yield: TunableI64,
    skiplist_reload_disabled: TunableBool,
    skiplist_reload_interval: TunableI64,
    deduplicated_put_sampling_rate: TunableI64,
    disable_repo_client_warm_bookmarks_cache: TunableBool,
    remotefilelog_file_history_limit: TunableI64,
    disable_hooks_on_plain_push: TunableBool,
    run_hooks_on_additional_changesets: TunableBool,
    hooks_additional_changesets_limit: TunableI64,
    // SCS scuba sampling knobs
    scs_popular_methods_sampling_rate: TunableI64,
    scs_other_methods_sampling_rate: TunableI64,
    // When false error logs are never sampled
    scs_error_log_sampling: TunableBool,
    redacted_logging_sampling_rate: TunableI64,
    redaction_config_from_xdb: TunableBool,
    getbundle_use_low_gen_optimization: TunableBool,
    getbundle_high_low_gen_num_difference_threshold: TunableI64,
    getbundle_low_gen_optimization_max_traversal_limit: TunableI64,
    getbundle_partial_getbundle_traversal_limit: TunableI64,
    repo_client_bookmarks_timeout_secs: TunableI64,
    repo_client_clone_timeout_secs: TunableI64,
    repo_client_default_timeout_secs: TunableI64,
    repo_client_getbundle_timeout_secs: TunableI64,
    repo_client_getpack_timeout_secs: TunableI64,
    repo_client_concurrent_blob_uploads: TunableI64,
    repo_client_max_nodes_in_known_method: TunableI64,
    // How many trees is getting prepared at once
    repo_client_gettreepack_buffer_size: TunableI64,
    derived_data_slow_derivation_threshold_secs: TunableI64,
    disable_running_hooks_in_pushredirected_repo: TunableBool,
    scs_request_read_qps: TunableI64,
    scs_request_write_qps: TunableI64,
    enable_logging_commit_rewrite_data: TunableBool,
    // All blobstore read request with size bigger than
    // this threshold will be logged to scuba
    blobstore_read_size_logging_threshold: TunableI64,
    hash_validation_percentage: TunableI64,
    // Filter out commits that we already have in infinitepush. Shouldn't be needed if we have a
    // client exchanging commits with us, but when processing bundled uploads (i.e. commit cloud
    // filling), it might help a lot.
    filter_pre_existing_commits_on_infinitepush: TunableBool,
    backfill_read_qps: TunableI64,
    backfill_write_qps: TunableI64,
    disable_commit_scribe_logging_scs: TunableBool,
    xrepo_sync_disable_all_syncs: TunableBool,
    xrepo_disable_commit_sync_lease: TunableBool,

    // Use Background session class while deriving data. This makes derived data not write
    // data to blobstore sync queue if a write was successful to the main blobstore.
    derived_data_use_background_session_class: TunableBoolByRepo,
    commit_cloud_use_background_session_class: TunableBool,
    multiplex_blobstore_background_session_timeout_ms: TunableI64,

    allow_change_xrepo_mapping_extra: TunableBool,

    // Disable EdenAPI in http_service.
    disable_http_service_edenapi: TunableBool,

    // Disable putting hydrating manifests in .hg
    disable_hydrating_manifests_in_dot_hg: TunableBool,

    // Rendez vous configuration.
    rendezvous_dispatch_delay_ms: TunableI64,
    rendezvous_dispatch_max_threshold: TunableI64,

    unbundle_limit_num_of_commits_in_push: TunableI64,

    // Maximium negative caching age of a blob, in milliseconds
    // Negative means to not use weak consistency at all
    manifold_weak_consistency_max_age_ms: TunableI64,

    // -1: No override, use manifold server side config and rollout checks
    // if set to > -1, set the client side option to override (see manifoldblob code)
    manifold_request_priority_override: TunableI64,

    // Frequency at which to collect SQL connection pool stats
    sql_connection_pool_stats_collection_interval_ms: TunableI64,

    bookmarks_cache_ttl_ms: TunableI64,

    // Disable running SaveMappingPushrebaseHook on every Pushrebase
    disable_save_mapping_pushrebase_hook: TunableBool,

    // Set to 0 to disable compression
    zstd_compression_level: TunableI64,

    // Commits that aren't related (i.e. that are not ancestors of each other)
    // can be derived in parallel, and that's what derived data does.
    // derived_data_parallel_derivation_buffer limits
    // how many commits will be derived at once.
    derived_data_parallel_derivation_buffer: TunableI64,

    // Tunables to disable derived data derivation either for the full repo
    // or for specific derived data types inside a repo
    all_derived_data_disabled: TunableBoolByRepo,
    derived_data_types_disabled: TunableVecOfStringsByRepo,
    // How often to check if derived data is disabled or not
    derived_data_disabled_watcher_delay_secs: TunableI64,

    // Stops deriving on derivation workers. Will not drain Derivation Queue
    derived_data_disable_derivation_workers: TunableBool,

    // How long to wait before worker retries in case of an error
    // or empty Derivation queue.
    derivation_worker_sleep_duration: TunableI64,

    // How long client should wait between polls of Derived data service
    derivation_request_retry_delay: TunableI64,

    // Sets the size of the batch for derivaiton.
    derivation_batch_size: TunableI64,

    // Maximum time to wait for remote derivation request to finish in secs
    // before falling back to local derivation
    remote_derivation_fallback_timeout_secs: TunableI64,

    // Timeout for derivation request on service.
    dds_request_timeout: TunableI64,

    // Disable the parallel derivation for DM and default to serial
    deleted_manifest_disable_new_parallel_derivation: TunableBool,

    // multiplexed blobstore is_present/get new semantics rollout
    multiplex_blobstore_get_do_queue_lookup: TunableBool,
    multiplex_blobstore_is_present_do_queue_lookup: TunableBool,

    // Not in use.
    // TODO(mitrandir): clean it up
    fastlog_use_mutable_renames: TunableBoolByRepo,
    // Disable mutable renames for fastlog in case they cause problems.
    fastlog_disable_mutable_renames: TunableBoolByRepo,
    megarepo_api_dont_set_file_mutable_renames: TunableBool,
    megarepo_api_dont_set_directory_mutable_renames: TunableBool,

    force_unode_v2: TunableBool,
    fastlog_use_gen_num_traversal: TunableBool,

    // Changing the value of this tunable forces all mononoke instances
    // to reload segmented changelog. One can also specify jitter (or use default)
    segmented_changelog_force_reload: TunableI64ByRepo,
    segmented_changelog_force_reload_jitter_secs: TunableI64,

    // Override default progress logging sampling rate for segmented changelog parts
    segmented_changelog_idmap_log_sampling_rate: TunableI64,
    segmented_changelog_tailer_log_sampling_rate: TunableI64,

    // How many commits to walk back from the client heads before failing to rebuild SC
    segmented_changelog_client_max_commits_to_traverse: TunableI64,

    // Timeout for is_present call for multiplexed blobstore
    is_present_timeout_ms: TunableI64,

    // What timeout to use when doing filenode lookup.
    // Usually filenode lookup is used while generating hg changesets
    filenode_lookup_timeout_ms: TunableI64,

    // Sampling ratio percentage for warm boomark cache.
    warm_bookmark_cache_logging_sampling_pct: TunableI64,

    // Setting this tunable to a new non-zero value and restarting
    // mononoke hosts will invalidate the cache
    blobstore_memcache_sitever: TunableI64,
    // Setting this tunable to a new non-zero value and restarting
    // mononoke hosts will invalidate the cache
    sql_memcache_sitever: TunableI64,

    // Setting this tunable to a new non-zero value and restarting
    // mononoke hosts will invalidate bonsai_hg_mapping cache
    bonsai_hg_mapping_sitever: TunableI64,

    // Setting this tunable to a new non-zero value will update the
    // TTL for the mutation store cache
    hg_mutation_store_caching_ttl_secs: TunableI64,

    // Setting this tunable to a new non-zero value and restarting
    // mononoke hosts will invalidate hg mutation store cache
    hg_mutation_store_sitever: TunableI64,

    // EdenAPI requests that take long than this get logged unsampled
    edenapi_unsampled_duration_threshold_ms: TunableI64,

    // Setting this tunable to a new non-zero value and restarting
    // mononoke hosts will invalidate mutable renames cache
    mutable_renames_sitever: TunableI64,

    // Setting this will enable hooks on pushrebase bookmark moves that were
    // initiated by a service (for user-initiated moves we always run hooks).
    // Most of them will run in no-op mode but some of them (namely,
    // verify_integrity) will need to do some work like logging and this is
    // why it may be useful to run them.
    enable_hooks_on_service_pushrebase: TunableBool,

    // Control whether the BYPASS_READONLY pushvar is restricted by an ACL
    enforce_bypass_readonly_acl: TunableBool,

    // Boolean to batch requests sent to Land Service
    batching_to_land_service: TunableBool,

    // Which region writes should be done to, in order to minimise latency.
    // This should align with underlying storage (SQL/Manifold) write regions.
    // Notice writes still work from any region, and this field is not necessarily
    // enforced.
    preferred_write_region: TunableString,

    // The replication_status call is problematic for SQL so we're experimenting
    // with removing it, but this tunable can be used as a quick killswitch to
    // enable them again.
    sql_lag_monitoring_blocklist: TunableVecOfStrings,

    // If set, the hook won't be created at all
    disable_check_write_permissions_hook: TunableBool,
    // If set, the check result will be discarded for user identities
    log_only_for_users_in_cwp_hook: TunableBool,
    // If set, the check result will be discarded for service identities
    log_only_for_services_in_cwp_hook: TunableBool,

    // If set, the wireproto implementation will only log the repo write ACL
    // check result.
    log_only_wireproto_write_acl: TunableBool,

    // If set the `draft` ACL action will be checked and logged on draft access
    // Unless `enforce_draft_acl` is set `read` action will still be used for
    // granting access.
    log_draft_acl_failures: TunableBool,

    // If set the `draft` ACL action will be used for `draft` access.
    enforce_draft_acl: TunableBool,

    // Force local pushrebase instead of talking to SCS or Land Service
    force_local_pushrebase: TunableBool,

    // Disable SCS pushredirecting commits landed on a small repo to a big one
    disable_scs_pushredirect: TunableBool,

    // Enable usage of basename_suffix_skeleton_manifest in commit_find_files
    disable_basename_suffix_skeleton_manifest: TunableBool,
    // Enable using BSSM for suffix queries. Might be inneficient for broad suffixes (like .php)
    enable_bssm_suffix_query: TunableBool,

    // List of targets in AOSP megarepo to apply squashing config overrides
    megarepo_squashing_config_override_targets: TunableVecOfStringsByRepo,
    // Override squashing limit for listed targets
    megarepo_override_squashing_limit: TunableI64ByRepo,
    // Override author check during squashing
    megarepo_override_author_check: TunableBoolByRepo,

    // Disable SQL queries being retried after admission control errors
    disable_sql_auto_retries: TunableBool,
    // Disable SQL queries being cached using `cacheable` keyword
    disable_sql_auto_cache: TunableBool,
    // Disable using rendezvous for batching WAL deletes.
    // TODO: delete once it using WAL here shows to be stable
    wal_disable_rendezvous_on_deletes: TunableBool,
    // Enable derivation on service per repo
    enable_remote_derivation: TunableBoolByRepo,

    // Enable reading from the new commit graph
    enable_reading_from_new_commit_graph: TunableBoolByRepo,
    // Enable writing to the new commit graph (double writes to both
    // the old changesets and csparents tables, and the new
    // commit_graph_edges and commit_graph_merge_parents tables)
    enable_writing_to_new_commit_graph: TunableBoolByRepo,

    // Usage of new commit graph for speeding up server-side operations
    new_commit_graph_is_ancestor_percentage: TunableI64ByRepo,

    // Disable all prefetching in the commit graph
    disable_commit_graph_prefetch: TunableBool,

    // Disable the fix to use isolation level read committed
    disable_wal_read_committed: TunableBool,
}

fn log_tunables(tunables: &TunablesStruct) -> String {
    serde_json::to_string(tunables)
        .unwrap_or_else(|e| format!("failed to serialize tunables: {}", e))
}

pub fn init_tunables_worker(
    logger: Logger,
    config_handle: ConfigHandle<TunablesStruct>,
    runtime_handle: Handle,
) -> Result<()> {
    init_tunables(&logger, &config_handle)?;
    if TUNABLES_WORKER_STATE
        .set(TunablesWorkerState {
            config_handle,
            logger,
        })
        .is_err()
    {
        panic!("Two or more tunables update threads exist at the same time");
    }
    runtime_handle.spawn(wait_and_update());

    Ok(())
}

pub fn init_tunables(logger: &Logger, config_handle: &ConfigHandle<TunablesStruct>) -> Result<()> {
    let tunables = config_handle.get();
    debug!(logger, "Initializing tunables: {}", log_tunables(&tunables));
    update_tunables(tunables)
}

/// Tunables are updated when the underlying config source notifies of a change.
/// Call this to force update them to the latest value provided by the config source.
/// Meant to be used in tests.
/// NOTE: if tunables are fetched from Configerator, you need to force update it as well.
pub fn force_update_tunables() {
    let state = TUNABLES_WORKER_STATE
        .get()
        .expect("Tunables worker state uninitialised");
    wait_and_update_iteration(state.config_handle.get(), &state.logger);
}

struct TunablesWorkerState {
    config_handle: ConfigHandle<TunablesStruct>,
    logger: Logger,
}

async fn wait_and_update() {
    let state = TUNABLES_WORKER_STATE
        .get()
        .expect("Tunables worker state uninitialised");
    let mut config_watcher = state
        .config_handle
        .watcher()
        .expect("Tunable backed by static config source");
    loop {
        match config_watcher.wait_for_next().await {
            Ok(new_tunables) => wait_and_update_iteration(new_tunables, &state.logger),
            Err(e) => {
                error!(
                    state.logger,
                    "Error in fetching latest config for tunable: {}.\n Exiting tunable updater", e
                );
                // Set the refresh failure count counter so that the oncall can be alerted
                // based on this metric
                STATS::refresh_failure_count.add_value(1);
                return;
            }
        }
    }
}

fn wait_and_update_iteration(new_tunables: Arc<TunablesStruct>, logger: &Logger) {
    debug!(
        logger,
        "Updating tunables to new: {}",
        log_tunables(&new_tunables),
    );
    if let Err(e) = update_tunables(new_tunables) {
        warn!(logger, "Failed to refresh tunables: {}", e);
        // Set the refresh failure count counter so that the oncall can be alerted
        // based on this metric
        STATS::refresh_failure_count.add_value(1);
    } else {
        // Add a value of 0 so that the counter won't get dead even if there
        // are no errors
        STATS::refresh_failure_count.add_value(0);
    }
}

fn update_tunables(new_tunables: Arc<TunablesStruct>) -> Result<()> {
    let tunables = tunables();
    tunables.update_bools(&new_tunables.killswitches);
    tunables.update_ints(&new_tunables.ints);
    tunables.update_strings(&new_tunables.strings);
    tunables.update_vec_of_strings(&new_tunables.vec_of_strings);

    if let Some(killswitches_by_repo) = &new_tunables.killswitches_by_repo {
        tunables.update_by_repo_bools(killswitches_by_repo);
    }

    if let Some(ints_by_repo) = &new_tunables.ints_by_repo {
        tunables.update_by_repo_ints(ints_by_repo);
    }

    if let Some(vec_of_strings_by_repo) = &new_tunables.vec_of_strings_by_repo {
        tunables.update_by_repo_vec_of_strings(vec_of_strings_by_repo);
    }
    Ok(())
}

/// A helper function to override tunables during a closure's execution.
/// This is useful for unit tests.
pub fn with_tunables<T>(new_tunables: MononokeTunables, f: impl FnOnce() -> T) -> T {
    TUNABLES_OVERRIDE.with(|t| *t.borrow_mut() = Some(Arc::new(new_tunables)));

    let res = f();

    TUNABLES_OVERRIDE.with(|tunables| *tunables.borrow_mut() = None);

    res
}

pub fn with_tunables_async<Out, Fut: Future<Output = Out> + Unpin>(
    new_tunables: MononokeTunables,
    fut: Fut,
) -> impl Future<Output = Out> {
    with_tunables_async_arc(Arc::new(new_tunables), fut)
}

pub fn with_tunables_async_arc<Out, Fut: Future<Output = Out> + Unpin>(
    new_tunables: Arc<MononokeTunables>,
    mut fut: Fut,
) -> impl Future<Output = Out> {
    poll_fn(move |cx| {
        TUNABLES_OVERRIDE.with(|t| *t.borrow_mut() = Some(new_tunables.clone()));

        let res = fut.poll_unpin(cx);

        TUNABLES_OVERRIDE.with(|tunables| *tunables.borrow_mut() = None);

        res
    })
}

pub fn override_tunables(new_tunables: Option<Arc<MononokeTunables>>) {
    TUNABLES_OVERRIDE.with(|t| *t.borrow_mut() = new_tunables);
}

/// Get a duration from a tunable or a provided default
/// value if the tunable is not set.
pub fn get_duration_from_tunable_or(
    get_tunable: impl Fn(&MononokeTunables) -> Option<i64>,
    into_duration: impl Fn(u64) -> Duration,
    default: u64,
) -> Duration {
    into_duration(get_tunable(tunables().deref()).map_or(default, |i| i as u64))
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use maplit::hashmap;

    use super::*;

    #[derive(Tunables, Default)]
    struct TestTunables {
        boolean: TunableBool,
        num: TunableI64,
        string: TunableString,
        vecofstrings: TunableVecOfStrings,

        repobool: TunableBoolByRepo,
        repobool2: TunableBoolByRepo,

        repoint: TunableI64ByRepo,
        repoint2: TunableI64ByRepo,

        repostr: TunableStringByRepo,
        repostr2: TunableStringByRepo,

        repovecofstrings: TunableVecOfStringsByRepo,
    }

    #[derive(Tunables, Default)]
    struct EmptyTunables {}

    fn s(a: &str) -> String {
        a.to_string()
    }

    #[test]
    fn test_override_tunables() {
        assert!(tunables().wishlist_write_qps().is_none());

        let res = with_tunables(
            MononokeTunables {
                wishlist_write_qps: ArcSwapOption::from(Some(Arc::new(2))),
                ..MononokeTunables::default()
            },
            || tunables().wishlist_write_qps().unwrap_or_default(),
        );

        assert_eq!(res, 2);
        assert!(tunables().wishlist_write_qps().is_none());
    }

    #[test]
    fn test_empty_tunables() {
        let bools = HashMap::new();
        let ints = HashMap::new();
        let empty = EmptyTunables::default();

        empty.update_bools(&bools);
        empty.update_ints(&ints);
        empty.update_strings(&HashMap::new());
        empty.update_vec_of_strings(&HashMap::new());
    }

    #[test]
    fn test_update_bool() {
        let mut d = HashMap::new();
        d.insert(s("boolean"), true);

        let test = TestTunables::default();
        assert!(test.boolean().is_none());
        test.update_bools(&d);
        assert_eq!(test.boolean(), Some(true));
    }

    #[test]
    fn test_revert_update() {
        // booleans
        let mut d = hashmap! {};
        d.insert(s("boolean"), true);

        let test = TestTunables::default();
        assert!(test.boolean().is_none());
        test.update_bools(&d);
        assert_eq!(test.boolean(), Some(true));

        test.update_bools(&hashmap! {});
        assert!(test.boolean().is_none());

        // ints
        let test = TestTunables::default();
        test.update_ints(&hashmap! { s("num") => 1});
        assert_eq!(test.num(), Some(1));

        test.update_ints(&hashmap! {});
        assert_eq!(test.num(), None);

        // strings
        let test = TestTunables::default();
        test.update_strings(&hashmap! { s("string") => s("string")});
        assert_eq!(test.string(), Some(Arc::new(s("string"))));

        test.update_strings(&hashmap! {});
        assert!(test.string().is_none());

        // by repo bools
        assert_eq!(test.by_repo_repobool("repo"), None);
        assert_eq!(test.by_repo_repobool("repo2"), None);

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => true,
            },
            s("repo2") => hashmap! {
                s("repobool") => false,
            }
        });
        assert_eq!(test.by_repo_repobool("repo"), Some(true));
        assert_eq!(test.by_repo_repobool("repo2"), Some(false));

        test.update_by_repo_bools(&hashmap! {});
        assert_eq!(test.by_repo_repobool("repo"), None);
        assert_eq!(test.by_repo_repobool("repo2"), None);
    }

    #[test]
    fn test_update_int() {
        let mut d = HashMap::new();
        d.insert(s("num"), 10);

        let test = TestTunables::default();
        assert!(test.num().is_none());
        test.update_ints(&d);
        assert_eq!(test.num(), Some(10));
    }

    #[test]
    fn test_missing_int() {
        let mut d = HashMap::new();
        d.insert(s("missing"), 10);

        let test = TestTunables::default();
        assert!(test.num().is_none());
        test.update_ints(&d);
        assert!(test.num().is_none());
    }

    #[test]
    fn update_string() {
        let mut d = HashMap::new();
        d.insert(s("string"), s("value"));

        let test = TestTunables::default();
        assert!(test.string().is_none());
        test.update_strings(&d);
        assert_eq!(test.string().unwrap().as_str(), "value");
    }

    #[test]
    fn update_vec_of_strings() {
        let mut d = HashMap::new();
        d.insert(s("vecofstrings"), vec![s("value"), s("value2")]);

        let test = TestTunables::default();
        assert!(&test.vecofstrings().is_none());
        test.update_vec_of_strings(&d);
        assert_eq!(
            &test.vecofstrings().unwrap().as_slice(),
            &[s("value"), s("value2")]
        );
    }

    #[test]
    fn update_by_repo_bool() {
        let test = TestTunables::default();

        assert_eq!(test.by_repo_repobool("repo"), None);
        assert_eq!(test.by_repo_repobool("repo2"), None);

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => true,
            },
            s("repo2") => hashmap! {
                s("repobool") => true,
            }
        });
        assert_eq!(test.by_repo_repobool("repo"), Some(true));
        assert_eq!(test.by_repo_repobool("repo2"), Some(true));

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => true,
            }
        });
        assert_eq!(test.by_repo_repobool("repo2"), None);

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => false,
            }
        });
        assert_eq!(test.by_repo_repobool("repo"), Some(false));
    }

    #[test]
    fn update_by_repo_two_bools() {
        let test = TestTunables::default();
        assert_eq!(test.by_repo_repobool("repo"), None);
        assert_eq!(test.by_repo_repobool2("repo"), None);

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => true,
                s("repobool2") => true,
            }
        });

        assert_eq!(test.by_repo_repobool("repo"), Some(true));
        assert_eq!(test.by_repo_repobool2("repo"), Some(true));

        test.update_by_repo_bools(&hashmap! {
            s("repo") => hashmap! {
                s("repobool") => true,
                s("repobool2") => false,
            }
        });

        assert_eq!(test.by_repo_repobool("repo"), Some(true));
        assert_eq!(test.by_repo_repobool2("repo"), Some(false));
    }

    #[test]
    fn update_by_repo_str() {
        let test = TestTunables::default();

        assert_eq!(test.by_repo_repostr("repo"), None);
        assert_eq!(test.by_repo_repostr("repo2"), None);

        test.update_by_repo_strings(&hashmap! {
            s("repo") => hashmap! {
                s("repostr") => s("hello"),
            },
            s("repo2") => hashmap! {
                s("repostr") => s("world"),
            },
        });
        assert_eq!(test.by_repo_repostr("repo"), Some(s("hello")));
        assert_eq!(test.by_repo_repostr("repo2"), Some(s("world")));

        test.update_by_repo_strings(&hashmap! {
            s("repo") => hashmap! {
                s("repostr") => s("hello2"),
            },
        });
        assert_eq!(test.by_repo_repostr("repo"), Some(s("hello2")));
        assert_eq!(test.by_repo_repostr("repo2"), None);
    }

    #[test]
    fn update_by_repo_two_strs() {
        let test = TestTunables::default();
        assert_eq!(test.by_repo_repostr("repo"), None);
        assert_eq!(test.by_repo_repostr2("repo"), None);

        test.update_by_repo_strings(&hashmap! {
            s("repo") => hashmap! {
                s("repostr") => s("hello"),
                s("repostr2") => s("world"),
            }
        });

        assert_eq!(test.by_repo_repostr("repo"), Some(s("hello")));
        assert_eq!(test.by_repo_repostr2("repo"), Some(s("world")));

        test.update_by_repo_strings(&hashmap! {
            s("repo") => hashmap! {
                s("repostr") => s("hello2"),
            }
        });

        assert_eq!(test.by_repo_repostr("repo"), Some(s("hello2")));
        assert_eq!(test.by_repo_repostr2("repo"), None);
    }

    #[test]
    fn update_by_repo_int() {
        let test = TestTunables::default();

        assert_eq!(test.by_repo_repoint("repo"), None);
        assert_eq!(test.by_repo_repoint("repo2"), None);

        test.update_by_repo_ints(&hashmap! {
            s("repo") => hashmap! {
                s("repoint") => 1,
            },
            s("repo2") => hashmap! {
                s("repoint") => 2,
            },
        });
        assert_eq!(test.by_repo_repoint("repo"), Some(1));
        assert_eq!(test.by_repo_repoint("repo2"), Some(2));

        test.update_by_repo_ints(&hashmap! {
            s("repo") => hashmap! {
                s("repoint") => 3,
            },
        });
        assert_eq!(test.by_repo_repoint("repo"), Some(3));
        assert_eq!(test.by_repo_repoint("repo2"), None);
    }

    #[test]
    fn update_by_repo_two_ints() {
        let test = TestTunables::default();
        assert_eq!(test.by_repo_repoint("repo"), None);
        assert_eq!(test.by_repo_repoint2("repo"), None);

        test.update_by_repo_ints(&hashmap! {
            s("repo") => hashmap! {
                s("repoint") => 1,
                s("repoint2") => 2,
            }
        });

        assert_eq!(test.by_repo_repoint("repo"), Some(1));
        assert_eq!(test.by_repo_repoint2("repo"), Some(2));

        test.update_by_repo_ints(&hashmap! {
            s("repo") => hashmap! {
                s("repoint") => 3
            }
        });

        assert_eq!(test.by_repo_repoint("repo"), Some(3));
        assert_eq!(test.by_repo_repoint2("repo"), None);
    }

    #[test]
    fn update_by_repo_vec_of_strings() {
        let test = TestTunables::default();
        assert_eq!(test.by_repo_repovecofstrings("repo"), None);

        test.update_by_repo_vec_of_strings(&hashmap! {
            s("repo") => hashmap! {
                s("unrelated") => vec![s("val1"), s("val2")],
            }
        });
        assert_eq!(test.by_repo_repovecofstrings("repo"), None);

        test.update_by_repo_vec_of_strings(&hashmap! {
            s("repo") => hashmap! {
                s("repovecofstrings") => vec![s("val1"), s("val2")],
            }
        });

        assert_eq!(
            test.by_repo_repovecofstrings("repo"),
            Some(vec![s("val1"), s("val2")])
        );
    }

    #[fbinit::test]
    async fn test_with_tunables_async(_fb: fbinit::FacebookInit) {
        let res = with_tunables_async(
            MononokeTunables {
                wishlist_write_qps: ArcSwapOption::from(Some(Arc::new(2))),
                ..MononokeTunables::default()
            },
            async { tunables().wishlist_write_qps().unwrap_or_default() }.boxed(),
        )
        .await;

        assert_eq!(res, 2);
    }
}
