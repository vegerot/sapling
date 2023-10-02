/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use context::CoreContext;
use metaconfig_types::HookConfig;
use mononoke_types::BasicFileChange;
use mononoke_types::NonRootMPath;
use regex::Regex;

use crate::CrossRepoPushSource;
use crate::FileHook;
use crate::HookExecution;
use crate::HookFileContentProvider;
use crate::HookRejectionInfo;
use crate::PushAuthoredBy;

#[derive(Default)]
pub struct NoQuestionableFilenamesBuilder<'a> {
    allowlist_for_braces: Option<&'a str>,
    allowlist_for_cmd_line: Option<&'a str>,
}

impl<'a> NoQuestionableFilenamesBuilder<'a> {
    pub fn set_from_config(mut self, config: &'a HookConfig) -> Self {
        if let Some(v) = config.strings.get("allowlist_for_braces") {
            self.allowlist_for_braces = Some(v);
        }
        if let Some(v) = config.strings.get("allowlist_for_cmd_line") {
            self.allowlist_for_cmd_line = Some(v);
        }
        self
    }

    pub fn build(self) -> Result<NoQuestionableFilenames> {
        Ok(NoQuestionableFilenames {
            allowlist_for_braces: self
                .allowlist_for_braces
                .map(Regex::new)
                .transpose()
                .context("Failed to create allowlist regex for braces")?,
            braces: Regex::new(r"[{}]")?,
            allowlist_for_cmd_line: self
                .allowlist_for_cmd_line
                .map(Regex::new)
                .transpose()
                .context("Failed to create allowlist regex for cmd_line")?,
            // Disallow spaces, apostrophes, and files that start with hyphens
            cmd_line: Regex::new(r"\s|'|(^|/)-")?,
        })
    }
}

pub struct NoQuestionableFilenames {
    allowlist_for_braces: Option<Regex>,
    braces: Regex,
    allowlist_for_cmd_line: Option<Regex>,
    cmd_line: Regex,
}

impl NoQuestionableFilenames {
    pub fn builder<'a>() -> NoQuestionableFilenamesBuilder<'a> {
        NoQuestionableFilenamesBuilder::default()
    }
}

#[async_trait]
impl FileHook for NoQuestionableFilenames {
    async fn run<'this: 'change, 'ctx: 'this, 'change, 'fetcher: 'change, 'path: 'change>(
        &'this self,
        _ctx: &'ctx CoreContext,
        _content_manager: &'fetcher dyn HookFileContentProvider,
        change: Option<&'change BasicFileChange>,
        path: &'path NonRootMPath,
        cross_repo_push_source: CrossRepoPushSource,
        push_authored_by: PushAuthoredBy,
    ) -> Result<HookExecution> {
        if push_authored_by.service() {
            return Ok(HookExecution::Accepted);
        }
        if cross_repo_push_source == CrossRepoPushSource::PushRedirected {
            // For push-redirected pushes we rely on the hook
            // running in the original repo
            return Ok(HookExecution::Accepted);
        }
        if change.is_none() {
            return Ok(HookExecution::Accepted);
        }

        let path = format!("{}", path);
        if self.braces.is_match(&path) {
            match self.allowlist_for_braces {
                Some(ref allow) if allow.is_match(&path) => {}
                _ => {
                    return Ok(HookExecution::Rejected(HookRejectionInfo::new_long(
                        "Illegal filename",
                        format!(
                            "ABORT: Illegal filename: {}. The file name cannot include brace(s).",
                            path
                        ),
                    )));
                }
            }
        }

        if self.cmd_line.is_match(&path) {
            match self.allowlist_for_cmd_line {
                Some(ref allow) if allow.is_match(&path) => {}
                _ => {
                    return Ok(HookExecution::Rejected(HookRejectionInfo::new_long(
                        "Illegal filename",
                        format!(
                            "ABORT: Illegal filename: {}. The file name cannot include spaces, apostrophes or start with hyphens.",
                            path
                        ),
                    )));
                }
            }
        }

        Ok(HookExecution::Accepted)
    }
}
