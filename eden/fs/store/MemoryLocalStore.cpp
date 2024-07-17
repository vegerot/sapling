/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/store/MemoryLocalStore.h"
#include <folly/String.h>
#include "eden/fs/store/StoreResult.h"
#include "eden/fs/telemetry/EdenStats.h"

namespace facebook::eden {

using folly::StringPiece;

namespace {
class MemoryWriteBatch : public LocalStore::WriteBatch {
 public:
  explicit MemoryWriteBatch(MemoryLocalStore* store) : store_(store) {
    storage_.resize(KeySpace::kTotalCount);
  }

  void put(KeySpace keySpace, folly::ByteRange key, folly::ByteRange value)
      override {
    storage_[keySpace->index][StringPiece(key)] = StringPiece(value).str();
  }

  void put(
      KeySpace keySpace,
      folly::ByteRange key,
      std::vector<folly::ByteRange> valueSlices) override {
    std::string value;
    for (const auto& slice : valueSlices) {
      value.append(reinterpret_cast<const char*>(slice.data()), slice.size());
    }
    put(keySpace, key, StringPiece(value));
  }

  void flush() override {
    for (auto& ks : KeySpace::kAll) {
      for (const auto& it : storage_[ks->index]) {
        store_->put(ks, folly::StringPiece(it.first), StringPiece(it.second));
      }
      storage_[ks->index].clear();
    }
  }

 private:
  MemoryLocalStore* store_;
  std::vector<folly::F14NodeMap<std::string, std::string>> storage_;
};
} // namespace

MemoryLocalStore::MemoryLocalStore(EdenStatsPtr edenStats)
    : LocalStore{std::move(edenStats)} {
  storage_.wlock()->resize(KeySpace::kTotalCount);
}

void MemoryLocalStore::open() {}
void MemoryLocalStore::close() {}

void MemoryLocalStore::clearKeySpace(KeySpace keySpace) {
  (*storage_.wlock())[keySpace->index].clear();
}

void MemoryLocalStore::compactKeySpace(KeySpace) {}

StoreResult MemoryLocalStore::get(KeySpace keySpace, folly::ByteRange key)
    const {
  auto store = storage_.rlock();
  auto it = (*store)[keySpace->index].find(StringPiece(key));
  if (it == (*store)[keySpace->index].end()) {
    return StoreResult::missing(keySpace, key);
  }
  return StoreResult(std::string(it->second));
}

bool MemoryLocalStore::hasKey(KeySpace keySpace, folly::ByteRange key) const {
  auto store = storage_.rlock();
  auto it = (*store)[keySpace->index].find(StringPiece(key));
  return it != (*store)[keySpace->index].end();
}

void MemoryLocalStore::put(
    KeySpace keySpace,
    folly::ByteRange key,
    folly::ByteRange value) {
  (*storage_.wlock())[keySpace->index][StringPiece(key)] =
      StringPiece(value).str();
}

std::unique_ptr<LocalStore::WriteBatch> MemoryLocalStore::beginWrite(size_t) {
  return std::make_unique<MemoryWriteBatch>(this);
}

} // namespace facebook::eden
