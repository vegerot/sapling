/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/common/telemetry/NullStructuredLogger.h"
#include "eden/common/testharness/TempFile.h"
#include "eden/common/utils/FaultInjector.h"
#include "eden/common/utils/benchharness/Bench.h"
#include "eden/fs/model/BlobMetadata.h"
#include "eden/fs/store/RocksDbLocalStore.h"
#include "eden/fs/telemetry/EdenStats.h"

namespace {
using namespace facebook::eden;

void getBlobMetadata(benchmark::State& st) {
  auto tempDir = makeTempDir();
  FaultInjector faultInjector{false};
  auto edenStats = makeRefPtr<EdenStats>();
  auto store = std::make_unique<RocksDbLocalStore>(
      canonicalPath(tempDir.path().string()),
      edenStats.copy(),
      std::make_shared<NullStructuredLogger>(),
      &faultInjector);
  store->open();

  const size_t N = 1'000'000;

  std::vector<ObjectId> ids;
  ids.reserve(N);
  for (size_t i = 0; i < N; ++i) {
    ids.emplace_back(fmt::format("{:08x}", i));
  }

  std::vector<BlobMetadata> metadata;
  metadata.reserve(N);
  for (size_t i = 0; i < N; ++i) {
    metadata.push_back(BlobMetadata{Hash20{}, std::nullopt, i});
  }

  for (size_t i = 0; i < N; ++i) {
    store->putBlobMetadata(ids[i], metadata[i]);
  }

  // Reopen the database to exercise the read-from-disk path.
  store.reset();
  store = std::make_unique<RocksDbLocalStore>(
      canonicalPath(tempDir.path().string()),
      edenStats.copy(),
      std::make_shared<NullStructuredLogger>(),
      &faultInjector);
  store->open();

  size_t i = 0;
  for (auto _ : st) {
    benchmark::DoNotOptimize(store->getBlobMetadata(ids[i]).get());
    if (++i == N) {
      i = 0;
    }
  }
}
BENCHMARK(getBlobMetadata);

} // namespace

EDEN_BENCHMARK_MAIN();
