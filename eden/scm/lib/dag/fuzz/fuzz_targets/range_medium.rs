/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![no_main]

use bindag::TestContext;
use lazy_static::lazy_static;
use libfuzzer_sys::fuzz_target;

mod tests;

lazy_static! {
    // Pick a subset of the DAG (with many merges).
    static ref CONTEXT: TestContext = TestContext::from_bin_sliced(bindag::MOZILLA, 56666..60415);
}

fuzz_target!(|input: (Vec<u16>, Vec<u16>)| {
    let roots = CONTEXT.clamp_revs(&input.0);
    let heads = CONTEXT.clamp_revs(&input.1);
    tests::test_range(&CONTEXT, roots, heads);
});
