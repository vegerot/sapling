/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! edenfsctl debug

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;

use crate::ExitCode;
use crate::Subcommand;

mod clear_local_caches;
mod compact_local_storage;
mod subscribe;

#[derive(Parser, Debug)]
#[clap(
    about = "Internal commands for examining eden state",
    disable_help_flag = true
)]
pub struct DebugCmd {
    #[clap(subcommand)]
    subcommand: DebugSubcommand,
}

#[derive(Parser, Debug)]
pub enum DebugSubcommand {
    ClearLocalCaches(clear_local_caches::ClearLocalCachesCmd),
    CompactLocalStorage(compact_local_storage::CompactLocalStorageCmd),
    Subscribe(subscribe::SubscribeCmd),
}

#[async_trait]
impl Subcommand for DebugCmd {
    async fn run(&self) -> Result<ExitCode> {
        use DebugSubcommand::*;
        let sc: &(dyn Subcommand + Send + Sync) = match &self.subcommand {
            ClearLocalCaches(cmd) => cmd,
            CompactLocalStorage(cmd) => cmd,
            Subscribe(cmd) => cmd,
        };
        sc.run().await
    }
}
