/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt::Debug;

use mononoke_types::hash::Blake3;
use mononoke_types::hash::RichGitSha1;
use mononoke_types::hash::Sha1;
use mononoke_types::hash::Sha256;
use mononoke_types::ContentId;
use thiserror::Error;

use crate::expected_size::ExpectedSize;
use crate::FetchKey;

#[derive(Debug)]
pub struct InvalidHash<T: Debug> {
    #[allow(dead_code)]
    pub expected: T,
    #[allow(dead_code)]
    pub effective: T,
}

#[derive(Debug, Error)]
pub enum ErrorKind {
    #[error("Invalid size: {0:?} was expected, {1:?} was observed")]
    InvalidSize(ExpectedSize, u64),

    #[error("Invalid ContentId: {0:?}")]
    InvalidContentId(InvalidHash<ContentId>),

    #[error("Invalid Sha1: {0:?}")]
    InvalidSha1(InvalidHash<Sha1>),

    #[error("Invalid Sha256: {0:?}")]
    InvalidSha256(InvalidHash<Sha256>),

    #[error("Invalid RichGitSha1: {0:?}")]
    InvalidGitSha1(InvalidHash<RichGitSha1>),

    #[error("Invalid Blake3: {0:?}")]
    InvalidBlake3(InvalidHash<Blake3>),

    #[error("Missing content: {0:?}")]
    MissingContent(FetchKey),
}
