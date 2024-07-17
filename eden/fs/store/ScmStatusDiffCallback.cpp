/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/store/ScmStatusDiffCallback.h"
#include <folly/Synchronized.h>
#include <folly/futures/Future.h>
#include <folly/logging/xlog.h>
#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/model/Tree.h"
#include "eden/fs/model/TreeEntry.h"
#include "eden/fs/store/ObjectStore.h"

namespace facebook::eden {

void ScmStatusDiffCallback::ignoredPath(RelativePathPiece path, dtype_t type) {
  if (type != dtype_t::Dir) {
    data_.wlock()->entries_ref()->emplace(
        path.asString(), ScmFileStatus::IGNORED);
  }
}

void ScmStatusDiffCallback::addedPath(RelativePathPiece path, dtype_t type) {
  if (type != dtype_t::Dir) {
    data_.wlock()->entries_ref()->emplace(
        path.asString(), ScmFileStatus::ADDED);
  }
}

void ScmStatusDiffCallback::removedPath(RelativePathPiece path, dtype_t type) {
  if (type != dtype_t::Dir) {
    data_.wlock()->entries_ref()->emplace(
        path.asString(), ScmFileStatus::REMOVED);
  }
}

void ScmStatusDiffCallback::modifiedPath(RelativePathPiece path, dtype_t type) {
  if (type != dtype_t::Dir) {
    data_.wlock()->entries_ref()->emplace(
        path.asString(), ScmFileStatus::MODIFIED);
  }
}

void ScmStatusDiffCallback::diffError(
    RelativePathPiece path,
    const folly::exception_wrapper& ew) {
  XLOG(WARNING) << "error computing status data for " << path << ": "
                << folly::exceptionStr(ew);
  data_.wlock()->errors_ref()->emplace(
      path.asString(), folly::exceptionStr(ew).toStdString());
}

/**
 * Extract the ScmStatus object from this callback.
 *
 * This method should be called no more than once, as this destructively
 * moves the results out of the callback.  It should only be invoked after
 * the diff operation has completed.
 */
ScmStatus ScmStatusDiffCallback::extractStatus() {
  auto data = data_.wlock();
  return std::move(*data);
}

ScmStatus ScmStatusDiffCallback::peekStatus() const {
  return data_.copy();
}

void ScmStatusDiffCallback::setStatus(ScmStatus status) {
  data_ = std::move(status);
}

char scmStatusCodeChar(ScmFileStatus code) {
  switch (code) {
    case ScmFileStatus::ADDED:
      return 'A';
    case ScmFileStatus::MODIFIED:
      return 'M';
    case ScmFileStatus::REMOVED:
      return 'R';
    case ScmFileStatus::IGNORED:
      return 'I';
  }
  throw_<std::runtime_error>(
      "Unrecognized ScmFileStatus: ",
      static_cast<typename std::underlying_type<ScmFileStatus>::type>(code));
}

std::ostream& operator<<(std::ostream& os, const ScmStatus& status) {
  os << "{";
  for (const auto& pair : *status.entries()) {
    os << scmStatusCodeChar(pair.second) << " " << pair.first << "; ";
  }
  os << "}";
  return os;
}

} // namespace facebook::eden
