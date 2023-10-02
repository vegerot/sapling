/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use thiserror::Error;

use crate::path::NonRootMPath;

#[derive(Debug, Error)]
pub enum MononokeTypeError {
    #[error("invalid blake2 input: {0}")]
    InvalidBlake2Input(String),
    #[error("invalid sha1 input: {0}")]
    InvalidSha1Input(String),
    #[error("invalid sha256 input: {0}")]
    InvalidSha256Input(String),
    #[error("invalid git sha1 input: {0}")]
    InvalidGitSha1Input(String),
    #[error("invalid path '{0}': {1}")]
    InvalidPath(String, String),
    #[error("invalid Mononoke path '{0}': {1}")]
    InvalidMPath(NonRootMPath, String),
    #[error("error while deserializing blob for '{0}'")]
    BlobDeserializeError(String),
    #[error("error for key '{0}'")]
    BlobKeyError(String),
    #[error("invalid Thrift structure '{0}': {1}")]
    InvalidThrift(String, String),
    #[error("invalid changeset date: {0}")]
    InvalidDateTime(String),
    #[error("not path-conflict-free: changed path '{0}' is a prefix of '{1}'")]
    NotPathConflictFree(NonRootMPath, NonRootMPath),
    #[error("invalid bonsai changeset: {0}")]
    InvalidBonsaiChangeset(String),
    #[error("Failed to parse RepositoryId from '{0}'")]
    FailedToParseRepositoryId(String),
    #[error("invalid blake3 input: {0}")]
    InvalidBlake3Input(String),
    #[error("Git submodules not supported")]
    GitSubmoduleNotSupported,
}
