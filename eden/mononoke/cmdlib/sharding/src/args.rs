/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use fbinit::FacebookInit;
use slog::Logger;
use tokio::runtime::Handle;

use crate::RepoShardedProcess;
use crate::ShardedProcessExecutor;

/// Command line arguments for sharded executor
#[derive(Clone, Args, Debug)]
pub struct ShardedExecutorArgs {
    /// The name of the ShardManager service corresponding to this service's region.
    /// If this argument isn't provided, the service will operate in non-sharded mode.
    #[clap(long, requires = "sharded_scope_name")]
    pub sharded_service_name: Option<String>,
    /// The scope of the ShardManager service that this service corresponds to.
    #[clap(long, requires = "sharded_service_name")]
    pub sharded_scope_name: Option<String>,
}

impl ShardedExecutorArgs {
    pub fn build_executor(
        self,
        fb: FacebookInit,
        runtime: Handle,
        logger: &Logger,
        process_fn: impl FnOnce() -> Arc<dyn RepoShardedProcess>,
        shard_healing: bool,
        cleanup_timeout_secs: u64,
    ) -> Result<Option<ShardedProcessExecutor>> {
        if let Some((sharded_service_name, sharded_scope_name)) =
            self.sharded_service_name.zip(self.sharded_scope_name)
        {
            let process = process_fn();
            Ok(Some(ShardedProcessExecutor::new(
                fb,
                runtime,
                logger,
                // The service name & scope needs to be 'static to satisfy SM contract
                Box::leak(Box::new(sharded_service_name)),
                Box::leak(Box::new(sharded_scope_name)),
                cleanup_timeout_secs,
                process,
                shard_healing,
            )?))
        } else {
            Ok(None)
        }
    }
}
