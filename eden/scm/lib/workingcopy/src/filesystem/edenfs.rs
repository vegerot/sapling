/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::Cell;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use configmodel::Config;
use configmodel::ConfigExt;
use context::CoreContext;
use edenfs_client::EdenFsClient;
use edenfs_client::FileStatus;
use manifest_tree::TreeManifest;
use parking_lot::Mutex;
use pathmatcher::DynMatcher;
use storemodel::FileStore;
use treestate::treestate::TreeState;
use types::hgid::NULL_ID;
use types::HgId;
use vfs::VFS;

use crate::client::WorkingCopyClient;
use crate::filesystem::FileSystem;
use crate::filesystem::PendingChange;
use crate::util::added_files;

pub struct EdenFileSystem {
    treestate: Arc<Mutex<TreeState>>,
    client: Arc<EdenFsClient>,
    vfs: VFS,
    store: Arc<dyn FileStore>,

    // For wait_for_potential_change
    journal_position: Cell<(i64, i64)>,
}

impl EdenFileSystem {
    pub fn new(
        client: Arc<EdenFsClient>,
        vfs: VFS,
        dot_dir: &Path,
        store: Arc<dyn FileStore>,
    ) -> Result<Self> {
        let journal_position = Cell::new(client.get_journal_position()?);
        let treestate = create_treestate(dot_dir, vfs.case_sensitive())?;
        let treestate = Arc::new(Mutex::new(treestate));
        Ok(EdenFileSystem {
            treestate,
            client,
            vfs,
            store,
            journal_position,
        })
    }
}

fn create_treestate(dot_dir: &std::path::Path, case_sensitive: bool) -> Result<TreeState> {
    let dirstate_path = dot_dir.join("dirstate");
    tracing::trace!("loading edenfs dirstate");
    TreeState::from_overlay_dirstate(&dirstate_path, case_sensitive)
}

impl FileSystem for EdenFileSystem {
    #[tracing::instrument(skip_all)]
    fn pending_changes(
        &self,
        _ctx: &CoreContext,
        matcher: DynMatcher,
        ignore_matcher: DynMatcher,
        _ignore_dirs: Vec<PathBuf>,
        include_ignored: bool,
    ) -> Result<Box<dyn Iterator<Item = Result<PendingChange>>>> {
        let p1 = self
            .treestate
            .lock()
            .parents()
            .next()
            .unwrap_or_else(|| Ok(NULL_ID))?;

        let status_map = self.client.get_status(p1, include_ignored)?;

        // In rare cases, a file can transition in the dirstate directly from "normal" to
        // "added". Eden won't report a pending change if the file is not modified (since
        // it looks like an unmodified file until dirstate p1 is updated). So, here we
        // look for added files that aren't in the results from Eden. If the files exist
        // on disk, we inject a pending change. Otherwise, later logic in status infers
        // that the added file must have been removed from disk because the file isn't in
        // the pending changes.
        let extra_added_files = added_files(&mut self.treestate.lock())?
            .into_iter()
            .filter_map(|path| {
                if status_map.contains_key(&path) {
                    None
                } else {
                    match self.vfs.exists(&path) {
                        Ok(true) => Some(Ok(PendingChange::Changed(path))),
                        Ok(false) => None,
                        Err(err) => Some(Err(err)),
                    }
                }
            })
            .collect::<Vec<_>>();

        Ok(Box::new(status_map.into_iter().filter_map(
            move |(path, status)| {
                tracing::trace!(target: "workingcopy::filesystem::edenfs::status", %path, ?status, "eden status");
                // EdenFS reports files that are present in the overlay but filtered from the repo
                // as untracked. We "drop" any files that are excluded by the current filter.
                let mut matched = false;
                let result = match matcher.matches_file(&path) {
                    Ok(true) => {
                        matched = true;
                        match &status {
                            FileStatus::Removed => Some(Ok(PendingChange::Deleted(path))),
                            FileStatus::Ignored => Some(Ok(PendingChange::Ignored(path))),
                            FileStatus::Added => {
                                // EdenFS doesn't know about global ignore files in ui.ignore.* config, so we need to run
                                // untracked files through our ignore matcher.
                                match ignore_matcher.matches_file(&path) {
                                    Ok(ignored) => {
                                        if ignored {
                                            if include_ignored {
                                                Some(Ok(PendingChange::Ignored(path)))
                                            } else {
                                                None
                                            }
                                        } else {
                                            Some(Ok(PendingChange::Changed(path)))
                                        }
                                    }
                                    Err(err) => Some(Err(err)),
                                }
                            },
                            FileStatus::Modified => Some(Ok(PendingChange::Changed(path))),
                        }
                    },
                    Ok(false) => None,
                    Err(e) => {
                        tracing::warn!(
                            "failed to determine if {} is ignored or not tracked by the active filter: {:?}",
                            &path,
                            e
                        );
                        Some(Err(e))
                    }
                };

                if tracing::enabled!(tracing::Level::TRACE) {
                    if let Some(result) = &result {
                        let result = result.as_ref().ok();
                        tracing::trace!(%matched, ?result, " processed eden status");
                    }
                }

                result
            },
        ).chain(extra_added_files.into_iter())))
    }

    fn wait_for_potential_change(&self, config: &dyn Config) -> Result<()> {
        let interval_ms = config
            .get_or("workingcopy", "poll-interval-ms-edenfs", || 200)?
            .max(50);
        loop {
            let new_journal_position = self.client.get_journal_position()?;
            let old_journal_position = self.journal_position.get();
            if old_journal_position != new_journal_position {
                tracing::trace!(
                    "edenfs journal position changed: {:?} -> {:?}",
                    old_journal_position,
                    new_journal_position
                );
                self.journal_position.set(new_journal_position);
                break;
            }
            std::thread::sleep(Duration::from_millis(interval_ms));
        }
        Ok(())
    }

    fn sparse_matcher(
        &self,
        manifests: &[Arc<TreeManifest>],
        dot_dir: &'static str,
    ) -> Result<Option<DynMatcher>> {
        crate::sparse::sparse_matcher(
            &self.vfs,
            manifests,
            self.store.clone(),
            &self.vfs.root().join(dot_dir),
        )
    }

    fn set_parents(
        &self,
        p1: HgId,
        p2: Option<HgId>,
        parent_tree_hash: Option<HgId>,
    ) -> Result<()> {
        let parent_tree_hash =
            parent_tree_hash.context("parent tree required for setting EdenFS parents")?;
        self.client.set_parents(p1, p2, parent_tree_hash)
    }

    fn get_treestate(&self) -> Result<Arc<Mutex<TreeState>>> {
        Ok(self.treestate.clone())
    }

    fn get_client(&self) -> Option<Arc<dyn WorkingCopyClient>> {
        Some(self.client.clone())
    }
}
