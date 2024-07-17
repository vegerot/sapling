/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#pragma once

#include <ctime>

namespace facebook::eden {

/**
 * Represents access to the system clock(s).
 */
class Clock {
 public:
  virtual ~Clock() = default;

  /**
   * Returns the real time elapsed since the Epoch.
   */
  virtual timespec getRealtime() const = 0;
};

/**
 *
 */
class UnixClock : public Clock {
 public:
  /// CLOCK_REALTIME
  timespec getRealtime() const override;
};

} // namespace facebook::eden
