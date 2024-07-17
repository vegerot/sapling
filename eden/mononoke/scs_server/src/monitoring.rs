/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use mononoke_api::CoreContext;
use mononoke_api::Mononoke;
use slog::warn;

const SUBMIT_STATS_ONCE_PER_SECS: u64 = 10;

pub async fn monitoring_stats_submitter(ctx: CoreContext, mononoke: Arc<Mononoke>) {
    tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(
        SUBMIT_STATS_ONCE_PER_SECS,
    )))
    .for_each(|_| async {
        if let Err(e) = mononoke.report_monitoring_stats(&ctx).await {
            warn!(ctx.logger(), "Failed to report monitoring stats: {:#?}", e);
        }
    })
    .await;
}
