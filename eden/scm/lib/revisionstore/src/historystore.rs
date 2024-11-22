/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use edenapi_types::HistoryEntry;
use types::Key;
use types::NodeInfo;

use crate::localstore::LocalStore;
use crate::types::StoreKey;

pub trait HgIdHistoryStore: LocalStore + Send + Sync {
    fn get_node_info(&self, key: &Key) -> Result<Option<NodeInfo>>;
    fn refresh(&self) -> Result<()>;

    fn get_local_node_info(&self, key: &Key) -> Result<Option<NodeInfo>> {
        if self.contains(&key.into())? {
            self.get_node_info(key)
        } else {
            Ok(None)
        }
    }
}

pub trait HgIdMutableHistoryStore: HgIdHistoryStore + Send + Sync {
    fn add(&self, key: &Key, info: &NodeInfo) -> Result<()>;
    fn flush(&self) -> Result<Option<Vec<PathBuf>>>;

    fn add_entry(&self, entry: &HistoryEntry) -> Result<()> {
        self.add(&entry.key, &entry.nodeinfo)
    }
}

pub trait HistoryStore: RemoteHistoryStore + HgIdMutableHistoryStore {
    /// Copy of self with local stores removed (i.e. cache only)..
    fn with_shared_only(&self) -> Arc<dyn HistoryStore>;
}

/// The `RemoteHistoryStore` trait indicates that data can fetched over the network. Care must be
/// taken to avoid serially fetching data and instead data should be fetched in bulk via the
/// `prefetch` API.
pub trait RemoteHistoryStore: HgIdHistoryStore + Send + Sync {
    /// Attempt to bring the data corresponding to the passed in keys to a local store.
    ///
    /// When implemented on a pure remote store, like the `SaplingRemoteApi`, the method will always fetch
    /// everything that was asked. On a higher level store, such as the `MetadataStore`, this will
    /// avoid fetching data that is already present locally.
    fn prefetch(&self, keys: &[StoreKey], length: Option<u32>) -> Result<()>;
}

/// Implement `HgIdHistoryStore` for all types that can be `Deref` into a `HgIdHistoryStore`.
impl<T: HgIdHistoryStore + ?Sized, U: Deref<Target = T> + Send + Sync> HgIdHistoryStore for U {
    fn get_node_info(&self, key: &Key) -> Result<Option<NodeInfo>> {
        T::get_node_info(self, key)
    }

    fn refresh(&self) -> Result<()> {
        T::refresh(self)
    }
}

impl<T: HgIdMutableHistoryStore + ?Sized, U: Deref<Target = T> + Send + Sync>
    HgIdMutableHistoryStore for U
{
    fn add(&self, key: &Key, info: &NodeInfo) -> Result<()> {
        T::add(self, key, info)
    }

    fn flush(&self) -> Result<Option<Vec<PathBuf>>> {
        T::flush(self)
    }
}

impl<T: RemoteHistoryStore + ?Sized, U: Deref<Target = T> + Send + Sync> RemoteHistoryStore for U {
    fn prefetch(&self, keys: &[StoreKey], length: Option<u32>) -> Result<()> {
        T::prefetch(self, keys, length)
    }
}
