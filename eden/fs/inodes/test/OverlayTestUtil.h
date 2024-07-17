/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <iomanip>
#include <sstream>

#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/inodes/DirEntry.h"
#include "eden/fs/inodes/InodeNumber.h"
#include "eden/fs/inodes/Overlay.h"

namespace facebook::eden {

void debugDumpOverlayInodes(
    Overlay& overlay,
    InodeNumber rootInode,
    folly::StringPiece path,
    std::ostringstream& out);

inline std::string debugDumpOverlayInodes(
    Overlay& overlay,
    InodeNumber rootInode) {
  std::ostringstream out;
  debugDumpOverlayInodes(overlay, rootInode, "/", out);
  return out.str();
}

} // namespace facebook::eden
