/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#ifndef _WIN32

#include <folly/String.h>
#include <folly/portability/GTest.h>
#include <folly/test/TestUtils.h>

#include "eden/common/utils/Bug.h"
#include "eden/fs/inodes/EdenMount.h"
#include "eden/fs/inodes/FileInode.h"
#include "eden/fs/inodes/InodeMap.h"
#include "eden/fs/inodes/TreeInode.h"
#include "eden/fs/store/ObjectFetchContext.h"
#include "eden/fs/testharness/FakeBackingStore.h"
#include "eden/fs/testharness/FakeTreeBuilder.h"
#include "eden/fs/testharness/TestMount.h"
#include "eden/fs/testharness/TestUtil.h"

using namespace facebook::eden;
using folly::StringPiece;

class SymlinkTest : public ::testing::Test {
 protected:
  void SetUp() override {
    // Set up a directory structure that we will use for most
    // of the tests below
    FakeTreeBuilder builder;
    builder.setFiles({
        {"doc.txt", "hello\n"},
    });
    mount_.initialize(builder);
    mount_.mkdir("a");
  }

  TestMount mount_;
};

TEST_F(SymlinkTest, makeSymlink) {
  StringPiece name{"s1"}; // node to create in the filesystem
  StringPiece target{"foo!"}; // the value we want readlink to return

  auto root = mount_.getTreeInode(RelativePathPiece());
  auto inode =
      root->symlink(PathComponentPiece{name}, target, InvalidationRequired::No);
  EXPECT_EQ(dtype_t::Symlink, inode->getType());
  EXPECT_EQ(
      inode->readlink(ObjectFetchContext::getNullContext()).get(), target);
  // Let's make sure that we can load it up and see the right results
  auto loadedInode = mount_.getFileInode(RelativePathPiece{name});
  EXPECT_EQ(
      loadedInode->readlink(ObjectFetchContext::getNullContext()).get(),
      target);
}

TEST_F(SymlinkTest, makeSymlinkCollisionFile) {
  StringPiece name{"doc.txt"}; // node to create in the filesystem
  StringPiece target{"foo!"}; // the value we want readlink to return

  auto root = mount_.getTreeInode(RelativePathPiece());
  // Name already exists, so we expect this to fail
  EXPECT_THROW_ERRNO(
      root->symlink(PathComponentPiece{name}, target, InvalidationRequired::No),
      EEXIST);
}

TEST_F(SymlinkTest, makeSymlinkCollisionDir) {
  StringPiece name{"a"}; // node to create in the filesystem
  StringPiece target{"foo!"}; // the value we want readlink to return

  auto root = mount_.getTreeInode(RelativePathPiece());
  // Name already exists, so we expect this to fail
  EXPECT_THROW_ERRNO(
      root->symlink(PathComponentPiece{name}, target, InvalidationRequired::No),
      EEXIST);
}

#endif
