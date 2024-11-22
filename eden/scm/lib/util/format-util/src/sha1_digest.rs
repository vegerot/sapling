/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io;

use sha1::Digest;
use sha1::Sha1;
use types::Id20;

#[derive(Default)]
pub(crate) struct Sha1Write(Sha1);

impl io::Write for Sha1Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Into<Id20> for Sha1Write {
    fn into(self) -> Id20 {
        Id20::from_byte_array(self.0.finalize().into())
    }
}
