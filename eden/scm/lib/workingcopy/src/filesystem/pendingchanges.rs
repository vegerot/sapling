/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use configmodel::Config;
use io::IO;
use pathmatcher::Matcher;
use serde::Serialize;
use types::RepoPathBuf;

#[derive(Debug, Serialize)]
pub enum ChangeType {
    Changed(RepoPathBuf),
    Deleted(RepoPathBuf),
}

impl ChangeType {
    pub fn get_path(&self) -> &RepoPathBuf {
        match self {
            ChangeType::Changed(path) => path,
            ChangeType::Deleted(path) => path,
        }
    }
}

#[derive(Serialize)]
pub enum PendingChangeResult {
    File(ChangeType),
    SeenDirectory(RepoPathBuf),
}

pub trait PendingChanges {
    fn pending_changes(
        &self,
        // The full matcher including user specified filters.
        matcher: Arc<dyn Matcher + Send + Sync + 'static>,
        // Git ignore matcher, except won't match committed files.
        ignore_matcher: Arc<dyn Matcher + Send + Sync + 'static>,
        last_write: SystemTime,
        config: &dyn Config,
        io: &IO,
    ) -> Result<Box<dyn Iterator<Item = Result<PendingChangeResult>>>>;
}
