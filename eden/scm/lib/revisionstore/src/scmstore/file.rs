/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

mod fetch;
mod metrics;
mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ::types::fetch_mode::FetchMode;
use ::types::HgId;
use ::types::Key;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use anyhow::Result;
use cas_client::CasClient;
use clientinfo::get_client_request_info_thread_local;
use clientinfo::set_client_request_info_thread_local;
use crossbeam::channel::unbounded;
use itertools::Itertools;
use minibytes::Bytes;
use parking_lot::Mutex;
use parking_lot::RwLock;
use progress_model::AggregatingProgressBar;
use rand::Rng;
use tracing::debug;

pub(crate) use self::fetch::FetchState;
pub use self::metrics::FileStoreFetchMetrics;
pub use self::metrics::FileStoreMetrics;
pub use self::metrics::FileStoreWriteMetrics;
pub use self::types::FileAttributes;
pub use self::types::FileAuxData;
pub(crate) use self::types::LazyFile;
pub use self::types::StoreFile;
use crate::datastore::HgIdDataStore;
use crate::datastore::HgIdMutableDeltaStore;
use crate::datastore::RemoteDataStore;
use crate::indexedlogauxstore::AuxStore;
use crate::indexedlogdatastore::Entry;
use crate::indexedlogdatastore::IndexedLogHgIdDataStore;
use crate::lfs::lfs_from_hg_file_blob;
use crate::lfs::LfsPointersEntry;
use crate::lfs::LfsRemote;
use crate::lfs::LfsStore;
use crate::remotestore::HgIdRemoteStore;
use crate::scmstore::activitylogger::ActivityLogger;
use crate::scmstore::fetch::FetchResults;
use crate::scmstore::metrics::StoreLocation;
use crate::ContentDataStore;
use crate::ContentMetadata;
use crate::ContentStore;
use crate::Delta;
use crate::ExtStoredPolicy;
use crate::LegacyStore;
use crate::LocalStore;
use crate::Metadata;
use crate::MultiplexDeltaStore;
use crate::RepackLocation;
use crate::SaplingRemoteApiFileStore;
use crate::StoreKey;
use crate::StoreResult;

#[derive(Clone)]
pub struct FileStore {
    // Config
    // TODO(meyer): Move these to a separate config struct with default impl, etc.
    pub(crate) extstored_policy: ExtStoredPolicy,
    pub(crate) lfs_threshold_bytes: Option<u64>,
    pub(crate) edenapi_retries: i32,
    /// Allow explicitly writing serialized LFS pointers outside of tests
    pub(crate) allow_write_lfs_ptrs: bool,

    // Top level flag allow disabling all local computation of aux data.
    pub(crate) compute_aux_data: bool,
    // Make prefetch() calls request aux data.
    pub(crate) prefetch_aux_data: bool,

    // Largest set of keys prefetch() accepts before chunking.
    // Configured by scmstore.max-prefetch-size, where 0 means unlimited.
    pub(crate) max_prefetch_size: usize,

    // Local-only stores
    pub(crate) indexedlog_local: Option<Arc<IndexedLogHgIdDataStore>>,
    pub(crate) lfs_local: Option<Arc<LfsStore>>,

    // Local non-lfs cache aka shared store
    pub(crate) indexedlog_cache: Option<Arc<IndexedLogHgIdDataStore>>,

    // Local LFS cache aka shared store
    pub(crate) lfs_cache: Option<Arc<LfsStore>>,

    // Remote stores
    pub(crate) lfs_remote: Option<Arc<LfsRemote>>,
    pub(crate) edenapi: Option<Arc<SaplingRemoteApiFileStore>>,

    // Legacy ContentStore fallback
    pub(crate) contentstore: Option<Arc<ContentStore>>,

    // Aux Data Store
    pub(crate) aux_cache: Option<Arc<AuxStore>>,

    pub(crate) cas_client: Option<Arc<dyn CasClient>>,

    // Metrics, statistics, debugging
    pub(crate) activity_logger: Option<Arc<Mutex<ActivityLogger>>>,
    pub(crate) metrics: Arc<RwLock<FileStoreMetrics>>,

    pub(crate) lfs_progress: Arc<AggregatingProgressBar>,

    // Don't flush on drop when we're using FileStore in a "disposable" context, like backingstore
    pub flush_on_drop: bool,
}

impl Drop for FileStore {
    fn drop(&mut self) {
        if self.flush_on_drop {
            let _ = self.flush();
        }
    }
}

macro_rules! try_local_content {
    ($id:ident, $e:expr) => {
        if let Some(store) = $e.as_ref() {
            if let Some(data) = store.get_local_content_direct($id)? {
                return Ok(Some(data));
            }
        }
    };
}

impl FileStore {
    /// Get the "local content" without going through the heavyweight "fetch" API.
    pub(crate) fn get_local_content_direct(&self, id: &HgId) -> Result<Option<Bytes>> {
        try_local_content!(id, self.indexedlog_cache);
        try_local_content!(id, self.indexedlog_local);
        try_local_content!(id, self.lfs_cache);
        try_local_content!(id, self.lfs_local);
        Ok(None)
    }

    pub fn fetch(
        &self,
        keys: impl IntoIterator<Item = Key>,
        attrs: FileAttributes,
        fetch_mode: FetchMode,
    ) -> FetchResults<StoreFile> {
        let (found_tx, found_rx) = unbounded();
        let mut state = FetchState::new(
            keys,
            attrs,
            self,
            found_tx,
            self.lfs_threshold_bytes.is_some(),
            fetch_mode,
        );

        debug!(
            ?attrs,
            ?fetch_mode,
            num_keys = state.pending_len(),
            first_keys = "fetching"
        );

        let keys_len = state.pending_len();

        let aux_cache = self.aux_cache.clone();
        let indexedlog_cache = self.indexedlog_cache.clone();
        let indexedlog_local = self.indexedlog_local.clone();
        let lfs_cache = self.lfs_cache.clone();
        let lfs_local = self.lfs_local.clone();
        let edenapi = self.edenapi.clone();
        let cas_client = self.cas_client.clone();
        let lfs_remote = self.lfs_remote.clone();
        let contentstore = self.contentstore.clone();
        let metrics = self.metrics.clone();
        let activity_logger = self.activity_logger.clone();

        let fetch_local = fetch_mode.contains(FetchMode::LOCAL);
        let fetch_remote = fetch_mode.contains(FetchMode::REMOTE);

        let process_func = move || {
            let start_instant = Instant::now();

            // Only copy keys for activity logger if we have an activity logger;
            let activity_logger_keys: Vec<Key> = if activity_logger.is_some() {
                state.all_keys()
            } else {
                Vec::new()
            };

            let span = tracing::span!(
                tracing::Level::DEBUG,
                "file fetch",
                id = rand::thread_rng().gen::<u16>()
            );
            let _enter = span.enter();

            if fetch_local || (fetch_remote && cas_client.is_some()) {
                if let Some(ref aux_cache) = aux_cache {
                    state.fetch_aux_indexedlog(
                        aux_cache,
                        StoreLocation::Cache,
                        cas_client.is_some(),
                    );
                }
            }

            if fetch_local {
                if let Some(ref indexedlog_cache) = indexedlog_cache {
                    state.fetch_indexedlog(
                        indexedlog_cache,
                        lfs_cache.as_ref().map(|v| v.as_ref()),
                        StoreLocation::Cache,
                    );
                }

                if let Some(ref indexedlog_local) = indexedlog_local {
                    state.fetch_indexedlog(
                        indexedlog_local,
                        lfs_local.as_ref().map(|v| v.as_ref()),
                        StoreLocation::Local,
                    );
                }

                if let Some(ref lfs_cache) = lfs_cache {
                    state.fetch_lfs(lfs_cache, StoreLocation::Cache);
                }

                if let Some(ref lfs_local) = lfs_local {
                    state.fetch_lfs(lfs_local, StoreLocation::Local);
                }
            }

            if fetch_remote {
                if let Some(cas_client) = &cas_client {
                    state.fetch_cas(cas_client);
                }

                if let Some(ref edenapi) = edenapi {
                    state.fetch_edenapi(
                        edenapi,
                        indexedlog_cache.clone(),
                        lfs_cache.clone(),
                        aux_cache.clone(),
                    );
                }

                if let Some(ref lfs_remote) = lfs_remote {
                    state.fetch_lfs_remote(
                        &lfs_remote.remote,
                        lfs_local.clone(),
                        lfs_cache.clone(),
                    );
                }

                if let Some(ref contentstore) = contentstore {
                    state.fetch_contentstore(contentstore);
                }
            }

            state.derive_computable(aux_cache.as_ref().map(|s| s.as_ref()));

            metrics.write().fetch += state.metrics().clone();
            if let Err(err) = state.metrics().update_ods() {
                tracing::error!("Error updating ods fetch metrics: {}", err);
            }
            state.finish();

            if let Some(activity_logger) = activity_logger {
                if let Err(err) = activity_logger.lock().log_file_fetch(
                    activity_logger_keys,
                    attrs,
                    start_instant.elapsed(),
                ) {
                    tracing::error!("Error writing activity log: {}", err);
                }
            }
        };

        // Only kick off a thread if there's a substantial amount of work.
        if keys_len > 1000 {
            let cri = get_client_request_info_thread_local();
            std::thread::spawn(move || {
                if let Some(cri) = cri {
                    set_client_request_info_thread_local(cri);
                }
                process_func();
            });
        } else {
            process_func();
        }

        FetchResults::new(Box::new(found_rx.into_iter()))
    }

    fn write_lfsptr(&self, key: Key, bytes: Bytes) -> Result<()> {
        if !self.allow_write_lfs_ptrs {
            ensure!(
                std::env::var("TESTTMP").is_ok(),
                "writing LFS pointers directly is not allowed outside of tests"
            );
        }
        let lfs_local = self.lfs_local.as_ref().ok_or_else(|| {
            anyhow!("trying to write LFS pointer but no local LfsStore is available")
        })?;

        let lfs_pointer = LfsPointersEntry::from_bytes(bytes, key.hgid)?;
        lfs_local.add_pointer(lfs_pointer)
    }

    fn write_lfs(&self, key: Key, bytes: Bytes) -> Result<()> {
        let lfs_local = self.lfs_local.as_ref().ok_or_else(|| {
            anyhow!("trying to write LFS file but no local LfsStore is available")
        })?;
        let (lfs_pointer, lfs_blob) = lfs_from_hg_file_blob(key.hgid, &bytes)?;
        let sha256 = lfs_pointer.sha256();

        lfs_local.add_blob(&sha256, lfs_blob)?;
        lfs_local.add_pointer(lfs_pointer)?;

        Ok(())
    }

    fn write_nonlfs(&self, key: Key, bytes: Bytes, meta: Metadata) -> Result<()> {
        let indexedlog_local = self.indexedlog_local.as_ref().ok_or_else(|| {
            anyhow!("trying to write non-LFS file but no local non-LFS IndexedLog is available")
        })?;
        indexedlog_local.put_entry(Entry::new(key, bytes, meta))?;

        Ok(())
    }

    pub fn write_batch(&self, entries: impl Iterator<Item = (Key, Bytes, Metadata)>) -> Result<()> {
        // TODO(meyer): Don't fail the whole batch for a single write error.
        let mut metrics = FileStoreWriteMetrics::default();
        for (key, bytes, meta) in entries {
            if meta.is_lfs() && self.lfs_threshold_bytes.is_some() {
                metrics.lfsptr.item(1);
                if let Err(e) = self.write_lfsptr(key, bytes) {
                    metrics.lfsptr.err(1);
                    return Err(e);
                }
                metrics.lfsptr.ok(1);
                continue;
            }
            let hg_blob_len = bytes.len() as u64;
            // Default to non-LFS if no LFS threshold is set
            if self
                .lfs_threshold_bytes
                .map_or(false, |threshold| hg_blob_len > threshold)
            {
                metrics.lfs.item(1);
                if let Err(e) = self.write_lfs(key, bytes) {
                    metrics.lfs.err(1);
                    return Err(e);
                }
                metrics.lfs.ok(1);
            } else {
                metrics.nonlfs.item(1);
                if let Err(e) = self.write_nonlfs(key, bytes, meta) {
                    metrics.nonlfs.err(1);
                    return Err(e);
                }
                metrics.nonlfs.ok(1);
            }
        }
        self.metrics.write().write += metrics;
        Ok(())
    }

    #[allow(unused_must_use)]
    pub fn flush(&self) -> Result<()> {
        let mut result = Ok(());
        let mut handle_error = |error| {
            tracing::error!(%error);
            result = Err(error);
        };

        if let Some(ref indexedlog_local) = self.indexedlog_local {
            indexedlog_local.flush_log().map_err(&mut handle_error);
        }

        if let Some(ref indexedlog_cache) = self.indexedlog_cache {
            indexedlog_cache.flush_log().map_err(&mut handle_error);
        }

        if let Some(ref lfs_local) = self.lfs_local {
            lfs_local.flush().map_err(&mut handle_error);
        }

        if let Some(ref lfs_cache) = self.lfs_cache {
            lfs_cache.flush().map_err(&mut handle_error);
        }

        if let Some(ref aux_cache) = self.aux_cache {
            aux_cache.flush().map_err(&mut handle_error);
        }

        let mut metrics = self.metrics.write();
        for (k, v) in metrics.metrics() {
            hg_metrics::increment_counter(k, v as u64);
        }
        *metrics = Default::default();

        result
    }

    pub fn refresh(&self) -> Result<()> {
        self.metrics.write().api.hg_refresh.call(0);
        if let Some(contentstore) = self.contentstore.as_ref() {
            contentstore.refresh()?;
        }
        self.flush()
    }

    pub fn metrics(&self) -> Vec<(String, usize)> {
        self.metrics.read().metrics().collect()
    }

    pub fn empty() -> Self {
        FileStore {
            extstored_policy: ExtStoredPolicy::Ignore,
            lfs_threshold_bytes: None,
            edenapi_retries: 0,
            allow_write_lfs_ptrs: false,

            prefetch_aux_data: false,
            compute_aux_data: false,
            max_prefetch_size: 0,

            indexedlog_local: None,
            lfs_local: None,

            indexedlog_cache: None,
            lfs_cache: None,

            edenapi: None,
            lfs_remote: None,
            cas_client: None,

            contentstore: None,
            metrics: FileStoreMetrics::new(),
            activity_logger: None,

            aux_cache: None,

            lfs_progress: AggregatingProgressBar::new("fetching", "LFS"),
            flush_on_drop: true,
        }
    }

    pub fn indexedlog_local(&self) -> Option<Arc<IndexedLogHgIdDataStore>> {
        self.indexedlog_local.clone()
    }

    pub fn indexedlog_cache(&self) -> Option<Arc<IndexedLogHgIdDataStore>> {
        self.indexedlog_cache.clone()
    }

    pub fn with_content_store(&self, cs: Arc<ContentStore>) -> Self {
        let mut clone = self.clone();
        clone.contentstore = Some(cs);
        clone
    }

    /// Returns only the local cache / shared stores, in place of the local-only stores,
    /// such that writes will go directly to the local cache.
    pub fn with_shared_only(&self) -> Self {
        // this is infallible in ContentStore so panic if there are no shared/cache stores.
        assert!(
            self.indexedlog_cache.is_some() || self.lfs_cache.is_some(),
            "cannot get shared_mutable, no shared / local cache stores available"
        );

        Self {
            extstored_policy: self.extstored_policy.clone(),
            lfs_threshold_bytes: self.lfs_threshold_bytes.clone(),
            edenapi_retries: self.edenapi_retries.clone(),
            allow_write_lfs_ptrs: self.allow_write_lfs_ptrs,

            prefetch_aux_data: self.prefetch_aux_data,
            compute_aux_data: self.compute_aux_data,
            max_prefetch_size: self.max_prefetch_size,

            indexedlog_local: self.indexedlog_cache.clone(),
            lfs_local: self.lfs_cache.clone(),

            indexedlog_cache: None,
            lfs_cache: None,

            edenapi: None,
            lfs_remote: None,
            cas_client: None,

            contentstore: None,
            metrics: self.metrics.clone(),
            activity_logger: self.activity_logger.clone(),

            aux_cache: None,

            lfs_progress: self.lfs_progress.clone(),

            // Conservatively flushing on drop here, didn't see perf problems and might be needed by Python
            flush_on_drop: true,
        }
    }
}

impl FileStore {
    pub(crate) fn get_file_content_impl(
        &self,
        key: &Key,
        fetch_mode: FetchMode,
    ) -> Result<Option<Bytes>> {
        self.metrics.write().api.hg_getfilecontent.call(0);
        self.fetch(
            std::iter::once(key.clone()),
            FileAttributes::CONTENT,
            fetch_mode,
        )
        .single()?
        .map(|entry| entry.content.unwrap().file_content())
        .transpose()
    }
}

impl LegacyStore for FileStore {
    /// For compatibility with ContentStore::get_shared_mutable
    fn get_shared_mutable(&self) -> Arc<dyn HgIdMutableDeltaStore> {
        Arc::new(self.with_shared_only())
    }

    fn get_file_content(&self, key: &Key) -> Result<Option<Bytes>> {
        self.get_file_content_impl(key, FetchMode::AllowRemote)
    }

    // If ContentStore is available, these call into ContentStore. Otherwise, implement these
    // methods on top of scmstore (though they should still eventaully be removed).
    fn add_pending(
        &self,
        key: &Key,
        data: Bytes,
        meta: Metadata,
        location: RepackLocation,
    ) -> Result<()> {
        self.metrics.write().api.hg_addpending.call(0);
        if let Some(contentstore) = self.contentstore.as_ref() {
            contentstore.add_pending(key, data, meta, location)
        } else {
            let delta = Delta {
                data,
                base: None,
                key: key.clone(),
            };

            match location {
                RepackLocation::Local => self.add(&delta, &meta),
                RepackLocation::Shared => self.get_shared_mutable().add(&delta, &meta),
            }
        }
    }

    fn commit_pending(&self, location: RepackLocation) -> Result<Option<Vec<PathBuf>>> {
        self.metrics.write().api.hg_commitpending.call(0);
        if let Some(contentstore) = self.contentstore.as_ref() {
            contentstore.commit_pending(location)
        } else {
            self.flush()?;
            Ok(None)
        }
    }
}

impl HgIdDataStore for FileStore {
    // Fetch the raw content of a single TreeManifest blob
    fn get(&self, key: StoreKey) -> Result<StoreResult<Vec<u8>>> {
        self.metrics.write().api.hg_get.call(0);
        Ok(
            match self
                .fetch(
                    std::iter::once(key.clone()).filter_map(|sk| sk.maybe_into_key()),
                    FileAttributes::CONTENT,
                    FetchMode::AllowRemote,
                )
                .single()?
            {
                Some(entry) => StoreResult::Found(entry.content.unwrap().hg_content()?.into_vec()),
                None => StoreResult::NotFound(key),
            },
        )
    }

    fn get_meta(&self, key: StoreKey) -> Result<StoreResult<Metadata>> {
        self.metrics.write().api.hg_getmeta.call(0);
        Ok(
            match self
                .fetch(
                    std::iter::once(key.clone()).filter_map(|sk| sk.maybe_into_key()),
                    FileAttributes::CONTENT,
                    FetchMode::AllowRemote,
                )
                .single()?
            {
                Some(entry) => StoreResult::Found(entry.content.unwrap().metadata()?),
                None => StoreResult::NotFound(key),
            },
        )
    }

    fn refresh(&self) -> Result<()> {
        self.refresh()
    }
}

impl FileStore {
    pub fn prefetch(&self, keys: Vec<Key>) -> Result<Vec<Key>> {
        self.metrics.write().api.hg_prefetch.call(keys.len());

        let mut attrs = FileAttributes::CONTENT;
        if self.prefetch_aux_data {
            attrs |= FileAttributes::AUX;
        }

        let mut missing = Vec::new();

        let max_size = match self.max_prefetch_size {
            0 => keys.len(),
            max => max,
        };

        for chunk in &keys.into_iter().chunks(max_size) {
            missing.extend_from_slice(
                &self
                    .fetch(
                        chunk,
                        attrs,
                        FetchMode::AllowRemote | FetchMode::IGNORE_RESULT,
                    )
                    .missing()?,
            );
        }

        Ok(missing)
    }
}

impl RemoteDataStore for FileStore {
    fn prefetch(&self, keys: &[StoreKey]) -> Result<Vec<StoreKey>> {
        let missing = self
            .prefetch(
                keys.iter()
                    .cloned()
                    .filter_map(|sk| sk.maybe_into_key())
                    .collect(),
            )?
            .into_iter()
            .map(StoreKey::HgId)
            .collect();
        Ok(missing)
    }

    fn upload(&self, keys: &[StoreKey]) -> Result<Vec<StoreKey>> {
        self.metrics.write().api.hg_upload.call(keys.len());
        // TODO(meyer): Eliminate usage of legacy API, or at least minimize it (do we really need multiplex, etc)
        if let Some(ref lfs_remote) = self.lfs_remote {
            let mut multiplex = MultiplexDeltaStore::new();
            multiplex.add_store(self.get_shared_mutable());
            lfs_remote
                .clone()
                .datastore(Arc::new(multiplex))
                .upload(keys)
        } else {
            Ok(keys.to_vec())
        }
    }
}

impl LocalStore for FileStore {
    fn get_missing(&self, keys: &[StoreKey]) -> Result<Vec<StoreKey>> {
        self.metrics.write().api.hg_getmissing.call(keys.len());
        Ok(self
            .fetch(
                keys.iter().cloned().filter_map(|sk| sk.maybe_into_key()),
                FileAttributes::CONTENT,
                FetchMode::LocalOnly | FetchMode::IGNORE_RESULT,
            )
            .missing()?
            .into_iter()
            .map(StoreKey::HgId)
            .collect())
    }
}

impl HgIdMutableDeltaStore for FileStore {
    fn add(&self, delta: &Delta, metadata: &Metadata) -> Result<()> {
        self.metrics.write().api.hg_add.call(0);
        if let Delta {
            data,
            base: None,
            key,
        } = delta.clone()
        {
            self.write_batch(std::iter::once((key, data, metadata.clone())))
        } else {
            bail!("Deltas with non-None base are not supported")
        }
    }

    fn flush(&self) -> Result<Option<Vec<PathBuf>>> {
        self.metrics.write().api.hg_flush.call(0);
        self.flush()?;
        Ok(None)
    }
}

// TODO(meyer): Content addressing not supported at all for trees. I could look for HgIds present here and fetch with
// that if available, but I feel like there's probably something wrong if this is called for trees.
impl ContentDataStore for FileStore {
    fn blob(&self, key: StoreKey) -> Result<StoreResult<Bytes>> {
        self.metrics.write().api.contentdatastore_blob.call(0);
        Ok(
            match self
                .fetch(
                    std::iter::once(key.clone()).filter_map(|sk| sk.maybe_into_key()),
                    FileAttributes::CONTENT,
                    FetchMode::LocalOnly,
                )
                .single()?
            {
                Some(entry) => StoreResult::Found(entry.content.unwrap().file_content()?),
                None => StoreResult::NotFound(key),
            },
        )
    }

    fn metadata(&self, key: StoreKey) -> Result<StoreResult<ContentMetadata>> {
        self.metrics.write().api.contentdatastore_metadata.call(0);
        Ok(
            match self
                .fetch(
                    std::iter::once(key.clone()).filter_map(|sk| sk.maybe_into_key()),
                    FileAttributes::CONTENT,
                    FetchMode::LocalOnly,
                )
                .single()?
            {
                Some(StoreFile {
                    content: Some(LazyFile::Lfs(_blob, pointer)),
                    ..
                }) => StoreResult::Found(pointer.into()),
                Some(_) => StoreResult::NotFound(key),
                None => StoreResult::NotFound(key),
            },
        )
    }
}
