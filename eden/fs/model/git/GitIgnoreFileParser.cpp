/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include <folly/File.h>
#include <folly/FileUtil.h>
#include <folly/logging/xlog.h>

#include "eden/common/utils/SystemError.h"
#include "eden/fs/model/git/GitIgnoreFileParser.h"

namespace facebook::eden {

folly::Expected<GitIgnore, int> GitIgnoreFileParser::operator()(
    int fileDescriptor,
    AbsolutePathPiece filePath) const {
  GitIgnore gitIgnore;
  try {
    std::string fileContents;
    if (!folly::readFile(fileDescriptor, fileContents)) {
      return folly::makeUnexpected((int)errno);
    }
    if (!folly::trimWhitespace(fileContents).empty()) {
      gitIgnore.loadFile(fileContents);
    }
  } catch (const std::system_error& ex) {
    int errNum{EIO};
    if (isErrnoError(ex)) {
      errNum = ex.code().value();
    }
    if (errNum != ENOENT) {
      XLOG(WARNING) << "error reading file " << filePath
                    << folly::exceptionStr(ex);
    }
    return folly::makeUnexpected((int)errNum);
  } catch (const std::exception& ex) {
    XLOG(WARNING) << "error reading file " << filePath
                  << folly::exceptionStr(ex);
    return folly::makeUnexpected<int>((int)EIO);
  }
  return gitIgnore;
}

} // namespace facebook::eden
