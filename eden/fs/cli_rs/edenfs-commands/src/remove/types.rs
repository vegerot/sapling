/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use crossterm::style::Stylize;
use dialoguer::Confirm;
use io::IO;
use termlogger::TermLogger;

use super::operations;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum PathType {
    ActiveEdenMount,
    InactiveEdenMount,
    RegularFile,
    Unknown,
}

impl PathType {
    pub fn get_prompt(&self, paths: Vec<&str>) -> String {
        let prompt_str = match self {
            PathType::ActiveEdenMount | PathType::InactiveEdenMount => format!(
                "Warning: this operation will permanently delete the following EdenFS checkouts:\n\
         \n\
         {}\n\
         \n\
         Any uncommitted changes and shelves in this checkout will be lost forever.\n",
                paths.join("\n")
            ),

            PathType::RegularFile => format!(
                "Warning: this operation will permanently delete the following files:\n\
        \n\
        {}\n\
        \n\
        After deletion, they will be lost forever.\n",
                paths.join("\n")
            ),

            PathType::Unknown => format!(
                "Warning: the following paths are directories not managed by EdenFS:\n\
        \n\
        {}\n\
        \n\
                Any files in them will be lost forever. \n",
                paths.join("\n")
            ),
        };
        prompt_str.yellow().to_string()
    }

    pub async fn remove(&self, context: &RemoveContext) -> Result<()> {
        match self {
            PathType::ActiveEdenMount => operations::remove_active_eden_mount(context).await,
            PathType::InactiveEdenMount => operations::remove_inactive_eden_mount(context).await,
            PathType::RegularFile => {
                fs::remove_file(context.canonical_path.as_path()).map_err(Into::into)
            }
            PathType::Unknown => operations::clean_up(context).await,
        }
    }
}

pub struct RemoveContext {
    pub original_path: String,
    pub canonical_path: PathBuf,
    pub path_type: PathType,
    pub preserve_mount_point: bool,
    pub io: Arc<Messenger>,
}

impl RemoveContext {
    pub fn new(
        original_path: String,
        canonical_path: PathBuf,
        path_type: PathType,
        preserve_mount_point: bool,
        io: Arc<Messenger>,
    ) -> RemoveContext {
        RemoveContext {
            original_path,
            canonical_path,
            path_type,
            preserve_mount_point,
            io,
        }
    }
}

impl fmt::Display for RemoveContext {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.canonical_path.display())
    }
}

// Object responsible to print messages to stdout or generate prompt
// for the user and receive response
pub struct Messenger {
    logger: TermLogger,
    skip_prompt: bool,
    answer_no: bool,
}

impl Messenger {
    pub fn new(io: IO, skip_prompt: bool, suppress_output: bool, answer_no: bool) -> Messenger {
        Messenger {
            logger: TermLogger::new(&io).with_quiet(suppress_output),
            skip_prompt,
            answer_no,
        }
    }

    pub fn new_stdio(skip_prompt: bool, suppress_output: bool, answer_no: bool) -> Messenger {
        Messenger::new(IO::stdio(), skip_prompt, suppress_output, answer_no)
    }

    pub fn info(&self, msg: String) {
        self.logger.info(msg);
    }

    pub fn warn(&self, msg: String) {
        self.logger.warn(msg.yellow().to_string());
    }

    #[allow(dead_code)]
    pub fn error(&self, msg: String) {
        self.logger.warn(msg.red().to_string());
    }

    pub fn success(&self, msg: String) {
        self.logger.info(msg.green().to_string());
    }

    pub fn done(&self) {
        self.success("✓".to_string());
    }

    pub fn prompt_user(&self, prompt: String) -> Result<bool> {
        if self.answer_no {
            return Ok(false);
        }

        if !self.skip_prompt {
            self.logger.info(prompt);
            let res = Confirm::new().with_prompt("Proceed?").interact()?;
            return Ok(res);
        }
        Ok(true)
    }
}
