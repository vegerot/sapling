/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/utils/ProcessAccessLog.h"

#include <folly/Exception.h>
#include <folly/MapUtil.h>
#include <folly/MicroLock.h>
#include <folly/ThreadLocal.h>

#include "eden/common/utils/ProcessInfoCache.h"

namespace facebook::eden {

struct ThreadLocalBucket {
  explicit ThreadLocalBucket(ProcessAccessLog* processAccessLog)
      : state_{std::in_place, processAccessLog} {}

  ~ThreadLocalBucket() {
    // This thread is going away, so merge our data into the parent.
    mergeUpstream();
  }

  /**
   * Returns whether the pid was newly-recorded in this thread-second or not.
   */
  bool add(
      uint64_t secondsSinceStart,
      pid_t pid,
      ProcessAccessLog::AccessType type) {
    auto state = state_.lock();

    // isNewPid must be initialized because BucketedLog::add will not call
    // Bucket::add if secondsSinceStart is too old and the sample is dropped.
    // (In that case, it's unnecessary to record the process name.)
    bool isNewPid = false;
    state->buckets.add(secondsSinceStart, pid, isNewPid, type);
    return isNewPid;
  }

  bool add(
      uint64_t secondsSinceStart,
      pid_t pid,
      std::chrono::nanoseconds duration) {
    auto state = state_.lock();

    bool isNewPid = false;
    state->buckets.add(secondsSinceStart, pid, isNewPid, duration);
    return isNewPid;
  }

  void mergeUpstream() {
    auto state = state_.lock();
    if (!state->owner) {
      return;
    }
    state->owner->state_.withWLock(
        [&](auto& ownerState) { ownerState.buckets.merge(state->buckets); });
    state->buckets.clear();
  }

  void clearOwnerIfMe(ProcessAccessLog* owner) {
    auto state = state_.lock();
    if (state->owner == owner) {
      state->owner = nullptr;
    }
  }

 private:
  /**
   * Sadly, because getAllAccesses needs to access all of the buckets, it
   * needs a mechanism to stop writers for the duration of the read.
   *
   * Reading the data (merging up-stream from all of the threads) is
   * exceptionally rare, so this lock should largely stay uncontended. I
   * considered using folly::SpinLock, but the documentation strongly suggests
   * not. I am hoping that acquiring an uncontended MicroLock
   * boils down to a single CAS, even though lock xchg can be painful by itself.
   *
   * This lock must always be acquired before the owner's buckets lock.
   */
  struct State {
    explicit State(ProcessAccessLog* pal) : owner{pal} {}
    ProcessAccessLog::Buckets buckets;
    ProcessAccessLog* owner;
  };

  folly::Synchronized<State, folly::MicroLock> state_;
};

namespace {
struct BucketTag;
folly::ThreadLocalPtr<ThreadLocalBucket, BucketTag> threadLocalBucketPtr;
} // namespace

void ProcessAccessLog::Bucket::clear() {
  accessCountsByPid.clear();
}

void ProcessAccessLog::Bucket::add(
    pid_t pid,
    bool& isNewPid,
    ProcessAccessLog::AccessType type) {
  auto [it, contains] = accessCountsByPid.emplace(pid, PerBucketAccessCounts{});
  it->second[type]++;
  isNewPid = contains;
}

void ProcessAccessLog::Bucket::add(
    pid_t pid,
    bool& isNewPid,
    std::chrono::nanoseconds duration) {
  auto [it, contains] = accessCountsByPid.emplace(pid, PerBucketAccessCounts{});
  it->second.duration += duration;
  isNewPid = contains;
}

void ProcessAccessLog::Bucket::merge(const Bucket& other) {
  for (auto [pid, otherAccessCounts] : other.accessCountsByPid) {
    for (std::underlying_type_t<AccessType> type = 0;
         type != folly::to_underlying(AccessType::Last);
         type++) {
      accessCountsByPid[pid].counts[type] += otherAccessCounts.counts[type];
    }
    accessCountsByPid[pid].duration += otherAccessCounts.duration;
  }
}

ProcessAccessLog::ProcessAccessLog(
    std::shared_ptr<ProcessInfoCache> processInfoCache)
    : processInfoCache_{std::move(processInfoCache)} {
  XCHECK(processInfoCache_) << "Process info cache is mandatory";
}

ProcessAccessLog::~ProcessAccessLog() {
  for (auto& tlb : threadLocalBucketPtr.accessAllThreads()) {
    tlb.clearOwnerIfMe(this);
  }
}

ThreadLocalBucket* ProcessAccessLog::getTlb() {
  auto tlb = threadLocalBucketPtr.get();
  if (!tlb) {
    threadLocalBucketPtr.reset(std::make_unique<ThreadLocalBucket>(this));
    tlb = threadLocalBucketPtr.get();
  }
  return tlb;
}

uint64_t ProcessAccessLog::getSecondsSinceEpoch() {
  return std::chrono::duration_cast<std::chrono::seconds>(
             std::chrono::steady_clock::now().time_since_epoch())
      .count();
}

void ProcessAccessLog::recordAccess(
    pid_t pid,
    ProcessAccessLog::AccessType type) {
  // This function is called very frequently from different threads. It's a
  // write-often, read-rarely use case, so, to avoid synchronization overhead,
  // record to thread-local storage and only merge into the access log when the
  // calling thread dies or when the data must be read.
  bool isNewPid = getTlb()->add(getSecondsSinceEpoch(), pid, type);

  // Many processes are short-lived, so grab the executable name during the
  // access. We could potentially get away with grabbing executable names a
  // bit later on another thread, but we'll only readlink() once per pid.

  // Sometimes we receive requests from pid 0. Record the access,
  // but don't try to look up a name.
  if (pid != 0) {
    // Since recordAccess is called a lot by latency- and throughput-sensitive
    // code, only try to lookup and cache the process name if we haven't seen
    // it this thread-second.
    if (isNewPid) {
      // It's a bit unfortunate that ProcessInfoCache maintains its own
      // SharedMutex, but it will be shared with thrift counters.
      processInfoCache_->add(pid);
    }
  }
}

void ProcessAccessLog::recordDuration(
    pid_t pid,
    std::chrono::nanoseconds duration) {
  bool isNewPid = getTlb()->add(getSecondsSinceEpoch(), pid, duration);
  if (pid != 0 && isNewPid) {
    processInfoCache_->add(pid);
  }
}

std::unordered_map<pid_t, AccessCounts> ProcessAccessLog::getAccessCounts(
    std::chrono::seconds lastNSeconds) {
  auto secondCount = lastNSeconds.count();
  // First, merge all the thread-local buckets into their owners, including us.
  for (auto& tlb : threadLocalBucketPtr.accessAllThreads()) {
    // This must be done outside of acquiring our own state_ lock.
    tlb.mergeUpstream();
  }

  auto state = state_.wlock();
  auto allBuckets = state->buckets.getAll(getSecondsSinceEpoch());

  if (secondCount < 0) {
    return {};
  }

  Bucket bucket;
  uint64_t count = std::min(
      static_cast<uint64_t>(allBuckets.size()),
      static_cast<uint64_t>(secondCount));
  for (auto iter = allBuckets.end() - count; iter != allBuckets.end(); ++iter) {
    bucket.merge(*iter);
  }

  // Transfer to a Thrift map
  std::unordered_map<pid_t, AccessCounts> accessCountsByPid;
  for (auto& [pid, accessCounts] : bucket.accessCountsByPid) {
    accessCountsByPid[pid].fsChannelReads_ref() =
        accessCounts[AccessType::FsChannelRead];
    accessCountsByPid[pid].fsChannelWrites_ref() =
        accessCounts[AccessType::FsChannelWrite];
    accessCountsByPid[pid].fsChannelTotal_ref() =
        accessCounts[AccessType::FsChannelRead] +
        accessCounts[AccessType::FsChannelWrite] +
        accessCounts[AccessType::FsChannelOther];
    accessCountsByPid[pid].fsChannelMemoryCacheImports_ref() =
        accessCounts[AccessType::FsChannelMemoryCacheImport];
    accessCountsByPid[pid].fsChannelDiskCacheImports_ref() =
        accessCounts[AccessType::FsChannelDiskCacheImport];
    accessCountsByPid[pid].fsChannelBackingStoreImports_ref() =
        accessCounts[AccessType::FsChannelBackingStoreImport];
    accessCountsByPid[pid].fsChannelDurationNs_ref() =
        accessCounts.duration.count();
  }
  return accessCountsByPid;
}

} // namespace facebook::eden
