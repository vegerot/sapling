/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <folly/Function.h>
#include <folly/Synchronized.h>
#include <folly/synchronization/LifoSem.h>
#include <gtest/gtest_prod.h>
#include <condition_variable>
#include <memory>
#include <optional>
#include <vector>

#include "eden/fs/inodes/overlay/gen-cpp2/overlay_types.h"
#include "eden/fs/inodes/sqlitecatalog/SqliteInodeCatalog.h"

namespace facebook::eden {

struct InodeNumber;
class EdenConfig;
class StructuredLogger;

class BufferedSqliteInodeCatalog : public SqliteInodeCatalog {
 public:
  explicit BufferedSqliteInodeCatalog(
      AbsolutePathPiece path,
      std::shared_ptr<StructuredLogger> logger,
      const EdenConfig& config,
      SqliteTreeStore::SynchronousMode mode =
          SqliteTreeStore::SynchronousMode::Normal);

  explicit BufferedSqliteInodeCatalog(
      std::unique_ptr<SqliteDatabase> store,
      const EdenConfig& config);

  ~BufferedSqliteInodeCatalog() override;

  BufferedSqliteInodeCatalog(const BufferedSqliteInodeCatalog&) = delete;
  BufferedSqliteInodeCatalog& operator=(const BufferedSqliteInodeCatalog&) =
      delete;

  BufferedSqliteInodeCatalog(BufferedSqliteInodeCatalog&&) = delete;
  BufferedSqliteInodeCatalog& operator=(BufferedSqliteInodeCatalog&&) = delete;

  /**
   * TODO: Implement semantic operations. Support was removed to easily allow
   * serving reads from the inflight work queue, but it would be worth
   * exploring semantic operations support. Semantic operations support allows
   * us to make operations like `rm -rf` on large directories no longer
   * quadratic.
   */
  bool supportsSemanticOperations() const override {
    return false;
  }

  void close(std::optional<InodeNumber> inodeNumber) override;

  std::optional<overlay::OverlayDir> loadOverlayDir(
      InodeNumber inodeNumber) override;

  std::optional<overlay::OverlayDir> loadAndRemoveOverlayDir(
      InodeNumber inodeNumber) override;

  void saveOverlayDir(InodeNumber inodeNumber, overlay::OverlayDir&& odir)
      override;

  void removeOverlayDir(InodeNumber inodeNumber) override;

  bool hasOverlayDir(InodeNumber inodeNumber) override;

 private:
  FRIEND_TEST(RawSqliteInodeCatalogTest, manual_recursive_delete);
  friend class DebugDumpSqliteInodeCatalogInodesTest;
  enum class OperationType {
    Write,
    Remove,
  };

  /**
   * Structure wrapping work waiting to be processed. odir will be std::nullopt
   * except when the creator was saveOverlayDir
   */
  struct Work {
    explicit Work(
        folly::Function<bool()> operation,
        std::optional<overlay::OverlayDir> odir,
        size_t estimateIndirectMemoryUsage)
        : operation(std::move(operation)),
          odir(std::move(odir)),
          estimateIndirectMemoryUsage(estimateIndirectMemoryUsage) {}
    folly::Function<bool()> operation;
    std::optional<overlay::OverlayDir> odir;
    size_t estimateIndirectMemoryUsage;
  };

  /**
   * Passive storage to inflight work, used to map a write or remove to the
   * corresponding payload
   */
  struct Operation {
    OperationType operationType;
    // Holding a raw pointer is safe because objects are never
    // deallocated without holding the State lock.
    Work* work;
  };

  struct State {
    bool workerThreadStopRequested = false;
    // map of InodeNumber to a (most recent operation, outstanding operation
    // payload) pair. waitingOperation represents work that is in the
    // `state_.work` vector. inflightOperation represents work that is currently
    // being processed by the worker thread (is on the thread local `work`
    // vector).
    std::unordered_map<InodeNumber, Operation> waitingOperation;
    std::unordered_map<InodeNumber, Operation> inflightOperation;
    std::vector<std::unique_ptr<Work>> work;
    size_t totalSize = 0;
  };

  // Maximum size of the buffer in bytes
  const size_t bufferSize_;
  std::thread workerThread_;
  folly::Synchronized<State, std::mutex> state_;
  // Encodes the condition !state_.work.empty()
  std::condition_variable workCV_;
  // Encodes the condition state_->totalSize < bufferSize_ ||
  // state_->workerThreadStopRequested
  std::condition_variable fullCV_;

  /**
   * Puts an folly::Function on a worker thread to be processed asynchronously.
   * The function should return a bool indicating whether or not the worker
   * thread should stop.
   */
  void process(
      folly::Function<bool()> fn,
      size_t captureSize,
      InodeNumber operationKey,
      OperationType operationType,
      std::optional<overlay::OverlayDir>&& odir = std::nullopt);

  /**
   * Uses the workerThread_ to process writes to the SqliteInodeCatalog
   */
  void processOnWorkerThread();

  void stopWorkerThread();

  /**
   * For testing purposes only. This function returns only once all writes prior
   * to the calling of this function have been processed.
   */
  void flush();

  /**
   * For testing purposes only. This function inserts an unfulfilled promise in
   * order to "pause" the worker thread so we can control data flow to test
   * different read/write scenarios. The caller must pass in an unfilled future
   * and is responsible for fulfilling the corresponding promise to unblock the
   * queue.
   */
  void pause(folly::Future<folly::Unit>&& fut);
};
} // namespace facebook::eden
