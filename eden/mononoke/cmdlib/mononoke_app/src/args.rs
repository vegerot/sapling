/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::format_err;
use anyhow::Result;

mod acl;
mod changeset;
mod derived_data;
mod gflags;
mod hooks;
mod just_knobs;
mod mcrouter;
mod mysql;
mod readonly;
mod repo;
mod repo_blobstore;
mod repo_filter;
mod runtime;
mod wbc;

pub use acl::AclArgs;
pub use changeset::ChangesetArgs;
pub use config_args::ConfigArgs;
pub use config_args::ConfigMode;
pub use derived_data::DerivedDataArgs;
pub use derived_data::MultiDerivedDataArgs;
pub use gflags::GFlagsArgs;
pub use hooks::HooksAppExtension;
pub use just_knobs::JustKnobsArgs;
pub use mcrouter::McrouterAppExtension;
pub use mcrouter::McrouterArgs;
pub use mysql::MysqlArgs;
pub use readonly::ReadonlyArgs;
pub use repo::AsRepoArg;
pub use repo::MultiRepoArgs;
pub use repo::OptRepoArgs;
pub use repo::OptSourceAndTargetRepoArgs;
pub use repo::RepoArg;
pub use repo::RepoArgs;
pub use repo::SourceAndTargetRepoArgs;
pub use repo::SourceRepoArgs;
pub use repo::TargetRepoArgs;
pub use repo_blobstore::RepoBlobstoreArgs;
pub use repo_filter::RepoFilterAppExtension;
pub use runtime::RuntimeArgs;
pub use shutdown_timeout::ShutdownTimeoutArgs;
pub use tls::TLSArgs;
pub use wbc::WarmBookmarksCacheExtension;

pub use crate::fb303::Fb303Args;
pub use crate::repo_args;
pub use crate::repo_args_optional;

/// NOTE: Don't use this. "configerator:" prefix don't need to exist and is going to be removed.
/// Pass raw path instead.
pub fn parse_config_spec_to_path(source_spec: &str) -> Result<String> {
    // NOTE: This means we don't support file paths with ":" in them, but it also means we can
    // add other options after the first ":" later if we want.
    let mut iter = source_spec.split(':');

    // NOTE: We match None as the last element to make sure the input doesn't contain
    // disallowed trailing parts.
    match (iter.next(), iter.next(), iter.next()) {
        (Some("configerator"), Some(path), None) => Ok(path.to_string()),
        (Some(path), None, None) => Ok(path.to_string()),
        _ => Err(format_err!("Invalid configuration spec: {:?}", source_spec)),
    }
}
