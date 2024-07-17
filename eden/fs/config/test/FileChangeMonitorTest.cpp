/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include <folly/File.h>
#include <folly/FileUtil.h>
#include <folly/portability/GTest.h>
#include <folly/test/TestUtils.h>
#include <folly/testing/TestUtil.h>

#include "eden/common/utils/FileUtils.h"
#include "eden/common/utils/PathFuncs.h"
#include "eden/fs/config/FileChangeMonitor.h"

using namespace std::chrono_literals;
using namespace facebook::eden;

namespace {

using facebook::eden::FileChangeMonitor;
using folly::test::TemporaryDirectory;

class MockFileChangeProcessor {
 public:
  MockFileChangeProcessor(bool throwException = false)
      : throwException_{throwException} {}

  MockFileChangeProcessor(const MockFileChangeProcessor&) = delete;
  MockFileChangeProcessor(MockFileChangeProcessor&&) = delete;
  MockFileChangeProcessor& operator=(const MockFileChangeProcessor&) = delete;
  MockFileChangeProcessor& operator=(MockFileChangeProcessor&&) = delete;

  /**
   * Setting the throwException to true will cause exception to be thrown
   * next time the processor is called.
   */
  void setThrowException(bool throwException) {
    throwException_ = throwException;
  }

  void operator()(
      const folly::File& f,
      int errorNum,
      AbsolutePathPiece /* unused */) {
    callbackCount_++;
    errorNum_ = errorNum;
    fileContents_ = "";
    fileProcessError_ = false;

    if (throwException_) {
      throw std::invalid_argument("Processed invalid value");
    }

    if (errorNum) {
      return;
    }
    try {
      if (!folly::readFile(f.fd(), fileContents_)) {
        fileProcessError_ = true;
      }
    } catch (const std::exception&) {
      fileProcessError_ = true;
    }
  }
  bool isFileProcessError() {
    return fileProcessError_;
  }
  int getErrorNum() {
    return errorNum_;
  }
  std::string& getFileContents() {
    return fileContents_;
  }
  int getCallbackCount() {
    return callbackCount_;
  }

 private:
  bool throwException_{false};
  int errorNum_{0};
  bool fileProcessError_{false};
  std::string fileContents_{};
  int callbackCount_{0};
};

class FileChangeMonitorTest : public ::testing::Test {
 protected:
  // Top level directory to hold test artifacts
  static constexpr folly::StringPiece fcTestName_{"FileChangeTest"};
  static constexpr folly::StringPiece dataOne_{"this is file one"};
  static constexpr folly::StringPiece dataTwo_{"this is file two"};

  std::unique_ptr<TemporaryDirectory> rootTestDir_;
  AbsolutePath rootPath_;
  AbsolutePath pathOne_;
  AbsolutePath pathTwo_;
  void SetUp() override {
    rootTestDir_ = std::make_unique<TemporaryDirectory>(fcTestName_);
    rootPath_ = canonicalPath(rootTestDir_->path().string());
    pathOne_ = rootPath_ + "file.one"_pc;
    writeFileAtomic(pathOne_, dataOne_).throwUnlessValue();

    pathTwo_ = rootPath_ + "file.two"_pc;
    writeFileAtomic(pathTwo_, dataTwo_).throwUnlessValue();
  }
  void TearDown() override {
    rootTestDir_.reset();
  }
};
} // namespace
TEST_F(FileChangeMonitorTest, simpleInitTest) {
  MockFileChangeProcessor fcp;
  auto fcm = std::make_shared<FileChangeMonitor>(pathOne_, 200s);

  EXPECT_EQ(fcm->getFilePath(), pathOne_);

  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  EXPECT_FALSE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);
}

TEST_F(FileChangeMonitorTest, nameChangeTest) {
  MockFileChangeProcessor fcp;
  auto fcm = std::make_shared<FileChangeMonitor>(pathOne_, 100s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), pathOne_);
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  // Changing the file path should force change
  fcm->setFilePath(pathTwo_);
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataTwo_);

  EXPECT_FALSE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataTwo_);

  // Check that the file path was updated
  EXPECT_EQ(fcm->getFilePath(), pathTwo_);
}

TEST_F(FileChangeMonitorTest, noOpNameChangeTest) {
  MockFileChangeProcessor fcp;
  auto fcm = std::make_shared<FileChangeMonitor>(pathOne_, 100s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), pathOne_);
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  // No-op set of file path - no change!
  fcm->setFilePath(pathOne_);
  EXPECT_FALSE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  // Check that the file path is the same
  EXPECT_EQ(fcm->getFilePath(), pathOne_);
}

#ifndef _WIN32
TEST_F(FileChangeMonitorTest, modifyExistFileTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "ModifyExistFile.txt"_pc;
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  writeFileAtomic(path, dataTwo_).throwUnlessValue();

  // File should have changed (there is no throttle)
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataTwo_);
}

TEST_F(FileChangeMonitorTest, fcpMoveTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "FcpMoveTest.txt"_pc;
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  writeFileAtomic(path, dataTwo_).throwUnlessValue();

  auto otherFcm = std::move(fcm);
  MockFileChangeProcessor otherFcp;

  // File should have changed (there is no throttle)
  EXPECT_EQ(otherFcm->getFilePath(), path.value());
  EXPECT_TRUE(otherFcm->invokeIfUpdated(std::ref(otherFcp)));
  EXPECT_EQ(otherFcp.getCallbackCount(), 1);
  EXPECT_EQ(otherFcp.getFileContents(), dataTwo_);
}

TEST_F(FileChangeMonitorTest, modifyExistFileThrottleExpiresTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "ModifyExistThrottleExpiresTest.txt"_pc;
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  auto fcm = std::make_shared<FileChangeMonitor>(path, 10ms);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  writeFileAtomic(path, dataTwo_).throwUnlessValue();

  auto rslt = fcm->invokeIfUpdated(std::ref(fcp));
  if (!rslt) {
    // The test ran fast (less than 10 millisecond). In this event,
    // check our results (not updated). Then, sleep for a second and validate
    // the update.
    EXPECT_EQ(fcp.getCallbackCount(), 1);
    EXPECT_EQ(fcp.getFileContents(), dataOne_);
    /* sleep override */
    sleep(1);
    rslt = fcm->invokeIfUpdated(std::ref(fcp));
  }
  EXPECT_TRUE(rslt);
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataTwo_);
}
#endif

TEST_F(FileChangeMonitorTest, modifyExistFileThrottleActiveTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "ModifyExistFileThrottleActive.txt"_pc;
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  auto fcm = std::make_shared<FileChangeMonitor>(path, 10s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  writeFileAtomic(path, dataTwo_).throwUnlessValue();

  // File change throttled
  auto rslt = fcm->invokeIfUpdated(std::ref(fcp));

  EXPECT_FALSE(rslt);
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);
}

TEST_F(FileChangeMonitorTest, nonExistFileTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "NonExist.txt"_pc;

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getErrorNum(), ENOENT);
}

TEST_F(FileChangeMonitorTest, readFailTest) {
  MockFileChangeProcessor fcp;

  // Note: we are using directory as our path
  auto path = rootPath_;
  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);

#ifndef _WIN32
  // Directory can be opened, but read will fail.
  EXPECT_EQ(fcp.getErrorNum(), 0);
  EXPECT_TRUE(fcp.isFileProcessError());
#else
  // Windows can't open directories
  EXPECT_NE(fcp.getErrorNum(), 0);
#endif
}

TEST_F(FileChangeMonitorTest, rmFileTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "ExistToNonExist.txt"_pc;
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path.value());
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);

  // Delete file
  remove(path.c_str());

  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getErrorNum(), ENOENT);
}

TEST_F(FileChangeMonitorTest, processExceptionTest) {
  MockFileChangeProcessor fcp{true};
  auto fcm = std::make_shared<FileChangeMonitor>(pathOne_, 0s);

  // Processor should throw exception on call to invokeIfUpdated
  EXPECT_EQ(fcm->getFilePath(), pathOne_);
  EXPECT_THROW(
      {
        try {
          fcm->invokeIfUpdated(std::ref(fcp));
        } catch (const std::invalid_argument& e) {
          EXPECT_STREQ("Processed invalid value", e.what());
          throw;
        }
      },
      std::invalid_argument);
}

TEST_F(FileChangeMonitorTest, createFileTest) {
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "NonExistToExist.txt"_pc;

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // Initial path and change check
  EXPECT_EQ(fcm->getFilePath(), path);
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getErrorNum(), ENOENT);

  // Create the file
  writeFileAtomic(path, dataOne_).throwUnlessValue();

  // File should have changed
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);
}

#ifndef _WIN32
TEST_F(FileChangeMonitorTest, openFailTest) {
  // Eden tests are run as root on Sandcastle - which invalidates this test.
  if (getuid() == 0) {
    return;
  }
  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "OpenFailTest.txt"_pc;

  // Create the file
  writeFileAtomic(path, dataOne_).throwUnlessValue();
  chmod(path.c_str(), S_IEXEC);

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // First time - file changed, but cannot read
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getErrorNum(), EACCES);

  // Nothing changed
  EXPECT_FALSE(fcm->invokeIfUpdated(std::ref(fcp)));

  // Update file - keep permissions same (inaccessible)
  writeFileAtomic(path, dataTwo_).throwUnlessValue();
  EXPECT_EQ(chmod(path.c_str(), S_IEXEC), 0);

  // FileChangeMonitor will not notify if the file has changed AND there is
  // still the same open error.
  EXPECT_FALSE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getErrorNum(), EACCES);
}

TEST_F(FileChangeMonitorTest, openFailFixTest) {
  // Eden tests are run as root on Sandcastle - which invalidates this test.
  if (getuid() == 0) {
    return;
  }

  MockFileChangeProcessor fcp;
  auto path = rootPath_ + "OpenFailFixTest.txt"_pc;

  // Create the file
  writeFileAtomic(path, dataOne_).throwUnlessValue();
  EXPECT_EQ(chmod(path.c_str(), S_IEXEC), 0);

  auto fcm = std::make_shared<FileChangeMonitor>(path, 0s);

  // First time - file changed, no read permission
  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 1);
  EXPECT_EQ(fcp.getErrorNum(), EACCES);

  // Fix permissions
  EXPECT_EQ(chmod(path.c_str(), S_IRUSR | S_IRGRP | S_IROTH), 0);

  EXPECT_TRUE(fcm->invokeIfUpdated(std::ref(fcp)));
  EXPECT_EQ(fcp.getCallbackCount(), 2);
  EXPECT_EQ(fcp.getFileContents(), dataOne_);
}
#endif
