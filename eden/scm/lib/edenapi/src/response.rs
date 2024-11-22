/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use anyhow::Context;
use async_runtime::block_unless_interrupted;
use clientinfo::get_client_request_info_thread_local;
use clientinfo_async::with_client_request_info_scope;
pub use edenapi_trait::Response;
pub use edenapi_trait::ResponseMeta;
use futures::prelude::*;
use http_client::Stats;

use crate::errors::SaplingRemoteApiError;

/// Non-async version of `Response`.
pub struct BlockingResponse<T> {
    pub entries: Vec<T>,
    pub stats: Stats,
}

impl<T> BlockingResponse<T> {
    pub fn from_async<F>(fetch: F) -> Result<Self, SaplingRemoteApiError>
    where
        F: Future<Output = Result<Response<T>, SaplingRemoteApiError>>,
    {
        // Fetch ClientRequestInfo from a thread local and pass to async code
        let maybe_client_request_info = get_client_request_info_thread_local();
        let Response { entries, stats } = block_unless_interrupted(with_client_request_info_scope(
            maybe_client_request_info.clone(),
            fetch,
        ))
        .context("transfer interrupted by user")??;
        let entries = block_unless_interrupted(with_client_request_info_scope(
            maybe_client_request_info.clone(),
            entries.try_collect(),
        ))
        .context("transfer interrupted by user")??;
        let stats = block_unless_interrupted(with_client_request_info_scope(
            maybe_client_request_info,
            stats,
        ))
        .context("transfer interrupted by user")??;
        Ok(Self { entries, stats })
    }
}
