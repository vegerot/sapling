/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io::stderr;
use std::io::stdin;
use std::io::stdout;
use std::io::Cursor;
use std::io::IsTerminal;
use std::sync::Weak;

use crate::IsTty;

impl IsTty for std::io::Empty {
    fn is_tty(&self) -> bool {
        false
    }
}

impl IsTty for std::io::Stdin {
    fn is_tty(&self) -> bool {
        stdin().is_terminal()
    }
    fn is_stdin(&self) -> bool {
        true
    }
}

impl IsTty for std::io::Stdout {
    fn is_tty(&self) -> bool {
        stdout().is_terminal()
    }
    fn is_stdout(&self) -> bool {
        true
    }
}

impl IsTty for std::io::Stderr {
    fn is_tty(&self) -> bool {
        stderr().is_terminal()
    }
    fn is_stderr(&self) -> bool {
        true
    }
}

impl IsTty for Vec<u8> {
    fn is_tty(&self) -> bool {
        false
    }
}

impl<'a> IsTty for &'a [u8] {
    fn is_tty(&self) -> bool {
        false
    }
}

impl<T> IsTty for Cursor<T> {
    fn is_tty(&self) -> bool {
        false
    }
}

impl IsTty for crate::IOInput {
    fn is_tty(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.input.is_tty()
    }
    fn is_stdin(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.input.is_stdin()
    }
    fn pager_active(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.input.pager_active()
    }
}

impl IsTty for crate::IOOutput {
    fn is_tty(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.output.is_tty()
    }
    fn is_stdout(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.output.is_stdout()
    }
    fn pager_active(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        inner.output.pager_active()
    }
}

impl IsTty for crate::IOError {
    fn is_tty(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        if let Some(error) = inner.error.as_ref() {
            error.is_tty()
        } else {
            false
        }
    }
    fn is_stderr(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        if let Some(error) = inner.error.as_ref() {
            error.is_stderr()
        } else {
            false
        }
    }
    fn pager_active(&self) -> bool {
        let inner = match Weak::upgrade(&self.0) {
            Some(inner) => inner,
            None => return false,
        };
        let inner = inner.io_state.lock();
        if let Some(error) = inner.error.as_ref() {
            error.pager_active()
        } else {
            false
        }
    }
}

pub(crate) struct WriterWithTty {
    inner: Box<dyn std::io::Write + Sync + Send>,
    pretend_tty: bool,
    pub(crate) pretend_stdout: bool,
}

impl std::io::Write for WriterWithTty {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl IsTty for WriterWithTty {
    fn is_tty(&self) -> bool {
        self.pretend_tty
    }
    fn is_stdout(&self) -> bool {
        self.pretend_stdout
    }
    fn pager_active(&self) -> bool {
        true
    }
}

impl WriterWithTty {
    pub fn new(inner: Box<dyn std::io::Write + Sync + Send>, pretend_tty: bool) -> Self {
        Self {
            inner,
            pretend_tty,
            pretend_stdout: false,
        }
    }
}
