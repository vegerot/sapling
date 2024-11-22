/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Implement traits from other crates.

use async_trait::async_trait;
use storemodel::BoxIterator;
use storemodel::FileStore;
use storemodel::InsertOpts;
use storemodel::KeyStore;
use storemodel::Kind;
use storemodel::SerializationFormat;
use storemodel::TreeStore;
use types::fetch_mode::FetchMode;
use types::HgId;
use types::RepoPath;

use crate::GitStore;

#[async_trait]
impl KeyStore for GitStore {
    fn get_local_content(
        &self,
        _path: &RepoPath,
        id: HgId,
    ) -> anyhow::Result<Option<minibytes::Bytes>> {
        match self.read_obj(id, git2::ObjectType::Any, FetchMode::LocalOnly) {
            Ok(data) => Ok(Some(data.into())),
            Err(e) => {
                if let Some(e) = e.downcast_ref::<git2::Error>() {
                    if e.code() == git2::ErrorCode::NotFound {
                        return Ok(None);
                    }
                }
                Err(e.into())
            }
        }
    }

    fn get_content_iter(
        &self,
        keys: Vec<types::Key>,
        fetch_mode: FetchMode,
    ) -> anyhow::Result<BoxIterator<anyhow::Result<(types::Key, minibytes::Bytes)>>> {
        if self.has_fetch_url() && fetch_mode.contains(FetchMode::REMOTE) {
            let ids = keys.iter().map(|k| k.hgid).collect::<Vec<_>>();
            self.fetch_objs(&ids)?
        }
        if fetch_mode.contains(FetchMode::IGNORE_RESULT) {
            return Ok(Box::new(std::iter::empty()));
        }
        let store = self.clone();
        let iter = keys.into_iter().map(move |k| {
            let data = store.read_obj(k.hgid, git2::ObjectType::Any, FetchMode::AllowRemote)?;
            Ok((k, data.into()))
        });
        Ok(Box::new(iter))
    }

    // This is an old API but still critical for BFS tree fetching.
    fn prefetch(&self, keys: Vec<types::Key>) -> anyhow::Result<()> {
        if self.has_fetch_url() {
            let ids = keys.iter().map(|k| k.hgid).collect::<Vec<_>>();
            self.fetch_objs(&ids)?;
        }
        Ok(())
    }

    fn insert_data(&self, opts: InsertOpts, path: &RepoPath, data: &[u8]) -> anyhow::Result<HgId> {
        let kind = match opts.kind {
            Kind::File => git2::ObjectType::Blob,
            Kind::Tree => git2::ObjectType::Tree,
        };
        let id = self.write_obj(kind, data)?;
        if let Some(forced_id) = opts.forced_id {
            if forced_id.as_ref() != &id {
                anyhow::bail!("hash mismatch when writing {}@{}", path, forced_id);
            }
        }
        Ok(id)
    }

    fn format(&self) -> SerializationFormat {
        SerializationFormat::Git
    }

    fn refresh(&self) -> anyhow::Result<()> {
        // We don't hold state in memory, so no need to refresh.
        Ok(())
    }

    fn flush(&self) -> anyhow::Result<()> {
        // We don't hold pending state in memory, so no need to flush.
        Ok(())
    }

    fn clone_key_store(&self) -> Box<dyn KeyStore> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl FileStore for GitStore {
    fn clone_file_store(&self) -> Box<dyn FileStore> {
        Box::new(self.clone())
    }
}

impl TreeStore for GitStore {
    fn clone_tree_store(&self) -> Box<dyn TreeStore> {
        Box::new(self.clone())
    }
}
