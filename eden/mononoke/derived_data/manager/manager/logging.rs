/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::time::Duration;

use anyhow::Error;
use anyhow::Result;
use context::CoreContext;
use context::PerfCounters;
use derived_data_constants::*;
use futures_stats::FutureStats;
use mononoke_types::ChangesetId;
use scuba_ext::MononokeScubaSampleBuilder;
use slog::warn;
use time_ext::DurationExt;

use super::derive::DerivationOutcome;
use super::DerivedDataManager;
use crate::derivable::BonsaiDerivable;
use crate::error::DerivationError;

impl DerivedDataManager {
    /// Log the start of derivation to both the request and derived data scuba
    /// tables.
    pub(super) fn log_derivation_start<Derivable>(
        &self,
        ctx: &CoreContext,
        derived_data_scuba: &mut MononokeScubaSampleBuilder,
        csid: ChangesetId,
    ) where
        Derivable: BonsaiDerivable,
    {
        ctx.scuba().clone().log_with_msg(
            DERIVATION_START,
            Some(format!("{} {}", Derivable::NAME, csid)),
        );
        derived_data_scuba.log_with_msg(DERIVATION_START, None);
    }

    /// Log the end of derivation to both the request and derived data scuba
    /// tables.
    pub(super) fn log_derivation_end<Derivable>(
        &self,
        ctx: &CoreContext,
        derived_data_scuba: &mut MononokeScubaSampleBuilder,
        csid: ChangesetId,
        stats: &FutureStats,
        error: Option<&Error>,
    ) where
        Derivable: BonsaiDerivable,
    {
        let (tag, error_str) = match error {
            None => (DERIVATION_END, None),
            Some(error) => (FAILED_DERIVATION, Some(format!("{:#}", error))),
        };

        let mut ctx_scuba = ctx.scuba().clone();
        ctx_scuba.add_future_stats(stats);
        if let Some(error_str) = &error_str {
            ctx_scuba.add("Derive error", error_str.as_str());
        };
        ctx_scuba.log_with_msg(tag, Some(format!("{} {}", Derivable::NAME, csid)));

        ctx.perf_counters().insert_perf_counters(derived_data_scuba);

        derived_data_scuba.add_future_stats(stats);
        derived_data_scuba.log_with_msg(tag, error_str);
    }

    /// Log the start of batch derivation to both the request and derived data
    /// scuba tables.
    pub(super) fn log_batch_derivation_start<Derivable>(
        &self,
        ctx: &CoreContext,
        derived_data_scuba: &mut MononokeScubaSampleBuilder,
        csid_range: Option<(ChangesetId, ChangesetId)>,
    ) where
        Derivable: BonsaiDerivable,
    {
        if let Some((first, last)) = csid_range {
            ctx.scuba().clone().log_with_msg(
                DERIVATION_START_BATCH,
                Some(format!("{} {}-{}", Derivable::NAME, first, last)),
            );
            derived_data_scuba.log_with_msg(DERIVATION_START_BATCH, None);
        }
    }

    /// Log the end of derivation to both the request and derived data scuba
    /// tables.
    pub(super) fn log_batch_derivation_end<Derivable>(
        &self,
        ctx: &CoreContext,
        derived_data_scuba: &mut MononokeScubaSampleBuilder,
        csid_range: Option<(ChangesetId, ChangesetId)>,
        stats: &FutureStats,
        error: Option<&Error>,
    ) where
        Derivable: BonsaiDerivable,
    {
        if let Some((first, last)) = csid_range {
            let (tag, error_str) = match error {
                None => (DERIVATION_END_BATCH, None),
                Some(error) => (FAILED_DERIVATION_BATCH, Some(format!("{:#}", error))),
            };

            let mut ctx_scuba = ctx.scuba().clone();
            ctx_scuba.add_future_stats(stats);
            if let Some(error_str) = &error_str {
                ctx_scuba.add("Derive error", error_str.as_str());
            };
            ctx_scuba.log_with_msg(tag, Some(format!("{} {}-{}", Derivable::NAME, first, last)));

            ctx.perf_counters().insert_perf_counters(derived_data_scuba);

            derived_data_scuba.add_future_stats(stats);
            derived_data_scuba.log_with_msg(tag, error_str);
        }
    }

    /// Log the insertion of a new derived data mapping to the derived data
    /// scuba table.
    pub(super) fn log_mapping_insertion<Derivable>(
        &self,
        ctx: &CoreContext,
        derived_data_scuba: &mut MononokeScubaSampleBuilder,
        value: Option<&Derivable>,
        stats: &FutureStats,
        error: Option<&Error>,
    ) where
        Derivable: BonsaiDerivable,
    {
        let (tag, error_str) = match error {
            None => (INSERTED_MAPPING, None),
            Some(error) => (FAILED_INSERTING_MAPPING, Some(format!("{:#}", error))),
        };

        ctx.perf_counters().insert_perf_counters(derived_data_scuba);

        if let Some(value) = value {
            // Limit how much we log to scuba.
            let value = format!("{:1000?}", value);
            derived_data_scuba.add("mapping_value", value);
        }

        derived_data_scuba
            .add_future_stats(stats)
            .log_with_msg(tag, error_str);
    }

    pub(super) fn should_log_slow_derivation(&self, duration: Duration) -> bool {
        let threshold = tunables::tunables()
            .derived_data_slow_derivation_threshold_secs()
            .unwrap_or_default();
        let threshold = match threshold.try_into() {
            Ok(t) if t > 0 => t,
            _ => return false,
        };
        duration > Duration::from_secs(threshold)
    }

    pub(super) fn log_slow_derivation<Derivable>(
        &self,
        ctx: &CoreContext,
        csid: ChangesetId,
        stats: &FutureStats,
        pc: &PerfCounters,
        result: &Result<DerivationOutcome<Derivable>, DerivationError>,
    ) where
        Derivable: BonsaiDerivable,
    {
        let mut scuba = ctx.scuba().clone();
        pc.insert_perf_counters(&mut scuba);

        scuba.add_future_stats(stats);
        scuba.add("changeset_id", csid.to_string());
        scuba.add("derived_data_type", Derivable::NAME);
        scuba.add("repo", self.repo_name());

        match result {
            Ok(derivation_outcome) => {
                scuba.add("derived", derivation_outcome.count);
                scuba.add(
                    "find_underived_completion_time_ms",
                    derivation_outcome.find_underived_time.as_millis_unchecked(),
                );
                warn!(
                    ctx.logger(),
                    "slow derivation of {} for {}, took {:.2?} (find_underived: {:.2?}), derived {} changesets",
                    Derivable::NAME,
                    csid,
                    stats.completion_time,
                    derivation_outcome.find_underived_time,
                    derivation_outcome.count,
                );
            }
            Err(derivation_error) => {
                warn!(
                    ctx.logger(),
                    "slow derivation of {} for {}, took {:.2?}, failed with Err({:?})",
                    Derivable::NAME,
                    csid,
                    stats.completion_time,
                    derivation_error,
                );
            }
        }

        scuba.log_with_msg(SLOW_DERIVATION, None);
    }
}
