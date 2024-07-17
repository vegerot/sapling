/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashSet;

use anyhow::Result;
use clap::builder::PossibleValuesParser;
use clap::Args;
use context::CoreContext;
use context::SessionClass;
use derived_data_utils::derived_data_utils;
use derived_data_utils::POSSIBLE_DERIVED_TYPE_NAMES;
use futures_stats::TimedTryFutureExt;
use mononoke_api::ChangesetId;
use mononoke_app::args::ChangesetArgs;
use mononoke_types::DerivableType;
use repo_derived_data::RepoDerivedDataRef;
use slog::trace;

use super::Repo;

#[derive(Args)]
pub(super) struct DeriveArgs {
    #[clap(flatten)]
    changeset_args: ChangesetArgs,
    /// Type of derived data
    #[clap(long, short = 'T', required = true,  value_parser = PossibleValuesParser::new(POSSIBLE_DERIVED_TYPE_NAMES), group="types to derive")]
    derived_data_types: Vec<String>,
    /// Whether all enabled derived data types should be derived
    #[clap(long, required = true, group = "types to derive")]
    all_types: bool,
    /// Whether the changesets need to be rederived or not
    #[clap(long)]
    pub(crate) rederive: bool,
    /// Whether to derive from the predecessor of this derived data type
    #[clap(long)]
    from_predecessor: bool,
}

pub(super) async fn derive(ctx: &mut CoreContext, repo: &Repo, args: DeriveArgs) -> Result<()> {
    let resolved_csids = args.changeset_args.resolve_changesets(ctx, repo).await?;
    let csids = resolved_csids.as_slice();

    let derived_data_types = if args.all_types {
        // Derive all the types enabled in the config
        let derived_data_config = repo.repo_derived_data().active_config();
        derived_data_config.types.clone()
    } else {
        // Only derive the types specified by the user
        args.derived_data_types
            .into_iter()
            .map(|ty| DerivableType::from_name(&ty))
            .collect::<Result<HashSet<_>>>()?
    };

    for derived_data_type in derived_data_types {
        derive_data_type(
            ctx,
            repo,
            derived_data_type,
            csids,
            args.rederive,
            args.from_predecessor,
        )
        .await?;
    }

    Ok(())
}

async fn derive_data_type(
    ctx: &mut CoreContext,
    repo: &Repo,
    derived_data_type: DerivableType,
    csids: &[ChangesetId],
    rederive: bool,
    from_predecessor: bool,
) -> Result<()> {
    let derived_utils = derived_data_utils(ctx.fb, repo, derived_data_type)?;

    if rederive {
        trace!(ctx.logger(), "about to rederive {} commits", csids.len());
        derived_utils.regenerate(csids);
        // Force this binary to write to all blobstores
        ctx.session_mut()
            .override_session_class(SessionClass::Background);
    } else {
        trace!(ctx.logger(), "about to derive {} commits", csids.len());
    };

    for csid in csids {
        trace!(ctx.logger(), "deriving {}", csid);
        let (stats, res) = if from_predecessor {
            derived_utils
                .derive_from_predecessor(ctx.clone(), repo.repo_derived_data.clone(), *csid)
                .try_timed()
                .await?
        } else {
            derived_utils
                .derive(ctx.clone(), repo.repo_derived_data.clone(), *csid)
                .try_timed()
                .await?
        };
        trace!(
            ctx.logger(),
            "derived {} for {} in {}ms, {:?}",
            derived_data_type.name(),
            csid,
            stats.completion_time.as_millis(),
            res,
        );
    }

    Ok(())
}
