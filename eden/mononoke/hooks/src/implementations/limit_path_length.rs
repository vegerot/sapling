/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::anyhow;
use anyhow::Error;
use anyhow::Result;
use async_trait::async_trait;
use context::CoreContext;
use mercurial_types::simple_fsencode;
use mononoke_types::BasicFileChange;
use mononoke_types::NonRootMPath;
use serde::Deserialize;

use crate::CrossRepoPushSource;
use crate::FileHook;
use crate::HookConfig;
use crate::HookExecution;
use crate::HookFileContentProvider;
use crate::HookRejectionInfo;
use crate::PushAuthoredBy;

// The filesystem max is 255.
const MAX_PATH_COMPONENT_LIMIT: usize = 255;

#[derive(Clone, Debug, Deserialize)]
pub struct LimitPathLengthConfig {
    length_limit: usize,
}

/// Hook to block commits changing file paths above a certain length.
/// This is needed because Mercurial has these limits and we shouldn't allow
/// commits that can't be synced to mercurial.
#[derive(Clone, Debug)]
pub struct LimitPathLengthHook {
    config: LimitPathLengthConfig,
}

impl LimitPathLengthHook {
    pub fn new(config: &HookConfig) -> Result<Self> {
        Self::with_config(config.parse_options()?)
    }

    pub fn with_config(config: LimitPathLengthConfig) -> Result<Self> {
        Ok(Self { config })
    }
}

#[async_trait]
impl FileHook for LimitPathLengthHook {
    async fn run<'this: 'change, 'ctx: 'this, 'change, 'fetcher: 'change, 'path: 'change>(
        &'this self,
        _ctx: &'ctx CoreContext,
        _content_manager: &'fetcher dyn HookFileContentProvider,
        change: Option<&'change BasicFileChange>,
        path: &'path NonRootMPath,
        _cross_repo_push_source: CrossRepoPushSource,
        push_authored_by: PushAuthoredBy,
    ) -> Result<HookExecution, Error> {
        if push_authored_by.service() {
            return Ok(HookExecution::Accepted);
        }
        if change.is_none() {
            // You can always delete paths
            return Ok(HookExecution::Accepted);
        }

        // Encode file in mercurial encoding to make sure mercurial can accept those files
        // as well
        if let Some(rejection) = check_path(path)? {
            return Ok(rejection);
        }

        let len = path.len();

        let execution = if len >= self.config.length_limit {
            HookExecution::Rejected(HookRejectionInfo::new_long(
                "Path too long",
                format!(
                    "Path length for '{}' ({}) exceeds length limit (>= {})",
                    path, len, self.config.length_limit
                ),
            ))
        } else {
            HookExecution::Accepted
        };

        Ok(execution)
    }
}

fn check_path(path: &NonRootMPath) -> Result<Option<HookExecution>, Error> {
    let mut elements = path
        .as_ref()
        .iter()
        .map(|e| e.as_ref())
        .collect::<Vec<&[u8]>>();

    let basename = elements
        .pop()
        .ok_or_else(|| anyhow!("invalid path - no basename!"))?;

    let mut basename = Vec::from(basename);
    basename.extend(b".i");
    elements.push(&basename);

    let encoded_index_path = simple_fsencode(&elements);

    for component in encoded_index_path.iter() {
        if component.len() > MAX_PATH_COMPONENT_LIMIT {
            return Ok(Some(HookExecution::Rejected(HookRejectionInfo::new_long(
                "Path component too long",
                format!(
                    "Path component length for {:?} ({}) exceeds length limit (>= {})",
                    component,
                    component.len(),
                    MAX_PATH_COMPONENT_LIMIT,
                ),
            ))));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_path_bad() {
        let path = NonRootMPath::new("flib/intern/__generated__/GraphQLMeerkatStep/flib/intern/entschema/generated/entity/profile_plus/EntPlatformToolViewerContextCallsiteMigrationRuleAction.php/GQLG_Intern__PlatformToolViewerContextCallsiteMigrationRuleChangeRuleDescriptionResponsePayload__EntPlatformToolViewerContextCallsiteMigrationRuleAction__genPerformGraphQLPlatformToolViewerContextCallsiteMigrationRuleChangeRuleDescriptionMutationType.php").unwrap();
        assert!(check_path(&path).unwrap().is_some());
    }

    #[test]
    fn test_path_ok() {
        let path = NonRootMPath::new("flib/intern/__generated__/GraphQLFetchersMeerkatStep/ic/GQLG_File__EntIcxPositionSearchHitWorkdayPositionViewStateJunction__GraphQLFacebookInternalTypeSetFetcherWrapper.php").unwrap();
        assert!(check_path(&path).unwrap().is_none());
    }
}
