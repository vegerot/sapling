/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <memory>

namespace facebook::eden {

class TreeMetadata;
using TreeMetadataPtr = std::shared_ptr<const TreeMetadata>;

} // namespace facebook::eden
