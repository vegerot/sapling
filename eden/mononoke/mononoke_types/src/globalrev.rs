/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt;
use std::fmt::Display;
use std::str;
use std::str::FromStr;

use abomonation_derive::Abomonation;
use anyhow::bail;
use anyhow::Error;
use anyhow::Result;
use sql::mysql;

use crate::BonsaiChangeset;
use crate::BonsaiChangesetMut;

pub const GLOBALREV_EXTRA: &str = "global_rev";

// Globalrev of first commit when globalrevs were introduced in Mercurial.
// To get globalrev from commit we want to check whether there exists "global_rev" key in bcs extras
// and is not less than START_COMMIT_GLOBALREV.
// Otherwise we try to fetch "convert_revision" key, and parse svnrev from it.
pub const START_COMMIT_GLOBALREV: u64 = 1000147970;

// Changeset globalrev.
#[derive(Abomonation, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[derive(mysql::OptTryFromRowField)]
pub struct Globalrev(u64);

impl Globalrev {
    pub const fn start_commit() -> Self {
        Self(START_COMMIT_GLOBALREV)
    }

    #[inline]
    pub const fn new(rev: u64) -> Self {
        Self(rev)
    }

    #[inline]
    pub fn id(&self) -> u64 {
        self.0
    }

    // ex. svn:uuid/path@1234
    pub fn parse_svnrev(svnrev: &str) -> Result<u64> {
        let at_pos = svnrev
            .rfind('@')
            .ok_or_else(|| Error::msg("Wrong convert_revision value"))?;
        let result = svnrev[1 + at_pos..].parse::<u64>()?;
        Ok(result)
    }

    pub fn from_bcs(bcs: &BonsaiChangeset) -> Result<Self> {
        match (
            bcs.hg_extra().find(|(key, _)| key == &GLOBALREV_EXTRA),
            bcs.hg_extra().find(|(key, _)| key == &"convert_revision"),
        ) {
            (Some((_, globalrev)), Some((_, svnrev))) => {
                let globalrev = str::from_utf8(globalrev)?.parse::<u64>()?;
                let svnrev = Globalrev::parse_svnrev(str::from_utf8(svnrev)?)?;
                if globalrev >= START_COMMIT_GLOBALREV {
                    Ok(Self::new(globalrev))
                } else {
                    Ok(Self::new(svnrev))
                }
            }
            (Some((_, globalrev)), None) => {
                let globalrev = str::from_utf8(globalrev)?.parse::<u64>()?;
                if globalrev < START_COMMIT_GLOBALREV {
                    bail!("Bonsai cs {:?} without globalrev", bcs)
                } else {
                    Ok(Self::new(globalrev))
                }
            }
            (None, Some((_, svnrev))) => {
                let svnrev = Globalrev::parse_svnrev(str::from_utf8(svnrev)?)?;
                Ok(Self::new(svnrev))
            }
            (None, None) => bail!("Bonsai cs {:?} without globalrev", bcs),
        }
    }

    pub fn set_on_changeset(&self, bcs: &mut BonsaiChangesetMut) {
        bcs.hg_extra.insert(
            GLOBALREV_EXTRA.into(),
            format!("{}", self.id()).into_bytes(),
        );
    }

    #[must_use = "increment does not modify the generation object"]
    pub fn increment(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for Globalrev {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.0, fmt)
    }
}

impl FromStr for Globalrev {
    type Err = <u64 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Globalrev::new)
    }
}
