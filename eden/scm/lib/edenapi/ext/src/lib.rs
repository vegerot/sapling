/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! This module has auto-implemented functions that expand on SaplingRemoteAPI functionalities.
// Think of it as a SaplingRemoteAPIExt trait full of auto-implemented functions.
// It's not implemented like that because trait implementations can't be split in
// multiple files, so this is instead implemented as many functions in different files.
// Always use the format:
// fn my_function(api: &(impl SaplingRemoteApi + ?Sized), other_args...) -> ... {...}
// this way the function can be called from inside any trait that extends SaplingRemoteAPI.

mod files;
mod snapshot;
mod util;

pub use files::check_files;
pub use files::download_files;
pub use snapshot::upload_snapshot;
pub use util::calc_contentid;
