/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Support `ImplInto` from cpython-ext.

use std::sync::Arc;

use cpython::*;
use cpython_ext::convert::register_into;
use storemodel::FileStore;
use storemodel::TreeStore;

use crate::gitstore;

pub(crate) fn register(py: Python) {
    register_into(py, |py, g: gitstore| g.to_dyn_treestore(py));
    register_into(py, |py, g: gitstore| g.to_read_file_contents(py));
}

impl gitstore {
    fn to_dyn_treestore(&self, py: Python) -> Arc<dyn TreeStore> {
        let store = self.inner(py);
        store.clone() as Arc<_>
    }

    fn to_read_file_contents(&self, py: Python) -> Arc<dyn FileStore> {
        let store = self.inner(py).clone();
        store as Arc<_>
    }
}
