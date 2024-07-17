/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include <folly/init/Init.h>
#include <folly/logging/Init.h>
#include <folly/logging/LogConfigParser.h>
#include <folly/logging/xlog.h>
#include <folly/portability/GFlags.h>

#include "eden/common/telemetry/NullStructuredLogger.h"
#include "eden/common/utils/FaultInjector.h"
#include "eden/fs/model/Blob.h"
#include "eden/fs/model/Hash.h"
#include "eden/fs/store/RocksDbLocalStore.h"
#include "eden/fs/telemetry/EdenStats.h"

DEFINE_string(edenDir, "", "The path to the .eden directory");
DEFINE_string(blobID, "", "The blob ID");

constexpr folly::StringPiece kRocksDBPath{"storage/rocks-db"};

using namespace facebook::eden;
using folly::IOBuf;

/*
 * This tool rewrites a specific blob in Eden's local store with empty contents.
 * This is intended for use in integration tests that exercise the behavior
 * with bogus blob contents in the LocalStore.
 */
int main(int argc, char* argv[]) {
  folly::init(&argc, &argv);

  auto loggingConfig = folly::parseLogConfig("eden=DBG2");
  folly::LoggerDB::get().updateConfig(loggingConfig);

  if (FLAGS_edenDir.empty()) {
    fprintf(stderr, "error: the --edenDir argument is required\n");
    return 1;
  }

  ObjectId blobID(FLAGS_blobID);

  auto edenDir = facebook::eden::canonicalPath(FLAGS_edenDir);
  const auto rocksPath = edenDir + RelativePathPiece{kRocksDBPath};
  FaultInjector faultInjector(/*enabled=*/false);
  RocksDbLocalStore localStore(
      rocksPath,
      makeRefPtr<EdenStats>(),
      std::make_shared<NullStructuredLogger>(),
      &faultInjector);
  localStore.open();
  Blob blob{IOBuf()};
  localStore.putBlob(blobID, &blob);

  return 0;
}
