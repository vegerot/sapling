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
use revisionstore::HgIdDataStore;
use revisionstore::RemoteDataStore;
use revisionstore::StoreKey;
use revisionstore::StoreResult;
use storemodel::minibytes::Bytes;
use storemodel::FileStore;
use storemodel::KeyStore;
use storemodel::TreeStore;
use types::Key;
use types::RepoPath;

use crate::filescmstore;
use crate::treescmstore;
use crate::PythonHgIdDataStore;

pub(crate) fn register(py: Python) {
    register_into(py, |py, t: treescmstore| t.to_dyn_treestore(py));
    register_into(py, py_to_dyn_treestore);

    register_into(py, |py, f: filescmstore| f.to_read_file_contents(py));
}

impl filescmstore {
    fn to_read_file_contents(&self, py: Python) -> Arc<dyn FileStore> {
        let store = self.extract_inner(py);
        let store = ArcFileStore(store);
        Arc::new(store)
    }
}

impl treescmstore {
    fn to_dyn_treestore(&self, py: Python) -> Arc<dyn TreeStore> {
        match &self.caching_store(py) {
            Some(caching_store) => caching_store.clone(),
            None => self.store(py).clone(),
        }
    }
}

// Legacy support for store in Python.
// Used at least by unioncontentstore.
fn py_to_dyn_treestore(_py: Python, obj: PyObject) -> Arc<dyn TreeStore> {
    Arc::new(ManifestStore::new(PythonHgIdDataStore::new(obj)))
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
