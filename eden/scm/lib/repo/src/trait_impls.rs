/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::path::Path;
use std::sync::Arc;

use edenapi::SaplingRemoteApi;
use metalog::MetaLog;
use parking_lot::RwLock;
use storemodel::StoreInfo;

use crate::repo::Repo;

impl StoreInfo for Repo {
    fn has_requirement(&self, requirement: &str) -> bool {
        // For storage we only check store_requirements.
        // "remotefilelog" should be but predates store requirements.
        self.store_requirements.contains(requirement)
            || (requirement == "remotefilelog" && self.requirements.contains(requirement))
    }

    fn config(&self) -> &dyn configmodel::Config {
        Repo::config(self)
    }

    fn store_path(&self) -> &Path {
        &self.store_path
    }

    fn remote_peer(&self) -> anyhow::Result<Option<Arc<dyn SaplingRemoteApi>>> {
        Ok(self.optional_eden_api()?)
    }

    fn metalog(&self) -> anyhow::Result<Arc<RwLock<MetaLog>>> {
        Repo::metalog(self)
    }
}
