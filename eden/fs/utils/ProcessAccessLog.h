/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <folly/Synchronized.h>
#include <type_traits>

#include "eden/fs/service/gen-cpp2/eden_types.h"
#include "eden/fs/utils/BucketedLog.h"
#include "eden/fs/utils/EnumValue.h"

namespace facebook::eden {

class ProcessInfoCache;
struct ThreadLocalBucket;

/**
 * An inexpensive mechanism for counting accesses by pids. Intended for counting
 * channel and Thrift calls from external processes.
 *
 * The first time a thread calls recordAccess, that thread is then coupled to
 * this particular ProcessAccessLog, even if it calls recordAccess on another
 * ProcessAccessLog instance. Thus, use one ProcessAccessLog per pool of
 * threads.
 */
class ProcessAccessLog {
 public:
  enum class AccessType : unsigned char {
    FsChannelRead,
    FsChannelWrite,
    FsChannelOther,
    FsChannelMemoryCacheImport,
    FsChannelDiskCacheImport,
    FsChannelBackingStoreImport,
    Last,
  };

  explicit ProcessAccessLog(std::shared_ptr<ProcessInfoCache> processInfoCache);
  ~ProcessAccessLog();

  /**
   * Records an access by a process ID. The first call to recordAccess by a
   * particular thread binds that thread to this access log. Future recordAccess
   * calls on that thread will accumulate within this access log.
   *
   * Process IDs passed to recordAccess are also inserted into the
   * ProcessInfoCache.
   */
  void recordAccess(pid_t pid, AccessType type);
  void recordDuration(pid_t pid, std::chrono::nanoseconds duration);

  /**
   * Returns the number of times each pid was passed to recordAccess() in
   * `lastNSeconds`.
   *
   * Note: ProcessAccessLog buckets by whole seconds, so this number should be
   * considered an approximation.
   */
  std::unordered_map<pid_t, AccessCounts> getAccessCounts(
      std::chrono::seconds lastNSeconds);

 private:
  struct PerBucketAccessCounts {
    size_t counts[enumValue(AccessType::Last)];
    std::chrono::nanoseconds duration;

    size_t& operator[](AccessType type) {
      static_assert(std::is_unsigned_v<std::underlying_type_t<AccessType>>);
      auto idx = enumValue(type);
      XCHECK_LT(idx, enumValue(AccessType::Last));
      return counts[idx];
    }
  };

  // Data for one second.
  struct Bucket {
    void clear();
    void add(pid_t pid, bool& isNew, AccessType type);
    void add(pid_t pid, bool& isNew, std::chrono::nanoseconds duration);
    void merge(const Bucket& other);

    std::unordered_map<pid_t, PerBucketAccessCounts> accessCountsByPid;
  };

  // Keep up to ten seconds of data, but use a power of two so BucketedLog
  // generates smaller, faster code.
  static constexpr uint64_t kBucketCount = 16;
  using Buckets = BucketedLog<Bucket, kBucketCount>;

  struct State {
    Buckets buckets;
  };

  const std::shared_ptr<ProcessInfoCache> processInfoCache_;
  folly::Synchronized<State> state_;

  uint64_t getSecondsSinceEpoch();
  ThreadLocalBucket* getTlb();

  friend struct ThreadLocalBucket;
};

} // namespace facebook::eden
