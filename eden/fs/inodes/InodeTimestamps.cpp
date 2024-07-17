/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/inodes/InodeTimestamps.h"

#include <folly/Conv.h>
#include <sys/stat.h>
#include "eden/common/utils/Throw.h"
#include "eden/fs/inodes/InodeMetadata.h"
#include "eden/fs/utils/Clock.h"

namespace facebook::eden {

namespace {

/**
 * We do a number of comparisons in the below logic to ensure we don't
 * {under, over}flow. To avoid UB, ensure these types are signed numbers.
 */
using tv_sec_t = decltype(timespec::tv_sec);
using tv_nsec_t = decltype(timespec::tv_nsec);
static_assert(
    std::is_signed<tv_sec_t>::value,
    "expect tv_sec to be signed type");
static_assert(
    std::is_signed<tv_nsec_t>::value,
    "expect tv_nsec to be signed type");

/**
 * Like ext4, our earliest representable date is 2^31 seconds before the unix
 * epoch, which works out to December 13th, 1901.
 */
constexpr int64_t kEpochOffsetSeconds = 0x80000000ll;

/**
 * On Windows, the FILETIME offset is 11644473600 seconds before the unix
 * epoch, which works out to Jan 1st, 1601.
 */
constexpr int64_t kEpochFileTimeOffsetSeconds = 11644473600ll;

/**
 * Largest representable sec,nsec pair.
 *
 * $ python3
 * >>> kEpochOffsetSeconds = 0x80000000
 * >>> kLargestRepresentableSec = 16299260425
 * >>> kLargestRepresentableNsec = 709551615
 * >>> hex((kEpochOffsetSeconds + kLargestRepresentableSec) * 1000000000 + \
 * ... kLargestRepresentableNsec)
 * '0xffffffffffffffff'
 */
constexpr tv_sec_t kLargestRepresentableSec = 16299260425;
constexpr tv_nsec_t kLargestRepresentableNsec = 709551615;

struct ClampPolicy {
  static constexpr bool is_noexcept = true;
  static uint64_t minimum(timespec /*ts*/) noexcept {
    return 0;
  }
  static uint64_t maximum(timespec /*ts*/) noexcept {
    return ~0ull;
  }
};

struct ThrowPolicy {
  static constexpr bool is_noexcept = false;
  static uint64_t minimum(timespec ts) {
    throw_<std::underflow_error>(
        "underflow converting timespec (",
        ts.tv_sec,
        " s, ",
        ts.tv_nsec,
        " ns) to EdenTimestamp");
  }
  static uint64_t maximum(timespec ts) {
    throw_<std::overflow_error>(
        "overflow converting timespec (",
        ts.tv_sec,
        " s, ",
        ts.tv_nsec,
        " ns) to EdenTimestamp");
  }
};

template <typename OutOfRangePolicy>
uint64_t repFromTimespec(timespec ts) noexcept(OutOfRangePolicy::is_noexcept) {
  if (ts.tv_sec < -kEpochOffsetSeconds) {
    return OutOfRangePolicy::minimum(ts);
  } else if (
      ts.tv_sec > kLargestRepresentableSec ||
      (ts.tv_sec == kLargestRepresentableSec &&
       ts.tv_nsec > kLargestRepresentableNsec)) {
    return OutOfRangePolicy::maximum(ts);
  } else {
    // Assume that ts.tv_nsec is within [0, 1000000000).
    // The first addition must be unsigned to avoid UB.
    return (static_cast<uint64_t>(kEpochOffsetSeconds) +
            static_cast<uint64_t>(ts.tv_sec)) *
        1000000000ll +
        ts.tv_nsec;
  }
}

timespec repToTimespec(uint64_t nsec) {
  static constexpr uint64_t kEpochNsec = kEpochOffsetSeconds * 1000000000ull;
  if (nsec < kEpochNsec) {
    int64_t before_epoch = kEpochNsec - nsec;
    timespec ts;
    auto sec = (before_epoch + 999999999) / 1000000000;
    ts.tv_sec = -sec;
    ts.tv_nsec = sec * 1000000000 - before_epoch;
    return ts;
  } else {
    uint64_t after_epoch = nsec - kEpochNsec;
    timespec ts;
    ts.tv_sec = after_epoch / 1000000000;
    ts.tv_nsec = after_epoch % 1000000000;
    return ts;
  }
}

} // namespace

EdenTimestamp::EdenTimestamp(timespec ts, Clamp) noexcept
    : nsec_{repFromTimespec<ClampPolicy>(ts)} {}

EdenTimestamp::EdenTimestamp(timespec ts, ThrowIfOutOfRange)
    : nsec_{repFromTimespec<ThrowPolicy>(ts)} {}

timespec EdenTimestamp::toTimespec() const noexcept {
  return repToTimespec(nsec_);
}

timespec EdenTimestamp::toFileTime() const noexcept {
  constexpr uint64_t kOffsetSinceEdenEpochSeconds =
      kEpochFileTimeOffsetSeconds - kEpochOffsetSeconds;
  uint64_t offsetSinceEdenEpochNsec =
      kOffsetSinceEdenEpochSeconds * 1000000000ull;

  // TODO(xavierd): Handle nsec_ > max representable timespec.

  auto timestamp = offsetSinceEdenEpochNsec + nsec_;
  timespec ts;
  ts.tv_sec = timestamp / 1000000000ull;
  ts.tv_nsec = timestamp % 1000000000ull;
  return ts;
}

#ifndef _WIN32
void InodeTimestamps::setattrTimes(
    const Clock& clock,
    const DesiredMetadata& attr) {
  const auto now = clock.getRealtime();

  // Set atime for TreeInode.
  if (attr.atime.has_value()) {
    atime = attr.atime.value();
  }

  // Set mtime for TreeInode.
  if (attr.mtime.has_value()) {
    mtime = attr.mtime.value();
  }

  // we do not allow users to set ctime using setattr. ctime should be changed
  // when ever setattr is called, as this function is called in setattr, update
  // ctime to now.
  ctime = now;
}

void InodeTimestamps::applyToStat(struct stat& st) const {
#ifdef __APPLE__
  st.st_atimespec = atime.toTimespec();
  st.st_ctimespec = ctime.toTimespec();
  st.st_mtimespec = mtime.toTimespec();
#elif defined(_BSD_SOURCE) || defined(_SVID_SOURCE) || \
    _POSIX_C_SOURCE >= 200809L || _XOPEN_SOURCE >= 700
  st.st_atim = atime.toTimespec();
  st.st_ctim = ctime.toTimespec();
  st.st_mtim = mtime.toTimespec();
#else
  st.st_atime = atime.toTimespec().tv_sec;
  st.st_mtime = mtime.toTimespec().tv_sec;
  st.st_ctime = ctime.toTimespec().tv_sec;
#endif
}
#endif

} // namespace facebook::eden
