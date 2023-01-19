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
mod config;
mod hooks;
mod mcrouter;
mod mysql;
mod readonly;
mod repo;
mod repo_blobstore;
mod repo_filter;
mod runtime;
mod shutdown_timeout;
mod tls;
mod tunables;
mod wbc;

pub use acl::AclArgs;
pub use changeset::ChangesetArgs;
pub use config::ConfigArgs;
pub use config::ConfigMode;
pub use hooks::HooksAppExtension;
pub use mcrouter::McrouterAppExtension;
pub use mcrouter::McrouterArgs;
pub use mysql::MysqlArgs;
pub use readonly::ReadonlyArgs;
pub use repo::AsRepoArg;
pub use repo::MultiRepoArgs;
pub use repo::RepoArg;
pub use repo::RepoArgs;
pub use repo::SourceAndTargetRepoArgs;
pub use repo_blobstore::RepoBlobstoreArgs;
pub use repo_filter::RepoFilterAppExtension;
pub use runtime::RuntimeArgs;
pub use shutdown_timeout::ShutdownTimeoutArgs;
pub use tls::TLSArgs;
pub use wbc::WarmBookmarksCacheExtension;

pub use self::tunables::TunablesArgs;
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
