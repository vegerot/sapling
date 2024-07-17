/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt;

use anyhow::Result;
use bitflags::bitflags;

use crate::errors::MononokeHgError;

bitflags! {
    // names are from hg revlog.py
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct RevFlags: u16 {
        const REVIDX_DEFAULT_FLAGS = 0;
        const REVIDX_EXTSTORED = 1 << 13;  // data is stored externally
        // Unused, not supported yet
        const REVIDX_ELLIPSIS = 1 << 14;  // revision hash does not match data (narrowhg)
    }
}

pub fn parse_rev_flags(flags: Option<u16>) -> Result<RevFlags> {
    // None -> Default
    // Some(valid) -> Ok(valid_flags)
    // Some(invalid) -> Err()
    match flags {
        Some(value) => match RevFlags::from_bits(value) {
            Some(value) => Ok(value),
            None => Err(MononokeHgError::UnknownRevFlags.into()),
        },
        None => Ok(RevFlags::REVIDX_DEFAULT_FLAGS),
    }
}

impl fmt::Display for RevFlags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.bits())
    }
}

impl From<RevFlags> for u64 {
    fn from(f: RevFlags) -> u64 {
        f.bits().into()
    }
}
