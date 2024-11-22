/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! # pathhistory
//!
//! This crate provides file or directory history algorithms that does not
//! depend on per-path indexes, is better than scanning commits one by one,
//! and is friendly for lazy stores.
//!
//! The basic idea is to use the segment struct from the `dag` crate and
//! (aggressively) skip large chunks of commits without visiting all commits
//! one by one.
//!
//! This might miss some "change then revert" commits but practically it
//! might be good enough.
//!
//! See `PathHistory` for the main structure.

mod pathhistory;
mod pathops;
mod utils;

#[cfg(test)]
mod tests;

pub use crate::pathhistory::PathHistory;

#[cfg(test)]
dev_logger::init!();
