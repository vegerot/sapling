/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <folly/CancellationToken.h>
#include <folly/Range.h>

#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/store/StatsFetchContext.h"

namespace facebook::eden {

template <typename T>
class ImmediateFuture;
class DiffCallback;
class GitIgnoreStack;
class ObjectStore;
class UserInfo;
class TopLevelIgnores;
class EdenMount;

/**
 * A helper class to store parameters for a TreeInode::diff() operation.
 *
 * These parameters remain fixed across all subdirectories being diffed.
 * Primarily intent is to compound related diff attributes.
 *
 * The DiffContext must be alive for the duration of the async operation it is
 * used in.
 */
class DiffContext {
 public:
  DiffContext(
      DiffCallback* cb,
      folly::CancellationToken cancellation,
      const ObjectFetchContextPtr& fetchContext,
      bool listIgnored,
      CaseSensitivity caseSensitive,
      bool windowsSymlinksEnabled,
      std::shared_ptr<ObjectStore> os,
      std::unique_ptr<TopLevelIgnores> topLevelIgnores);

  DiffContext(const DiffContext&) = delete;
  DiffContext& operator=(const DiffContext&) = delete;
  DiffContext(DiffContext&&) = delete;
  DiffContext& operator=(DiffContext&&) = delete;
  ~DiffContext();

  DiffCallback* const callback;
  std::shared_ptr<ObjectStore> store;
  /**
   * If listIgnored is true information about ignored files will be reported.
   * If listIgnored is false then ignoredFile() will never be called on the
   * callback.  The diff operation may be faster with listIgnored=false, since
   * it can completely omit processing ignored subdirectories.
   */
  bool const listIgnored;

  const GitIgnoreStack* getToplevelIgnore() const;
  bool isCancelled() const;

  const StatsFetchContext& getStatsContext() {
    return *statsContext_;
  }

  const ObjectFetchContextPtr& getFetchContext() {
    return fetchContext_;
  }

  /** Whether this repository is mounted in case-sensitive mode */
  CaseSensitivity getCaseSensitive() const {
    return caseSensitive_;
  }

  // Whether symlinks are enabled or not
  bool getWindowsSymlinksEnabled() const {
    return windowsSymlinksEnabled_;
  }

 private:
  std::unique_ptr<TopLevelIgnores> topLevelIgnores_;
  const folly::CancellationToken cancellation_;

  StatsFetchContextPtr statsContext_;

  // This redundant, upcasted RefPtr exists to avoid needing to bump the
  // reference count on every fetch.
  ObjectFetchContextPtr fetchContext_;

  // Controls the case sensitivity of the diff operation.
  CaseSensitivity caseSensitive_;

  bool windowsSymlinksEnabled_;
};

} // namespace facebook::eden
