/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! # Communicating to EdenFS via Thrift

mod client;
mod filter;
mod types;

pub use crate::client::EdenFsClient;
pub use crate::types::CheckoutConflict;
pub use crate::types::CheckoutMode;
pub use crate::types::ConflictType;
pub use crate::types::EdenError;
pub use crate::types::FileStatus;
