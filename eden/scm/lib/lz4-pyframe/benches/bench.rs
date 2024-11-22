/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use lz4_pyframe::compress;
use lz4_pyframe::decompress;
use minibench::bench;
use minibench::elapsed;
use rand_core::RngCore;
use rand_core::SeedableRng;

fn main() {
    let mut rng = rand_chacha::ChaChaRng::seed_from_u64(0);
    let mut buf = vec![0u8; 100_000000];
    rng.fill_bytes(&mut buf);
    let compressed = compress(&buf).unwrap();

    bench("compress (100M)", || {
        elapsed(|| {
            compress(&buf).unwrap();
        })
    });

    bench("decompress (~100M)", || {
        elapsed(|| {
            decompress(&compressed).unwrap();
        })
    });
}
