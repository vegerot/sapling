/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![no_main]

use bindag::octopus;
use bindag::OctopusTestContext;
use lazy_static::lazy_static;
use libfuzzer_sys::fuzz_target;

mod tests;

lazy_static! {
    static ref CONTEXT: OctopusTestContext =
        OctopusTestContext::from_parents(octopus::cross_octopus());
}

fuzz_target!(|input: Vec<u8>| {
    let revs = CONTEXT.clamp_revs(&input[..input.len().min(5)]);
    tests::test_gca(&CONTEXT, revs);
});
