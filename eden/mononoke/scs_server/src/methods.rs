/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use async_requests::types::AsyncPingToken;
use context::CoreContext;
use source_control as thrift;

use crate::async_requests::enqueue;
use crate::async_requests::get_queue;
use crate::async_requests::poll;
use crate::errors;
use crate::source_control_impl::SourceControlServiceImpl;

pub(crate) mod cloud;
pub(crate) mod commit;
pub(crate) mod commit_lookup_pushrebase_history;
pub(crate) mod commit_path;
pub(crate) mod commit_sparse_profile_info;
pub(crate) mod create_repos;
pub(crate) mod file;
pub(crate) mod git;
pub(crate) mod megarepo;
pub(crate) mod repo;
pub(crate) mod tree;

impl SourceControlServiceImpl {
    pub(crate) async fn list_repos(
        &self,
        _ctx: CoreContext,
        _params: thrift::ListReposParams,
    ) -> Result<Vec<thrift::Repo>, errors::ServiceError> {
        let mut repo_names: Vec<_> = self.mononoke.repo_names_in_tier.clone();
        repo_names.sort();
        let rsp = repo_names
            .into_iter()
            .map(|repo_name| thrift::Repo {
                name: repo_name,
                ..Default::default()
            })
            .collect();
        Ok(rsp)
    }

    pub(crate) async fn async_ping(
        &self,
        ctx: CoreContext,
        params: thrift::AsyncPingParams,
    ) -> Result<thrift::AsyncPingToken, errors::ServiceError> {
        let queue = get_queue(&ctx, &self.async_requests_queue_client).await?;
        enqueue::<thrift::AsyncPingParams>(&ctx, &queue, None, params).await
    }

    pub(crate) async fn async_ping_poll(
        &self,
        ctx: CoreContext,
        token: thrift::AsyncPingToken,
    ) -> Result<thrift::AsyncPingPollResponse, errors::ServiceError> {
        let queue = get_queue(&ctx, &self.async_requests_queue_client).await?;
        let token = AsyncPingToken(token);
        poll::<AsyncPingToken>(&ctx, &queue, token).await
    }
}
