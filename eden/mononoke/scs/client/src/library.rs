/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Library contating code shared between commands.

pub(crate) mod bookmark;
pub(crate) mod commit;
pub(crate) mod commit_id;
pub(crate) mod diff;
pub(crate) mod path_tree;
pub(crate) mod summary;

use chrono::DateTime;
use chrono::FixedOffset;
use chrono::TimeZone;
use scs_client_raw::thrift;

pub fn datetime(datetime: &thrift::DateTime) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(datetime.tz)
        .unwrap()
        .timestamp_opt(datetime.timestamp, 0)
        .unwrap()
}
