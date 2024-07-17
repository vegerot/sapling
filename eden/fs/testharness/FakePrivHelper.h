/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <folly/futures/Future.h>
#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/privhelper/PrivHelper.h"

#include <memory>
#include <string>
#include <unordered_map>

namespace facebook::eden {

class FakeFuse;

/**
 * FakePrivHelper implements the PrivHelper API, but returns FakeFuse
 * connections rather than performing actual FUSE mounts to the kernel.
 *
 * This allows test code to directly control the FUSE messages sent to an
 * EdenMount.
 */
class FakePrivHelper final : public PrivHelper {
 public:
  FakePrivHelper() = default;

  class MountDelegate {
   public:
    virtual ~MountDelegate();

    virtual folly::Future<folly::File> fuseMount() = 0;
    virtual folly::Future<folly::Unit> fuseUnmount() = 0;
  };

  void registerMount(
      AbsolutePathPiece mountPath,
      std::shared_ptr<FakeFuse> fuse);

  void registerMountDelegate(
      AbsolutePathPiece mountPath,
      std::shared_ptr<MountDelegate>);

  // PrivHelper functions
  void attachEventBase(folly::EventBase* eventBase) override;
  void detachEventBase() override;
  folly::Future<folly::File> fuseMount(
      folly::StringPiece mountPath,
      bool readOnly,
      std::optional<folly::StringPiece> vfsType) override;
  folly::Future<folly::Unit> nfsMount(
      folly::StringPiece mountPath,
      folly::SocketAddress mountdAddr,
      folly::SocketAddress nfsdAddr,
      bool readOnly,
      uint32_t iosize,
      bool useReaddirplus) override;
  folly::Future<folly::Unit> fuseUnmount(folly::StringPiece mountPath) override;
  folly::Future<folly::Unit> nfsUnmount(folly::StringPiece mountPath) override;
  folly::Future<folly::Unit> bindMount(
      folly::StringPiece clientPath,
      folly::StringPiece mountPath) override;
  folly::Future<folly::Unit> bindUnMount(folly::StringPiece mountPath) override;
  folly::Future<folly::Unit> takeoverShutdown(
      folly::StringPiece mountPath) override;
  folly::Future<folly::Unit> takeoverStartup(
      folly::StringPiece mountPath,
      const std::vector<std::string>& bindMounts) override;
  folly::Future<folly::Unit> setLogFile(folly::File logFile) override;
  folly::Future<folly::Unit> setDaemonTimeout(
      std::chrono::nanoseconds duration) override;
  folly::Future<folly::Unit> setUseEdenFs(bool useEdenFs) override;
  int stop() override;
  int getRawClientFd() const override {
    return -1;
  }
  bool checkConnection() override {
    return true;
  }

 private:
  FakePrivHelper(FakePrivHelper const&) = delete;
  FakePrivHelper& operator=(FakePrivHelper const&) = delete;

  std::shared_ptr<MountDelegate> getMountDelegate(folly::StringPiece mountPath);

  std::unordered_map<std::string, std::shared_ptr<MountDelegate>>
      mountDelegates_;
};

class FakeFuseMountDelegate : public FakePrivHelper::MountDelegate {
 public:
  explicit FakeFuseMountDelegate(
      AbsolutePath mountPath,
      std::shared_ptr<FakeFuse>) noexcept;

  folly::Future<folly::File> fuseMount() override;
  folly::Future<folly::Unit> fuseUnmount() override;

  FOLLY_NODISCARD bool wasFuseUnmountEverCalled() const noexcept;

 private:
  AbsolutePath mountPath_;
  std::shared_ptr<FakeFuse> fuse_;
  bool wasFuseUnmountEverCalled_{false};
};

} // namespace facebook::eden
