/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;

use context::CoreContext;
use megarepo_config::MononokeMegarepoConfigs;
use megarepo_config::SyncTargetConfig;
use megarepo_config::Target;
use megarepo_error::MegarepoError;
use metaconfig_types::RepoConfigArc;
use mononoke_api::Mononoke;
use mononoke_api::MononokeRepo;
use mononoke_types::ChangesetId;

use crate::common::find_target_sync_config;
use crate::common::MegarepoOp;

// Create a new sync target
pub struct AddBranchingSyncTarget<'a, R> {
    pub megarepo_configs: &'a Arc<dyn MononokeMegarepoConfigs>,
    pub mononoke: &'a Arc<Mononoke<R>>,
}

impl<'a, R> MegarepoOp<R> for AddBranchingSyncTarget<'a, R> {
    fn mononoke(&self) -> &Arc<Mononoke<R>> {
        self.mononoke
    }
}

impl<'a, R: MononokeRepo> AddBranchingSyncTarget<'a, R> {
    pub fn new(
        megarepo_configs: &'a Arc<dyn MononokeMegarepoConfigs>,
        mononoke: &'a Arc<Mononoke<R>>,
    ) -> Self {
        Self {
            megarepo_configs,
            mononoke,
        }
    }

    pub async fn run(
        self,
        ctx: &CoreContext,
        sync_target_config: SyncTargetConfig,
        branching_point: ChangesetId,
    ) -> Result<ChangesetId, MegarepoError> {
        let repo = self
            .find_repo_by_id(ctx, sync_target_config.target.repo_id)
            .await?;
        let repo_config = repo.repo().repo_config_arc();
        let bookmark = sync_target_config.target.bookmark.clone();

        self.megarepo_configs
            .add_config_version(ctx.clone(), repo_config, sync_target_config)
            .await?;
        self.create_bookmark(ctx, repo.repo(), bookmark, branching_point)
            .await?;
        Ok(branching_point)
    }

    pub async fn fork_new_sync_target_config(
        &self,
        ctx: &CoreContext,
        target: Target,
        branching_point: ChangesetId,
        source_target: Target,
    ) -> Result<SyncTargetConfig, MegarepoError> {
        let repo = self.find_repo_by_id(ctx, target.repo_id).await?;

        let (_, sync_target_config) = find_target_sync_config(
            ctx,
            repo.repo(),
            branching_point,
            &source_target,
            self.megarepo_configs,
        )
        .await?;

        let sync_target_config = SyncTargetConfig {
            target,
            ..sync_target_config
        };
        Ok(sync_target_config)
    }
}
