/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "folly/portability/Windows.h"

#include <ProjectedFSLib.h> // @manual

#include "eden/common/utils/ImmediateFuture.h"
#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/inodes/RequestContext.h"
#include "eden/fs/prjfs/PrjfsChannel.h"

namespace facebook::eden {

class PrjfsObjectFetchContext : public FsObjectFetchContext {
 public:
  PrjfsObjectFetchContext(ProcessId pid) : pid_{pid} {}

  OptionalProcessId getClientPid() const override {
    return pid_;
  }

 private:
  ProcessId pid_;
};

class PrjfsRequestContext : public RequestContext {
 public:
  PrjfsRequestContext(const PrjfsRequestContext&) = delete;
  PrjfsRequestContext& operator=(const PrjfsRequestContext&) = delete;
  PrjfsRequestContext(PrjfsRequestContext&&) = delete;
  PrjfsRequestContext& operator=(PrjfsRequestContext&&) = delete;

  explicit PrjfsRequestContext(
      folly::ReadMostlySharedPtr<PrjfsChannelInner> channel,
      const PRJ_CALLBACK_DATA& prjfsData)
      : RequestContext(
            channel->getProcessAccessLog(),
            makeRefPtr<PrjfsObjectFetchContext>(
                ProcessId{prjfsData.TriggeringProcessId})),
        channel_(std::move(channel)),
        commandId_(prjfsData.CommandId) {}

  folly::ReadMostlyWeakPtr<PrjfsChannelInner> getChannelForAsyncUse() {
    return folly::ReadMostlyWeakPtr<PrjfsChannelInner>{channel_};
  }

  ImmediateFuture<folly::Unit> catchErrors(
      ImmediateFuture<folly::Unit>&& fut,
      EdenStatsPtr stats,
      StatsGroupBase::Counter PrjfsStats::*countSuccessful,
      StatsGroupBase::Counter PrjfsStats::*countFailure) {
    return std::move(fut).thenTry(
        [this, stats = std::move(stats), countSuccessful, countFailure](
            folly::Try<folly::Unit>&& try_) {
          auto result = tryToHResult(try_);
          if (result != S_OK) {
            if (stats && countFailure) {
              stats->increment(countFailure);
            }
            sendError(result);
          } else {
            if (stats && countSuccessful) {
              stats->increment(countSuccessful);
            }
          }
        });
  }

  void sendSuccess() const {
    return channel_->sendSuccess(commandId_, nullptr);
  }

  void sendNotificationSuccess() const {
    PRJ_COMPLETE_COMMAND_EXTENDED_PARAMETERS extra{};
    extra.CommandType = PRJ_COMPLETE_COMMAND_TYPE_NOTIFICATION;
    return channel_->sendSuccess(commandId_, &extra);
  }

  void sendEnumerationSuccess(PRJ_DIR_ENTRY_BUFFER_HANDLE buffer) const {
    PRJ_COMPLETE_COMMAND_EXTENDED_PARAMETERS extra{};
    extra.CommandType = PRJ_COMPLETE_COMMAND_TYPE_ENUMERATION;
    extra.Enumeration.DirEntryBufferHandle = buffer;
    return channel_->sendSuccess(commandId_, &extra);
  }

  void sendError(HRESULT result) const {
    return channel_->sendError(commandId_, result);
  }

 private:
  folly::ReadMostlySharedPtr<PrjfsChannelInner> channel_;
  int32_t commandId_;
};

} // namespace facebook::eden
