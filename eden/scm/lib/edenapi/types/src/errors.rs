/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[derive(Serialize, Deserialize)] // used to convert to Python
#[cfg_attr(
    any(test, feature = "for-tests"),
    derive(quickcheck_arbitrary_derive::Arbitrary)
)]
#[error("server error (code {code}): {message}")]
/// Common error structure between Mononoke and Mercurial.
/// The `message` field is self explanatory, a natural language description of the issue that was
/// encountered.
/// The `code` field represents a numeric identifier of the type of issue that was encountered. In
/// most situations the code will be `0`, meaning that there is nothing special about the error.
/// Non-zero codes are used for situations where the client wants to take a specific action (when
/// the client needs to handle that error).
///
/// Error code list:
/// ---------------
/// 1: SegmentedChangelogMismatchedHeads
///    Fatal inconsistency between client and server. The client will want to reclone in this
///    situation.
/// 2: HexError
///    Failed to convert hex to binary hash.
pub struct ServerError {
    pub message: String,
    pub code: u64,
}

impl ServerError {
    pub fn new<M: Into<String>>(m: M, code: u64) -> Self {
        Self {
            message: m.into(),
            code,
        }
    }

    pub fn generic<M: Into<String>>(m: M) -> Self {
        Self::new(m, 0)
    }
}

impl From<types::hash::HexError> for ServerError {
    fn from(e: types::hash::HexError) -> Self {
        Self::new(e.to_string(), 2)
    }
}
