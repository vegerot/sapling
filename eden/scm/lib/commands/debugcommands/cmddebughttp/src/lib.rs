/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use async_runtime::block_unless_interrupted as block_on;
use clidispatch::ReqCtx;
use cmdutil::NoOpts;
use cmdutil::Result;
use repo::repo::Repo;

pub fn run(ctx: ReqCtx<NoOpts>, repo: &Repo) -> Result<u8> {
    let client = edenapi::Builder::from_config(repo.config())?.build()?;
    let meta = block_on(client.health())?;
    ctx.io().write(format!("{:#?}\n", &meta))?;
    Ok(0)
}

pub fn aliases() -> &'static str {
    "debughttp"
}

pub fn doc() -> &'static str {
    "check whether the SaplingRemoteAPI server is reachable"
}

pub fn synopsis() -> Option<&'static str> {
    None
}
