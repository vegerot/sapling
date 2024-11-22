/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io;

use termwiz::caps::Capabilities;
use termwiz::render::terminfo::TerminfoRenderer;
use termwiz::render::RenderTty;
use termwiz::surface;
use termwiz::surface::Change;
use termwiz::terminal::Terminal;
use termwiz::Result;

use crate::IsTty;

pub(crate) const DEFAULT_TERM_WIDTH: usize = 80;
pub(crate) const DEFAULT_TERM_HEIGHT: usize = 25;

#[cfg(windows)]
mod windows_term;

#[cfg(unix)]
mod unix_term;

/// Term is a minimally skinny abstraction over termwiz::Terminal.
/// It makes it easy to swap in other things for testing.
pub(crate) trait Term {
    fn render(&mut self, changes: &[Change]) -> Result<()>;
    fn size(&mut self) -> Result<(usize, usize)>;

    /// Attempt to reset terminal back to initial state. This is not necessary if using
    /// termwiz::Terminal since termwiz::Terminal resets automatically when dropped.
    fn reset(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) trait ResettableTty: RenderTty {
    /// Attempt to reset tty back to initial state.
    fn reset(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<T: Terminal> Term for T {
    fn render(&mut self, changes: &[Change]) -> Result<()> {
        Terminal::render(self, changes)?;
        self.flush()?;
        Ok(())
    }

    fn size(&mut self) -> Result<(usize, usize)> {
        let size = self.get_screen_size()?;
        Ok((size.cols, size.rows))
    }
}

/// DumbTerm allows writing termwiz Changes to an arbitrary writer,
/// ignoring lack of ttyness and using a default terminal size.
pub(crate) struct DumbTerm<W: ResettableTty + io::Write> {
    tty: W,
    renderer: TerminfoRenderer,
    separator: Option<u8>,
}

impl<W: ResettableTty + io::Write> DumbTerm<W> {
    pub fn new(tty: W) -> Result<Self> {
        Ok(Self {
            tty,
            renderer: TerminfoRenderer::new(caps()?),
            separator: None,
        })
    }

    pub fn set_separator(&mut self, sep: u8) {
        self.separator = Some(sep);
    }
}

struct BufTty<'a> {
    w: &'a mut dyn io::Write,
    size: (usize, usize),
}

impl RenderTty for BufTty<'_> {
    fn get_size_in_cells(&mut self) -> termwiz::Result<(usize, usize)> {
        Ok(self.size)
    }
}

impl io::Write for BufTty<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.w.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

impl<W: ResettableTty + io::Write> Term for DumbTerm<W> {
    fn render(&mut self, changes: &[Change]) -> Result<()> {
        // Buffer the progress output so we can write it in a single `write_all()`,
        // minimizing flickering.
        let mut buf = Vec::new();
        self.renderer.render_to(
            changes,
            &mut BufTty {
                w: &mut buf,
                size: self.tty.get_size_in_cells()?,
            },
        )?;
        if let Some(sep) = self.separator {
            buf.push(sep);
        }

        self.tty.write_all(&buf)?;
        self.tty.flush()?;
        Ok(())
    }

    fn size(&mut self) -> Result<(usize, usize)> {
        self.tty.get_size_in_cells()
    }

    fn reset(&mut self) -> io::Result<()> {
        // Make sure cursor is visible.
        self.render(&[Change::CursorVisibility(surface::CursorVisibility::Visible)])
            .ok();

        self.tty.reset()
    }
}

pub(crate) struct DumbTty {
    write: Box<dyn io::Write + Send + Sync>,
}

impl DumbTty {
    pub fn new(write: Box<dyn io::Write + Send + Sync>) -> Self {
        Self { write }
    }
}

impl RenderTty for DumbTty {
    fn get_size_in_cells(&mut self) -> Result<(usize, usize)> {
        Ok((DEFAULT_TERM_WIDTH, DEFAULT_TERM_HEIGHT))
    }
}

impl ResettableTty for DumbTty {}

impl io::Write for DumbTty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write.flush()
    }
}

fn caps() -> Result<Capabilities> {
    let hints = termwiz::caps::ProbeHints::new_from_env().mouse_reporting(Some(false));
    termwiz::caps::Capabilities::new_with_hints(hints)
}

pub(crate) fn make_real_term() -> Result<Box<dyn Term + Send + Sync>> {
    // Don't use the real termwiz terminal yet because:
    //   1. On Windows, it disables automatic \n -> \r\n conversion.
    //   2. On Mac, we were detecting /dev/tty as usable, but ended up blocking when dropping the UnixTerminal object (when invoked via buck).
    //   3. Termwiz sets up a SIGWINCH handler which causes crash in Python crecord stuff.

    #[cfg(windows)]
    {
        let stderr = io::stderr();
        if stderr.is_tty() {
            let tty = windows_term::WindowsTty::new(Box::new(stderr));
            return Ok(Box::new(DumbTerm::new(tty)?));
        }
    }

    #[cfg(unix)]
    {
        let stderr = io::stderr();
        if stderr.is_tty() {
            let tty = unix_term::UnixTty::new(Box::new(stderr));
            return Ok(Box::new(DumbTerm::new(tty)?));
        }
    }

    termwiz::bail!("no suitable term output file");
}
