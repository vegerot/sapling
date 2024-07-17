/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::anyhow;
use blobrepo::BlobRepo;
use changeset_fetcher::ChangesetFetcherArc;
use clap_old::App;
use clap_old::Arg;
use clap_old::ArgMatches;
use clap_old::SubCommand;
use cmdlib::args;
use cmdlib::args::MononokeMatches;
use cmdlib::helpers;
use context::CoreContext;
use fbinit::FacebookInit;
use futures::compat::Stream01CompatExt;
use futures::StreamExt;
use futures::TryStreamExt;
use revset::AncestorsNodeStream;
use slog::Logger;

use crate::error::SubcommandError;

pub const LIST_ANCESTORS: &str = "list-ancestors";
const ARG_CHANGESET: &str = "changeset-id";
const ARG_LIMIT: &str = "limit";

pub fn build_subcommand<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(LIST_ANCESTORS)
        .about("list ancestors of a commit")
        .arg(
            Arg::with_name(ARG_CHANGESET)
                .required(true)
                .takes_value(true)
                .help("hg/bonsai changeset id or bookmark to start listing ancestors from"),
        )
        .arg(
            Arg::with_name(ARG_LIMIT)
                .long(ARG_LIMIT)
                .short("l")
                .takes_value(true)
                .required(false)
                .help("Imposes the limit on number of log records in output."),
        )
}

pub async fn list_ancestors<'a>(
    fb: FacebookInit,
    logger: Logger,
    matches: &'a MononokeMatches<'_>,
    sub_m: &'a ArgMatches<'_>,
) -> Result<(), SubcommandError> {
    let ctx = CoreContext::new_with_logger_and_client_info(
        fb,
        logger.clone(),
        ClientInfo::default_with_entry_point(ClientEntryPoint::MononokeAdmin),
    );
    let repo: BlobRepo =
        args::not_shardmanager_compatible::open_repo(fb, ctx.logger(), matches).await?;
    let rev = sub_m
        .value_of(ARG_CHANGESET)
        .ok_or_else(|| anyhow!("{} is not set", ARG_CHANGESET))?;
    let limit = args::get_usize(sub_m, ARG_LIMIT, 10);

    let cs_id = helpers::csid_resolve(&ctx, &repo, rev).await?;

    let ancestors = AncestorsNodeStream::new(ctx, &repo.changeset_fetcher_arc(), cs_id)
        .compat()
        .take(limit)
        .try_collect::<Vec<_>>()
        .await?;

    for cs_id in ancestors {
        println!("{}", cs_id);
    }
    Ok(())
}
