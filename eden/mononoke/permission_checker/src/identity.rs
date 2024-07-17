/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

pub type MononokeIdentitySet = BTreeSet<MononokeIdentity>;

#[derive(Clone, Eq, PartialEq, Debug, Hash, Ord, PartialOrd)]
pub struct MononokeIdentity {
    id_type: String,
    id_data: String,
}

impl MononokeIdentity {
    pub fn new(id_type: impl Into<String>, id_data: impl Into<String>) -> Self {
        let id_type = id_type.into();
        let id_data = id_data.into();

        Self { id_type, id_data }
    }

    pub fn id_type(&self) -> &str {
        &self.id_type
    }

    pub fn id_data(&self) -> &str {
        &self.id_data
    }

    pub fn is_of_type(&self, id_type: &str) -> bool {
        self.id_type == id_type
    }
}

impl fmt::Display for MononokeIdentity {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}:{}", self.id_type, self.id_data)
    }
}

impl FromStr for MononokeIdentity {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (ty, data) = value.split_once(':').with_context(|| {
            format!(
                "MononokeIdentity parse error, expected TYPE:data, got {:?}",
                value
            )
        })?;
        Ok(Self::new(ty, data))
    }
}

impl Serialize for MononokeIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MononokeIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

pub trait MononokeIdentitySetExt {
    fn is_quicksand(&self) -> bool;

    fn is_hg_sync_job(&self) -> bool;

    fn is_proxygen_test_identity(&self) -> bool;

    fn hostprefix(&self) -> Option<&str>;

    fn hostname(&self) -> Option<&str>;

    fn username(&self) -> Option<&str>;
    fn identity_type_filtered_concat(&self, id_type: &str) -> Option<String>;
    fn main_client_identity(&self) -> String;

    fn to_string(&self) -> String;
}

#[test]
fn test_ipv6_identity() {
    let id = MononokeIdentity::from_str("MACHINE:2621:10d:c1a8:12c9::1162").unwrap();
    assert_eq!(id.id_data(), "2621:10d:c1a8:12c9::1162");
}
