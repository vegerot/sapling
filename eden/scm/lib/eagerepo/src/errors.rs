/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dag::Vertex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Dag(#[from] dag::Error),

    #[error("hash mismatch ({0:?} != {1:?})")]
    HashMismatch(Vertex, Vertex),

    #[error(
        "EagerRepo detected unsupported requires at {0}:\n  Unsupported: {1:?}\n  Missing: {2:?}\n(is there a non-EagerRepo created accidentally at the same location?)"
    )]
    RequirementsMismatch(String, Vec<String>, Vec<String>),

    #[error(
        "when adding commit {0:?} with root tree {1:?}, referenced paths {2:?} are not present"
    )]
    CommitMissingPaths(Vertex, Vertex, Vec<String>),

    #[error("when moving bookmark {0:?} to {1:?}, the commit does not exist")]
    BookmarkMissingCommit(String, Vertex),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Dag(dag::errors::BackendError::from(err).into())
    }
}

impl From<zstore::Error> for Error {
    fn from(err: zstore::Error) -> Self {
        anyhow::Error::from(err).into()
    }
}

impl From<metalog::Error> for Error {
    fn from(err: metalog::Error) -> Self {
        anyhow::Error::from(err).into()
    }
}
