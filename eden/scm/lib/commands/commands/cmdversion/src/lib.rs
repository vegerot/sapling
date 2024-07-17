/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use clidispatch::ReqCtx;
use cmdutil::NoOpts;
use cmdutil::Result;

#[cfg(feature = "fb")]
mod fb;

pub fn run(ctx: ReqCtx<NoOpts>) -> Result<u8> {
    let id = identity::default();
    let io = ctx.io();
    io.write(format!("{} {}\n", id.product_name(), ::version::VERSION))?;

    if !ctx.global_opts().quiet {
        io.write_err("(see https://sapling-scm.com/ for more information)\n")?;

        #[cfg(feature = "fb")]
        io.write_err(fb::VERSION_TEXT)?;
    }

    Ok(0)
}

pub fn aliases() -> &'static str {
    "version|vers|versi|versio"
}

pub fn doc() -> &'static str {
    "output version and copyright information"
}

pub fn synopsis() -> Option<&'static str> {
    None
}
