/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use mononoke_macros::mononoke;

use super::*;

static EMPTY: &[u8] = include_bytes!("empty.i.bin");

#[mononoke::test]
fn emptyrev() {
    let revlog = Revlog::new(EMPTY.to_vec(), None).expect("construction failed");
    let node = revlog
        .get_rev(RevIdx::from(0u32))
        .expect("failed to get rev");

    assert_eq!(node.size(), 0);
}
