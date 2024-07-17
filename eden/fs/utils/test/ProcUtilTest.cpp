/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#ifndef _WIN32

#include "eden/fs/utils/ProcUtil.h"

#include <fstream>

#include <folly/Portability.h>
#include <folly/portability/GTest.h>
#include "eden/common/utils/PathFuncs.h"

using namespace facebook::eden;
using namespace facebook::eden::proc_util;

TEST(procUtil, trimTest) {
  std::string tst("");
  EXPECT_EQ(proc_util::trim(tst), "");

  tst = "   spaceBefore";
  EXPECT_EQ(proc_util::trim(tst), "spaceBefore");

  tst = "spaceAfter   ";
  EXPECT_EQ(proc_util::trim(tst), "spaceAfter");

  tst = " spaceBeforeAfter ";
  EXPECT_EQ(proc_util::trim(tst), "spaceBeforeAfter");

  tst = " space between ";
  EXPECT_EQ(proc_util::trim(tst), "space between");

  tst = "noSpaces";
  EXPECT_EQ(proc_util::trim(tst), "noSpaces");

  tst = " \t\n\v\f\r";
  EXPECT_EQ(proc_util::trim(tst), "");

  tst = " \t\n\v\f\rtheGoods \t\n\v\f\r";
  EXPECT_EQ(proc_util::trim(tst), "theGoods");

  tst = "start \t\n\v\f\rend";
  EXPECT_EQ(proc_util::trim(tst), "start \t\n\v\f\rend");
}

TEST(procUtil, splitTest) {
  std::string line;

  line = "key : value";
  auto kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "key");
  EXPECT_EQ(kvPair.second, "value");

  line = "    key :  value      ";
  kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "key");
  EXPECT_EQ(kvPair.second, "value");

  line = "extra:colon:";
  kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "");
  EXPECT_EQ(kvPair.second, "");

  line = "noColonHere";
  kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "");
  EXPECT_EQ(kvPair.second, "");

  line = ":value";
  kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "");
  EXPECT_EQ(kvPair.second, "value");

  line = ":";
  kvPair = proc_util::getKeyValuePair(line, ":");
  EXPECT_EQ(kvPair.first, "");
  EXPECT_EQ(kvPair.second, "");
}

static AbsolutePath dataPath(PathComponentPiece name) {
  if (auto test_data = std::getenv("TEST_DATA")) {
    return realpath(test_data) + name;
  }

  auto thisFile = realpath(__FILE__);
  return thisFile.dirname() + "test-data"_relpath + name;
}

TEST(procUtil, readMemoryStats) {
  auto stats = readMemoryStats();
  if (!stats) {
    EXPECT_FALSE(folly::kIsLinux);
    return;
  }

  EXPECT_GT(stats->vsize, 0);
  EXPECT_GT(stats->resident, 0);
  if (folly::kIsLinux) {
    EXPECT_GT(*stats->shared, 0);
    EXPECT_GT(*stats->text, 0);
    EXPECT_GT(*stats->data, 0);
  }
  EXPECT_GE(stats->vsize, stats->resident);
  if (folly::kIsLinux) {
    EXPECT_GE(stats->vsize, stats->text);
    EXPECT_GE(stats->vsize, stats->data);
  }
}

TEST(procUtil, parseMemoryStats) {
  size_t pageSize = 4096;
  auto stats = parseStatmFile("26995 164 145 11 0 80 0\n", pageSize);
  ASSERT_TRUE(stats.has_value());
  EXPECT_EQ(pageSize * 26995, stats->vsize);
  EXPECT_EQ(pageSize * 164, stats->resident);
  EXPECT_EQ(pageSize * 145, stats->shared);
  EXPECT_EQ(pageSize * 11, stats->text);
  EXPECT_EQ(pageSize * 80, stats->data);

  stats = parseStatmFile("6418297 547249 17716 22695 0 1657632 0\n", pageSize);
  EXPECT_EQ(pageSize * 6418297, stats->vsize);
  EXPECT_EQ(pageSize * 547249, stats->resident);
  EXPECT_EQ(pageSize * 17716, stats->shared);
  EXPECT_EQ(pageSize * 22695, stats->text);
  EXPECT_EQ(pageSize * 1657632, stats->data);
}

TEST(procUtil, procStatusSomeInvalidInput) {
  EXPECT_FALSE(parseStatmFile("26995 164 145 11 0\n", 4096));
  EXPECT_FALSE(parseStatmFile("abc 547249 17716 22695 0 1657632 0\n", 4096));
  EXPECT_FALSE(
      parseStatmFile("6418297 547249 foobar 22695 0 1657632 0\n", 4096));
  EXPECT_FALSE(parseStatmFile("6418297 547249 17716", 4096));
  EXPECT_FALSE(
      parseStatmFile("6418297 -547249 17716 22695 0 1657632 0\n", 4096));
  EXPECT_FALSE(parseStatmFile("6418297 0x14 17716 22695 0 1657632 0\n", 4096));

  EXPECT_TRUE(parseStatmFile("6418297 547249 17716 22695 0 1657632 0\n", 4096));
}

TEST(procUtil, readMemoryStatsNoThrow) {
  auto stats = readStatmFile(canonicalPath("/DOES_NOT_EXIST"));
  EXPECT_FALSE(stats.has_value());
}

TEST(procUtil, procSmapsPrivateBytes) {
  auto procPath = dataPath("ProcSmapsSimple.txt"_pc);
  std::ifstream input(procPath.c_str());
  auto smapsListOfMaps = proc_util::parseProcSmaps(input);
  auto privateBytes = proc_util::calculatePrivateBytes(smapsListOfMaps).value();
  EXPECT_EQ(privateBytes, 20 * 1024);
}

TEST(procUtil, procSmapsSomeInvalidInput) {
  auto procPath = dataPath("ProcSmapsError.txt"_pc);
  auto smapsListOfMaps = proc_util::loadProcSmaps(procPath.c_str());
  auto privateBytes = proc_util::calculatePrivateBytes(smapsListOfMaps).value();
  EXPECT_EQ(privateBytes, 4096);
}

TEST(procUtil, procSmapsUnknownFormat) {
  auto procPath = dataPath("ProcSmapsUnknownFormat.txt"_pc);
  auto smapsListOfMaps = proc_util::loadProcSmaps(procPath.c_str());
  auto privateBytes = proc_util::calculatePrivateBytes(smapsListOfMaps);
  EXPECT_EQ(privateBytes, std::nullopt);
}

TEST(procUtil, noProcSmapsNoThrow) {
  std::string procPath("/DOES_NOT_EXIST");
  auto smapsListOfMaps = proc_util::loadProcSmaps(procPath);
  auto privateBytes = proc_util::calculatePrivateBytes(smapsListOfMaps).value();
  EXPECT_EQ(privateBytes, 0);
}

#endif
