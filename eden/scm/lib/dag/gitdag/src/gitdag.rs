/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeMap;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;

use dag::ops::DagAlgorithm;
use dag::ops::DagPersistent;
use dag::Dag;
use dag::Group;
use dag::Set;
use dag::Vertex;
use dag::VertexListWithOptions;
use nonblocking::non_blocking_result;
use parking_lot::Mutex;
use phf::phf_set;

use crate::errors::MapDagError;

/// `GitDag` maintains segmented changelog as an index on the Git commit graph.
///
/// This struct provides a "read-only" view for the commit graph. To read other
/// parts of the git repo, or make changes to the Git commit graph, use a
/// separate `git2::Repository` object.
///
/// The `dag` part is append-only. It might include vertexes no longer referred
/// by the git repo. Use `ancestors(git_heads())` to get commits referred by
/// the git repo, and use `&` to filter them.
pub struct GitDag {
    dag: Dag,
    heads: Set,
    references: BTreeMap<String, Vertex>,
    pub opts: GitDagOptions,
}

/// Config for `GitDag`.
#[derive(Debug, Default)]
pub struct GitDagOptions {
    /// Sync all branches including refs/remotes/.
    ///
    /// This can be set to `true` for `sl clone`-ed Git repos.
    ///
    /// However, for `git clone`-ed repos there might be too many references (tags, release
    /// branches) that it is better to set this to `false`.
    ///
    /// When set to `false`:
    /// - In `refs/remotes/`, only a hardcoded list of "main" references will be imported.
    /// - Local branches will be imported. Whether they are treated as bookmarks or visibleheads
    ///   is up to the upper layer that deals with metalog.
    /// - Tags will be skipped.
    pub import_all_references: bool,
}

impl GitDag {
    /// `open` a Git repo at `git_dir`. Build index at `dag_dir`, with specified `opts`.
    pub fn open(git_dir: &Path, dag_dir: &Path, opts: GitDagOptions) -> dag::Result<Self> {
        let git_repo = git2::Repository::open(git_dir)
            .with_context(|| format!("opening git repo at {}", git_dir.display()))?;
        Self::open_git_repo(&git_repo, dag_dir, opts)
    }

    /// For an git repo, build index at `dag_dir` with specified `opts`.
    pub fn open_git_repo(
        git_repo: &git2::Repository,
        dag_dir: &Path,
        opts: GitDagOptions,
    ) -> dag::Result<Self> {
        let dag = Dag::open(dag_dir)?;
        sync_from_git(dag, git_repo, opts)
    }

    /// Get "snapshotted" references.
    pub fn git_references(&self) -> &BTreeMap<String, Vertex> {
        &self.references
    }

    /// Get "snapshotted" heads.
    pub fn git_heads(&self) -> Set {
        self.heads.clone()
    }
}

impl Deref for GitDag {
    type Target = Dag;

    fn deref(&self) -> &Self::Target {
        &self.dag
    }
}

impl DerefMut for GitDag {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.dag
    }
}

/// Main refs have 2 use-cases:
/// - Filter out "unineresting" remote branches when `import_all_references` is false.
/// - Figure out what to insert to the MASTER group in segmented changelog (for better perf).
const MAIN_REFS: phf::Set<&str> = phf_set! {
    // "origin" is the default remote name used by `git clone`.
    "refs/remotes/origin/main",
    "refs/remotes/origin/master",
    // "remote" is the default remote name used by `sl clone`.
    "refs/remotes/remote/main",
    "refs/remotes/remote/master",
};

/// Read references from git, build segments for new heads.
///
/// Useful when the git repo is changed by other processes or threads.
fn sync_from_git(
    mut dag: Dag,
    git_repo: &git2::Repository,
    opts: GitDagOptions,
) -> dag::Result<GitDag> {
    let mut master_heads = Vec::new();
    let mut non_master_heads = Vec::new();
    let mut references = BTreeMap::new();

    let git_refs = git_repo.references().context("listing git references")?;
    tracing::info!(all = opts.import_all_references, "importing git references",);
    for git_ref in git_refs {
        let git_ref = git_ref.context("resolving git reference")?;
        let name = match git_ref.name() {
            None => continue,
            Some(name) => name,
        };

        let mut is_main = false;
        if !opts.import_all_references {
            let mut should_import = false;
            if master_heads.is_empty() && name.starts_with("refs/remotes/") {
                is_main = MAIN_REFS.contains(name);
                should_import = is_main;
            } else if name.starts_with("refs/heads/") {
                should_import = true;
            } else if name.starts_with("refs/visibleheads/") {
                should_import = true;
            }
            if !should_import {
                continue;
            }
        }

        let commit = match git_ref.peel_to_commit() {
            Err(e) => {
                tracing::warn!(
                    "git ref {} cannot resolve to commit: {}",
                    String::from_utf8_lossy(git_ref.name_bytes()),
                    e
                );
                // Ignore this error. Some git references (ex. tags) can point
                // to trees instead of commits.
                continue;
            }
            Ok(c) => c,
        };
        let oid = commit.id();
        let vertex = Vertex::copy_from(oid.as_bytes());
        references.insert(name.to_string(), vertex.clone());
        if is_main {
            master_heads.push(vertex);
        } else {
            non_master_heads.push(vertex);
        }
    }

    struct ForceSend<T>(T);

    // See https://github.com/rust-lang/git2-rs/issues/194, libgit2 can be
    // accessed by a different thread.
    unsafe impl<T> Send for ForceSend<T> {}

    let git_repo = ForceSend(git_repo);
    let git_repo = Mutex::new(git_repo);

    let parent_func = move |v: Vertex| -> dag::Result<Vec<Vertex>> {
        tracing::trace!("visiting git commit {:?}", &v);
        let oid = git2::Oid::from_bytes(v.as_ref())
            .with_context(|| format!("converting to git oid for {:?}", &v))?;
        let commit = git_repo
            .lock()
            .0
            .find_commit(oid)
            .with_context(|| format!("resolving {:?} to git commit", &v))?;
        Ok(commit
            .parent_ids()
            .map(|id| Vertex::copy_from(id.as_bytes()))
            .collect())
    };
    let parents: Box<dyn Fn(Vertex) -> dag::Result<Vec<Vertex>> + Send + Sync> =
        Box::new(parent_func);
    let heads = VertexListWithOptions::from(master_heads.clone())
        .with_desired_group(Group::MASTER)
        .chain(non_master_heads.clone());
    non_blocking_result(dag.add_heads_and_flush(&parents, &heads))?;

    let possible_heads = Set::from_static_names(master_heads.into_iter().chain(non_master_heads));
    let heads = non_blocking_result(dag.heads_ancestors(possible_heads))?;

    Ok(GitDag {
        dag,
        heads,
        references,
        opts,
    })
}
