/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::Result;
use async_requests::tokens::MegarepoAddBranchingTargetToken;
use async_requests::tokens::MegarepoAddTargetToken;
use async_requests::tokens::MegarepoChangeTargetConfigToken;
use async_requests::tokens::MegarepoRemergeSourceToken;
use async_requests::tokens::MegarepoSyncChangesetToken;
use async_requests::types::IntoApiFormat;
use async_requests::types::IntoConfigFormat;
use async_requests::types::ThriftParams;
use async_requests::types::Token;
use context::CoreContext;
use megarepo_config::SyncTargetConfig;
use mononoke_api::ChangesetSpecifier;
use mononoke_api::MononokeError;
use mononoke_types::RepositoryId;
use repo_authorization::RepoWriteOperation;
use slog::warn;
use source_control as thrift;

use crate::errors;
use crate::from_request::FromRequest;
use crate::source_control_impl::SourceControlServiceImpl;

impl SourceControlServiceImpl {
    fn verify_repos_by_config(
        &self,
        config: &SyncTargetConfig,
    ) -> Result<(), errors::ServiceError> {
        let known_repo_ids: HashSet<RepositoryId> =
            self.mononoke.known_repo_ids().into_iter().collect();

        let repo_ids_in_cfg = {
            let mut repo_ids_in_cfg = Vec::new();
            repo_ids_in_cfg.push(config.target.repo_id);
            repo_ids_in_cfg.extend(config.sources.iter().map(|src| src.repo_id));
            repo_ids_in_cfg
        };

        for repo_id_in_cfg in repo_ids_in_cfg {
            if !known_repo_ids.contains(&RepositoryId::new(repo_id_in_cfg as i32)) {
                return Err(errors::ServiceError::from(errors::repo_not_found(format!(
                    "{}",
                    repo_id_in_cfg
                ))));
            }
        }

        Ok(())
    }

    async fn check_write_allowed(
        &self,
        ctx: &CoreContext,
        target_repo_id: RepositoryId,
    ) -> Result<(), errors::ServiceError> {
        let target_repo = self
            .mononoke
            .repo_by_id(ctx.clone(), target_repo_id)
            .await
            .map_err(errors::invalid_request)?
            .ok_or_else(|| errors::invalid_request(anyhow!("repo not found {}", target_repo_id)))?
            .build()
            .await?;
        // Check that source control service writes are enabled
        target_repo.start_write()?;
        // Check that we are allowed to write to the target repo
        target_repo
            .authorization_context()
            .require_repo_write(ctx, target_repo.repo(), RepoWriteOperation::MegarepoSync)
            .await
            .map_err(MononokeError::from)?;
        Ok(())
    }

    pub(crate) async fn megarepo_add_sync_target_config(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoAddConfigParams,
    ) -> Result<thrift::MegarepoAddConfigResponse, errors::ServiceError> {
        let target = params
            .new_config
            .target
            .clone()
            .into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;
        let repo_configs = self.configs.repo_configs();
        let (_, target_repo_config) = repo_configs
            .get_repo_config(target_repo_id)
            .ok_or_else(|| MononokeError::InvalidRequest("repo not found".to_string()))?;

        let new_config = params.new_config.into_config_format(&self.mononoke)?;
        self.verify_repos_by_config(&new_config)?;
        let megarepo_configs = self.megarepo_api.configs();
        megarepo_configs
            .add_config_version(
                ctx.clone(),
                Arc::new(target_repo_config.clone()),
                new_config.clone(),
            )
            .await?;

        // We've seen cases where config is not readable immediately after
        // it was written. Let's try reading a few times before returning
        let mut latest_error = None;
        for _ in 0..10 {
            let res = megarepo_configs
                .get_config_by_version(
                    ctx.clone(),
                    Arc::new(target_repo_config.clone()),
                    new_config.target.clone(),
                    new_config.version.clone(),
                )
                .await;
            match res {
                Ok(_) => {
                    latest_error = None;
                    break;
                }
                Err(err) => {
                    latest_error = Some(err);
                    warn!(
                        ctx.logger(),
                        "failed to read just written config version {}, retrying...",
                        new_config.version
                    );

                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }

        if let Some(err) = latest_error {
            return Err(errors::internal_error(format!(
                "Failed to read just written config version {}, error: {:?}",
                new_config.version, err
            ))
            .into());
        }

        Ok(thrift::MegarepoAddConfigResponse {
            ..Default::default()
        })
    }

    pub(crate) async fn megarepo_read_target_config(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoReadConfigParams,
    ) -> Result<thrift::MegarepoReadConfigResponse, errors::ServiceError> {
        let target = params.target.clone().into_config_format(&self.mononoke)?;
        let repo = self
            .megarepo_api
            .target_repo(&ctx, &target)
            .await
            .map_err(|err| {
                errors::invalid_request(anyhow!(
                    "can't open target repo {}: {}",
                    target.repo_id,
                    err
                ))
            })?;
        let changeset = repo
            .changeset(ChangesetSpecifier::from_request(&params.commit)?)
            .await?
            .ok_or_else(|| errors::invalid_request(anyhow!("commit not found")))?;
        let (_commit_remapping_state, target_config) = self
            .megarepo_api
            .get_target_sync_config(&ctx, &target, &changeset.id())
            .await?;

        Ok(thrift::MegarepoReadConfigResponse {
            config: target_config.into_api_format(&self.mononoke)?,
            ..Default::default()
        })
    }

    pub(crate) async fn megarepo_add_sync_target(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoAddTargetParams,
    ) -> Result<thrift::MegarepoAddTargetToken, errors::ServiceError> {
        let target = params.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;
        let config_with_new_target = params
            .config_with_new_target
            .clone()
            .into_config_format(&self.mononoke)?;
        self.verify_repos_by_config(&config_with_new_target)?;

        let token = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .enqueue(ctx, &self.mononoke, params)
            .await
            .map_err(|e| errors::internal_error(format!("Failed to enqueue the request: {}", e)))?;

        Ok(token.into_thrift())
    }

    pub(crate) async fn megarepo_add_sync_target_poll(
        &self,
        ctx: CoreContext,
        token: thrift::MegarepoAddTargetToken,
    ) -> Result<thrift::MegarepoAddTargetPollResponse, errors::ServiceError> {
        let token = MegarepoAddTargetToken(token);
        let target = token.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let poll_response = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .poll(ctx, token)
            .await?;

        Ok(poll_response)
    }

    pub(crate) async fn megarepo_add_branching_sync_target(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoAddBranchingTargetParams,
    ) -> Result<thrift::MegarepoAddBranchingTargetToken, errors::ServiceError> {
        let target = params.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let token = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .enqueue(ctx, &self.mononoke, params)
            .await
            .map_err(|e| errors::internal_error(format!("Failed to enqueue the request: {}", e)))?;

        Ok(token.into_thrift())
    }

    pub(crate) async fn megarepo_add_branching_sync_target_poll(
        &self,
        ctx: CoreContext,
        token: thrift::MegarepoAddBranchingTargetToken,
    ) -> Result<thrift::MegarepoAddBranchingTargetPollResponse, errors::ServiceError> {
        let token = MegarepoAddBranchingTargetToken(token);
        let target = token.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let poll_response = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .poll(ctx, token)
            .await?;

        Ok(poll_response)
    }

    pub(crate) async fn megarepo_change_target_config(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoChangeTargetConfigParams,
    ) -> Result<thrift::MegarepoChangeConfigToken, errors::ServiceError> {
        let target = params.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let token = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .enqueue(ctx, &self.mononoke, params)
            .await
            .map_err(|e| errors::internal_error(format!("Failed to enqueue the request: {}", e)))?;

        Ok(token.into_thrift())
    }

    pub(crate) async fn megarepo_change_target_config_poll(
        &self,
        ctx: CoreContext,
        token: thrift::MegarepoChangeConfigToken,
    ) -> Result<thrift::MegarepoChangeTargetConfigPollResponse, errors::ServiceError> {
        let token = MegarepoChangeTargetConfigToken(token);
        let target = token.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let poll_response = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .poll(ctx, token)
            .await?;

        Ok(poll_response)
    }

    pub(crate) async fn megarepo_sync_changeset(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoSyncChangesetParams,
    ) -> Result<thrift::MegarepoSyncChangesetToken, errors::ServiceError> {
        let target = params.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let token = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .enqueue(ctx, &self.mononoke, params)
            .await
            .map_err(|e| errors::internal_error(format!("Failed to enqueue the request: {}", e)))?;

        Ok(token.into_thrift())
    }

    pub(crate) async fn megarepo_sync_changeset_poll(
        &self,
        ctx: CoreContext,
        token: thrift::MegarepoSyncChangesetToken,
    ) -> Result<thrift::MegarepoSyncChangesetPollResponse, errors::ServiceError> {
        let token = MegarepoSyncChangesetToken(token);
        let target = token.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;
        let poll_response = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .poll(ctx, token)
            .await?;

        Ok(poll_response)
    }

    pub(crate) async fn megarepo_remerge_source(
        &self,
        ctx: CoreContext,
        params: thrift::MegarepoRemergeSourceParams,
    ) -> Result<thrift::MegarepoRemergeSourceToken, errors::ServiceError> {
        let target = params.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let token = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .enqueue(ctx, &self.mononoke, params)
            .await
            .map_err(|e| errors::internal_error(format!("Failed to enqueue the request: {}", e)))?;

        Ok(token.into_thrift())
    }

    pub(crate) async fn megarepo_remerge_source_poll(
        &self,
        ctx: CoreContext,
        token: thrift::MegarepoRemergeSourceToken,
    ) -> Result<thrift::MegarepoRemergeSourcePollResponse, errors::ServiceError> {
        let token = MegarepoRemergeSourceToken(token);
        let target = token.target().clone().into_config_format(&self.mononoke)?;
        let target_repo_id = RepositoryId::new(target.repo_id.try_into().unwrap());
        self.check_write_allowed(&ctx, target_repo_id).await?;

        let poll_response = self
            .megarepo_api
            .async_method_request_queue(&ctx, &target)
            .await?
            .poll(ctx, token)
            .await?;

        Ok(poll_response)
    }
}
