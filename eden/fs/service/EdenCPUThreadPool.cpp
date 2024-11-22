/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/service/EdenCPUThreadPool.h"

namespace facebook::eden {

EdenCPUThreadPool::EdenCPUThreadPool(uint8_t numThreads)
    : UnboundedQueueExecutor(numThreads, "EdenCPUThread") {}

} // namespace facebook::eden
