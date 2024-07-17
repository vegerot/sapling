/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Support `ImplInto` from cpython-ext.

use std::sync::Arc;

use anyhow::Result;
use cpython::*;
use cpython_ext::convert::register_into;
use cpython_ext::ExtractInner;
use revisionstore::trait_impls::ArcFileStore;
use revisionstore::trait_impls::ArcRemoteDataStore;
use revisionstore::HgIdDataStore;
use revisionstore::LegacyStore;
use revisionstore::RemoteDataStore;
use revisionstore::StoreKey;
use revisionstore::StoreResult;
use storemodel::minibytes::Bytes;
use storemodel::FileStore;
use storemodel::KeyStore;
use storemodel::TreeStore;
use types::Key;
use types::RepoPath;

use crate::contentstore;
use crate::filescmstore;
use crate::pyfilescmstore;
use crate::treescmstore;
use crate::PythonHgIdDataStore;

pub(crate) fn register(py: Python) {
    register_into(py, |py, c: contentstore| c.to_dyn_treestore(py));
    register_into(py, |py, t: treescmstore| t.to_dyn_treestore(py));
    register_into(py, py_to_dyn_treestore);

    register_into(py, |py, c: contentstore| c.to_read_file_contents(py));
    register_into(py, |py, f: filescmstore| f.to_read_file_contents(py));
    register_into(py, |py, p: pyfilescmstore| p.to_read_file_contents(py));
}

impl contentstore {
    fn to_dyn_treestore(&self, py: Python) -> Arc<dyn TreeStore> {
        let store = self.extract_inner(py) as Arc<dyn LegacyStore>;
        Arc::new(ManifestStore::new(store))
    }

    fn to_read_file_contents(&self, py: Python) -> Arc<dyn FileStore> {
        let store = self.extract_inner(py) as Arc<dyn LegacyStore>;
        let store = ArcRemoteDataStore(store as Arc<_>);
        Arc::new(store)
    }
}

impl filescmstore {
    fn to_read_file_contents(&self, py: Python) -> Arc<dyn FileStore> {
        let store = self.extract_inner(py);
        let store = ArcFileStore(store);
        Arc::new(store)
    }
}

impl pyfilescmstore {
    fn to_read_file_contents(&self, py: Python) -> Arc<dyn FileStore> {
        self.extract_inner(py)
    }
}

impl treescmstore {
    fn to_dyn_treestore(&self, py: Python) -> Arc<dyn TreeStore> {
        let store = self.extract_inner(py) as Arc<dyn LegacyStore>;
        Arc::new(ManifestStore::new(store))
    }
}

// Legacy support for store in Python.
// XXX: Check if it's used and drop support for it.
fn py_to_dyn_treestore(_py: Python, obj: PyObject) -> Arc<dyn TreeStore> {
    let store = Arc::new(PythonHgIdDataStore::new(obj)) as Arc<dyn LegacyStore>;
    Arc::new(ManifestStore::new(store))
}

struct ManifestStore<T> {
    underlying: T,
}

impl<T> ManifestStore<T> {
    pub fn new(underlying: T) -> Self {
        ManifestStore { underlying }
    }
}

impl<T: HgIdDataStore + RemoteDataStore> KeyStore for ManifestStore<T> {
    fn get_local_content(
        &self,
        path: &RepoPath,
        node: types::HgId,
    ) -> anyhow::Result<Option<Bytes>> {
        if node.is_null() {
            return Ok(Some(Default::default()));
        }
        let key = Key::new(path.to_owned(), node);
        match self.underlying.get(StoreKey::hgid(key))? {
            StoreResult::NotFound(_key) => Ok(None),
            StoreResult::Found(data) => Ok(Some(data.into())),
        }
    }

    fn prefetch(&self, keys: Vec<Key>) -> Result<()> {
        let keys = keys
            .iter()
            .filter_map(|k| {
                if k.hgid.is_null() {
                    None
                } else {
                    Some(StoreKey::from(k))
                }
            })
            .collect::<Vec<_>>();
        self.underlying.prefetch(&keys).map(|_| ())
    }
}

impl<T: HgIdDataStore + RemoteDataStore> TreeStore for ManifestStore<T> {}
