/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::Error;
use async_trait::async_trait;
use cached_config::ConfigHandle;
use fbinit::FacebookInit;
use permission_checker::MononokeIdentitySet;
use permission_checker::MononokeIdentitySetExt;
use scuba_ext::MononokeScubaSampleBuilder;
use stats::prelude::*;
use thiserror::Error;

#[cfg(fbcode_build)]
mod facebook;
#[cfg(not(fbcode_build))]
mod oss;

#[cfg(fbcode_build)]
pub use facebook::create_rate_limiter;
#[cfg(not(fbcode_build))]
pub use oss::create_rate_limiter;
pub use rate_limiting_config::RateLimitStatus;

pub mod config;

pub type LoadCost = f64;
pub type BoxRateLimiter = Box<dyn RateLimiter + Send + Sync + 'static>;

pub enum RateLimitResult {
    Pass,
    Fail(RateLimitReason),
}

#[async_trait]
pub trait RateLimiter {
    async fn check_rate_limit(
        &self,
        metric: Metric,
        identities: &MononokeIdentitySet,
        main_id: Option<&str>,
        scuba: &mut MononokeScubaSampleBuilder,
    ) -> Result<RateLimitResult, Error>;

    fn check_load_shed(
        &self,
        identities: &MononokeIdentitySet,
        main_id: Option<&str>,
        scuba: &mut MononokeScubaSampleBuilder,
    ) -> LoadShedResult;

    fn bump_load(&self, metric: Metric, scope: Scope, load: LoadCost);

    fn category(&self) -> &str;

    fn commits_per_author_limit(&self) -> Option<RateLimit>;

    fn total_file_changes_limit(&self) -> Option<RateLimitBody>;
}

define_stats! {
    load_shed_counter: dynamic_singleton_counter("{}", (key: String)),
}

#[derive(Clone)]
pub struct RateLimitEnvironment {
    fb: FacebookInit,
    category: String,
    config: ConfigHandle<MononokeRateLimitConfig>,
}

impl RateLimitEnvironment {
    pub fn new(
        fb: FacebookInit,
        category: String,
        config: ConfigHandle<MononokeRateLimitConfig>,
    ) -> Self {
        Self {
            fb,
            category,
            config,
        }
    }

    pub fn get_rate_limiter(&self) -> BoxRateLimiter {
        let config = self.config.get();

        create_rate_limiter(self.fb, self.category.clone(), config)
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitBody {
    pub raw_config: rate_limiting_config::RateLimitBody,
    pub window: Duration,
}

#[derive(Debug, Clone)]
pub struct MononokeRateLimitConfig {
    pub rate_limits: Vec<RateLimit>,
    pub load_shed_limits: Vec<LoadShedLimit>,
    #[allow(dead_code)]
    total_file_changes: Option<RateLimitBody>,
}

#[derive(Debug, Clone)]
pub struct RateLimit {
    pub body: RateLimitBody,
    #[allow(dead_code)]
    target: Option<Target>,
    #[allow(dead_code)]
    fci_metric: FciMetric,
}

#[cfg(fbcode_build)]
impl RateLimit {
    fn applies_to_client(&self, identities: &MononokeIdentitySet, main_id: Option<&str>) -> bool {
        match &self.target {
            // TODO (harveyhunt): Pass identities rather than Some(identities) once LFS server has
            // been updated to require certs.
            Some(t) => t.matches_client(Some(identities), main_id),
            None => true,
        }
    }
}

pub enum LoadShedResult {
    Pass,
    Fail(RateLimitReason),
}

pub fn log_or_enforce_status(
    raw_config: rate_limiting_config::LoadShedLimit,
    metric: String,
    value: i64,
    scuba: &mut MononokeScubaSampleBuilder,
) -> LoadShedResult {
    match raw_config.status {
        RateLimitStatus::Disabled => LoadShedResult::Pass,
        RateLimitStatus::Tracked => {
            scuba.log_with_msg(
                "Would have rate limited",
                format!(
                    "{:?}",
                    (RateLimitReason::LoadShedMetric(metric, value, raw_config.limit,))
                ),
            );
            LoadShedResult::Pass
        }
        RateLimitStatus::Enforced => LoadShedResult::Fail(RateLimitReason::LoadShedMetric(
            metric,
            value,
            raw_config.limit,
        )),
        _ => panic!(
            "Thrift enums aren't real enums once in Rust. We have to account for other values here."
        ),
    }
}

impl LoadShedLimit {
    // TODO(harveyhunt): Make identities none optional once LFS server enforces that.
    pub fn should_load_shed(
        &self,
        fb: FacebookInit,
        identities: Option<&MononokeIdentitySet>,
        main_id: Option<&str>,
        scuba: &mut MononokeScubaSampleBuilder,
    ) -> LoadShedResult {
        let applies_to_client = match &self.target {
            Some(t) => t.matches_client(identities, main_id),
            None => true,
        };

        if !applies_to_client {
            return LoadShedResult::Pass;
        }

        let metric = self.raw_config.metric.to_string();

        match STATS::load_shed_counter.get_value(fb, (metric.clone(),)) {
            Some(value) if value > self.raw_config.limit => {
                log_or_enforce_status(self.raw_config.clone(), metric, value, scuba)
            }
            _ => LoadShedResult::Pass,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadShedLimit {
    pub raw_config: rate_limiting_config::LoadShedLimit,
    target: Option<Target>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Metric {
    EgressBytes,
    TotalManifests,
    GetpackFiles,
    Commits,
    CommitsPerAuthor,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Scope {
    Global,
    Regional,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FciMetric {
    metric: Metric,
    window: Duration,
    scope: Scope,
}

#[must_use]
#[derive(Debug, Error)]
pub enum RateLimitReason {
    #[error("Rate limited by {0:?} over {1:?}")]
    RateLimitedMetric(Metric, Duration),
    #[error("Load shed due to {0} (value: {1}, limit: {2})")]
    LoadShedMetric(String, i64, i64),
}

#[derive(Debug, Clone)]
pub enum Target {
    StaticSlice(StaticSlice),
    MainClientId(String),
    Identities(MononokeIdentitySet),
}

#[derive(Debug, Copy, Clone)]
struct SlicePct(u8);

impl TryFrom<i32> for SlicePct {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if !(0..=100).contains(&value) {
            return Err(anyhow!("Invalid percentage"));
        }

        Ok(Self(value.try_into()?))
    }
}

#[derive(Debug, Clone)]
pub struct StaticSlice {
    slice_pct: SlicePct,
    // This is hashed with a client's hostname to allow us to change
    // which percentage of hosts are in a static slice.
    nonce: String,
    target: StaticSliceTarget,
}

#[derive(Debug, Clone)]
pub enum StaticSliceTarget {
    Identities(MononokeIdentitySet),
    MainClientId(String),
}

impl Target {
    pub fn matches_client(
        &self,
        identities: Option<&MononokeIdentitySet>,
        main_client_id: Option<&str>,
    ) -> bool {
        match self {
            Self::Identities(target_identities) => {
                match identities {
                    Some(identities) => {
                        // Check that identities is a subset of client_idents
                        target_identities.is_subset(identities)
                    }
                    None => false,
                }
            }
            Self::MainClientId(id) => match main_client_id {
                Some(client_id) => client_id == id,
                None => false,
            },
            Self::StaticSlice(s) => {
                // Check that identities is a subset of client_idents
                match matches_static_slice_target(&s.target, identities, main_client_id) {
                    true => in_throttled_slice(identities, s.slice_pct, &s.nonce),
                    false => false,
                }
            }
        }
    }
}

fn matches_static_slice_target(
    target: &StaticSliceTarget,
    identities: Option<&MononokeIdentitySet>,
    main_client_id: Option<&str>,
) -> bool {
    match target {
        StaticSliceTarget::Identities(target_identities) => {
            match identities {
                Some(identities) => {
                    // Check that identities is a subset of client_idents
                    target_identities.is_subset(identities)
                }
                None => false,
            }
        }
        StaticSliceTarget::MainClientId(id) => match main_client_id {
            Some(client_id) => client_id == id,
            None => false,
        },
    }
}

fn in_throttled_slice(
    identities: Option<&MononokeIdentitySet>,
    slice_pct: SlicePct,
    nonce: &str,
) -> bool {
    let hostname = if let Some(hostname) = identities.map(|i| i.hostname()) {
        hostname
    } else {
        return false;
    };

    let mut hasher = DefaultHasher::new();
    hostname.hash(&mut hasher);
    nonce.hash(&mut hasher);

    hasher.finish() % 100 < slice_pct.0.into()
}

#[cfg(test)]
mod test {
    use mononoke_macros::mononoke;
    use permission_checker::MononokeIdentity;

    use super::*;

    #[mononoke::test]
    fn test_target_matches() {
        let test_ident = MononokeIdentity::new("USER", "foo");
        let test2_ident = MononokeIdentity::new("USER", "baz");
        let test_client_id = String::from("test_client_id");
        let empty_idents = Some(MononokeIdentitySet::new());

        let ident_target = Target::Identities([test_ident.clone()].into());

        assert!(!ident_target.matches_client(empty_idents.as_ref(), None));

        let mut idents = MononokeIdentitySet::new();
        idents.insert(test_ident.clone());
        idents.insert(test2_ident.clone());
        let idents = Some(idents);

        assert!(ident_target.matches_client(idents.as_ref(), None));

        let two_idents = Target::Identities([test_ident, test2_ident].into());

        assert!(two_idents.matches_client(idents.as_ref(), None));

        let client_id_target = Target::MainClientId(test_client_id.clone());
        assert!(client_id_target.matches_client(None, Some(&test_client_id)))
    }

    #[mononoke::test]
    fn test_target_in_static_slice() {
        let mut identities = MononokeIdentitySet::new();
        identities.insert(MononokeIdentity::new("MACHINE", "abc123.abc1.facebook.com"));

        assert!(!in_throttled_slice(None, 100.try_into().unwrap(), "abc"));

        assert!(!in_throttled_slice(
            Some(&identities),
            0.try_into().unwrap(),
            "abc"
        ));

        assert!(in_throttled_slice(
            Some(&identities),
            100.try_into().unwrap(),
            "abc"
        ));

        assert!(in_throttled_slice(
            Some(&identities),
            50.try_into().unwrap(),
            "123"
        ));

        // Check that changing the nonce results in a different slice.
        assert!(!in_throttled_slice(
            Some(&identities),
            50.try_into().unwrap(),
            "abc"
        ));
    }

    #[cfg(fbcode_build)]
    #[mononoke::test]
    fn test_static_slice_of_identity_set() {
        let test_ident = MononokeIdentity::new("USER", "foo");
        let test2_ident = MononokeIdentity::new("SERVICE_IDENTITY", "bar");
        let test3_ident = MononokeIdentity::new("MACHINE", "abc125.abc.facebook.com");
        let test4_ident = MononokeIdentity::new("MACHINE", "abc124.abc.facebook.com");

        let ident_target = Target::Identities([test2_ident.clone()].into());
        let twenty_pct_service_identity = Target::StaticSlice(StaticSlice {
            slice_pct: 20.try_into().unwrap(),
            nonce: "nonce".into(),
            target: StaticSliceTarget::Identities([test2_ident.clone()].into()),
        });
        let hundred_pct_service_identity = Target::StaticSlice(StaticSlice {
            slice_pct: 100.try_into().unwrap(),
            nonce: "nonce".into(),
            target: StaticSliceTarget::Identities([test2_ident.clone()].into()),
        });

        let mut idents = MononokeIdentitySet::new();
        idents.insert(test_ident.clone());
        idents.insert(test2_ident.clone());
        idents.insert(test3_ident);
        let idents1 = Some(idents);

        let mut idents = MononokeIdentitySet::new();
        idents.insert(test_ident);
        idents.insert(test2_ident);
        idents.insert(test4_ident);
        let idents2 = Some(idents);

        // All of SERVICE_IDENTITY: bar
        assert!(ident_target.matches_client(idents1.as_ref(), None));

        // 20% of SERVICE_IDENTITY: bar. ratelimited host
        assert!(twenty_pct_service_identity.matches_client(idents1.as_ref(), None));

        // 20% of SERVICE_IDENTITY: bar. not ratelimited host
        assert!(!twenty_pct_service_identity.matches_client(idents2.as_ref(), None));

        // 100% of SERVICE_IDENTITY: bar
        assert!(hundred_pct_service_identity.matches_client(idents1.as_ref(), None));

        // 100% of SERVICE_IDENTITY: bar
        assert!(hundred_pct_service_identity.matches_client(idents2.as_ref(), None));
    }
}
