/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Register factory constructors.

use std::path::Path;
use std::path::PathBuf;

use commits_trait::DagCommits;
use fs_err as fs;
use storemodel::StoreInfo;

use crate::git::GitSegmentedCommits;

macro_rules! concat_os_path {
    ($p1:literal, $p2:literal) => {
        // Cannot use std::path::MAIN_SEPARATOR inside concat! yet.
        if cfg!(windows) {
            concat!($p1, '\\', $p2)
        } else {
            concat!($p1, '/', $p2)
        }
    };
}

const SEGMENTS_PATH: &str = concat_os_path!("segments", "v1");
const GIT_STORE_REQUIREMENT: &str = "git-store";
const DOTGIT_REQUIREMENT: &str = "dotgit";
const GIT_FILE: &str = "gitdir";

pub(crate) fn setup_commits_constructor() {
    factory::register_constructor("10-git-commits", maybe_construct_commits);
}

fn maybe_construct_commits(
    info: &dyn StoreInfo,
) -> anyhow::Result<Option<Box<dyn DagCommits + Send + 'static>>> {
    if info.has_requirement(GIT_STORE_REQUIREMENT) {
        // If the repo is cloned by `git`, not `sl`, then all references are cloned by default,
        // which hurts perf. Do not import all references in that case.
        let is_dotgit = info.has_requirement(DOTGIT_REQUIREMENT);
        tracing::info!(target: "changelog_info", changelog_backend="git");
        let git_dag = open_git(info, is_dotgit)?;
        Ok(Some(git_dag))
    } else {
        Ok(None)
    }
}

fn open_git(
    info: &dyn StoreInfo,
    is_dotgit: bool,
) -> anyhow::Result<Box<dyn DagCommits + Send + 'static>> {
    let store_path = info.store_path();
    let metalog = info.metalog()?;
    let mut metalog = metalog.write();
    let git_path = calculate_git_path(store_path)?;
    let segments_path = calculate_segments_path(store_path);
    let config = info.config();
    let mut git_segmented_commits =
        GitSegmentedCommits::new(&git_path, &segments_path, config, is_dotgit)?;
    // Import (maybe changed) git references on construction.
    git_segmented_commits.import_from_git(&mut metalog)?;
    Ok(Box::new(git_segmented_commits))
}

fn calculate_git_path(store_path: &Path) -> Result<PathBuf, std::io::Error> {
    let git_file_contents = get_path_from_file(store_path, GIT_FILE)?;
    let git_path = PathBuf::from(&git_file_contents);
    if !git_path.is_absolute() {
        return Ok(store_path.join(git_path));
    }
    Ok(git_path)
}

fn calculate_segments_path(store_path: &Path) -> PathBuf {
    store_path.join(SEGMENTS_PATH)
}

fn get_path_from_file(store_path: &Path, target_file: &str) -> Result<PathBuf, std::io::Error> {
    let path_file = store_path.join(target_file);
    fs::read_to_string(path_file).map(PathBuf::from)
}
