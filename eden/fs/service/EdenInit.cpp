/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/service/EdenInit.h"

#include <boost/filesystem.hpp>

#include "eden/common/utils/PathFuncs.h"
#include "eden/common/utils/UserInfo.h"
#include "eden/fs/config/EdenConfig.h"
#include "eden/fs/config/TomlFileConfigSource.h"
#include "eden/fs/eden-config.h"

using folly::StringPiece;

DEFINE_string(configPath, "", "The path of the ~/.edenrc config file");
DEFINE_string(edenDir, "", "The path to the .eden directory");
DEFINE_string(
    etcEdenDir,
    EDEN_ETC_EDEN_DIR,
    "The directory holding all system configuration files");
DEFINE_bool(
    foreground,
    false,
    "Run edenfs in the foreground, rather than daemonizing "
    "as a background process");
DEFINE_string(
    logPath,
    "",
    "If set, redirects stdout and stderr to the log file given.");

namespace {
using namespace facebook::eden;

constexpr StringPiece kDefaultUserConfigFile{".edenrc"};
constexpr StringPiece kEdenfsSystemConfigFile{"edenfs.rc"};
constexpr StringPiece kEdenfsDynamicConfigFile{"edenfs_dynamic.rc"};

void findEdenDir(EdenConfig& config) {
  // Get the initial path to the Eden directory.
  // We use the --edenDir flag if set, otherwise the value loaded from the
  // config file.
  boost::filesystem::path boostPath(
      FLAGS_edenDir.empty() ? config.edenDir.getValue().value()
                            : FLAGS_edenDir);

  try {
    // Ensure that the directory exists, and then canonicalize its name with
    // realpath().  Using realpath() requires that the directory exist.
    boost::filesystem::create_directories(boostPath);
    auto resolvedDir = facebook::eden::realpath(boostPath.string());

    // Updating the value in the config using ConfigSource::CommandLine also
    // makes sure that any future updates to the config file do not affect the
    // value we use.  Once we start we want to always use a fixed location for
    // the eden directory.
    config.edenDir.setValue(resolvedDir, ConfigSourceType::CommandLine);
  } catch (const std::exception& ex) {
    throw ArgumentError(fmt::format(
        FMT_STRING("error creating {}: {}"),
        boostPath.string(),
        folly::exceptionStr(ex).c_str()));
  }
}

} // namespace

namespace facebook::eden {

PathComponentPiece getDefaultLogFileName() {
  return "edenfs.log"_pc;
}

AbsolutePath makeDefaultLogDirectory(AbsolutePathPiece edenDir) {
  auto logDir = edenDir + "logs"_pc;
  ensureDirectoryExists(logDir);
  return logDir;
}

std::string getLogPath(AbsolutePathPiece edenDir) {
  // If a log path was explicitly specified as a command line argument use that
  if (!FLAGS_logPath.empty()) {
    return FLAGS_logPath;
  }

  // If we are running in the foreground default to an empty log path
  // (just log directly to stderr)
  if (FLAGS_foreground) {
    return "";
  }

  auto logDir = makeDefaultLogDirectory(edenDir);
  return (logDir + getDefaultLogFileName()).value();
}

std::unique_ptr<EdenConfig> getEdenConfig(UserInfo& identity) {
  // normalizeBestEffort() to try resolving symlinks in these paths but don't
  // fail if they don't exist.
  AbsolutePath systemConfigDir;
  try {
    systemConfigDir = normalizeBestEffort(FLAGS_etcEdenDir);
  } catch (const std::exception& ex) {
    throw ArgumentError(fmt::format(
        FMT_STRING("invalid flag value: {}: {}"),
        FLAGS_etcEdenDir,
        folly::exceptionStr(ex).c_str()));
  }
  const auto systemConfigPath =
      systemConfigDir + PathComponentPiece{kEdenfsSystemConfigFile};

  const auto dynamicConfigPath =
      systemConfigDir + PathComponentPiece{kEdenfsDynamicConfigFile};

  const std::string configPathStr = FLAGS_configPath;
  AbsolutePath userConfigPath;
  if (configPathStr.empty()) {
    userConfigPath = identity.getHomeDirectory() +
        PathComponentPiece{kDefaultUserConfigFile};
  } else {
    try {
      userConfigPath = normalizeBestEffort(configPathStr);
    } catch (const std::exception& ex) {
      throw ArgumentError(fmt::format(
          FMT_STRING("invalid flag value: {}: {}"),
          FLAGS_configPath,
          folly::exceptionStr(ex).c_str()));
    }
  }

  auto systemConfigSource = std::make_shared<TomlFileConfigSource>(
      systemConfigPath, ConfigSourceType::SystemConfig);

  auto dynamicConfigSource = std::make_shared<TomlFileConfigSource>(
      dynamicConfigPath, ConfigSourceType::Dynamic);

  auto userConfigSource = std::make_shared<TomlFileConfigSource>(
      userConfigPath, ConfigSourceType::UserConfig);

  // Create the default EdenConfig. Next, update with command line arguments.
  // Command line arguments will take precedence over config file settings.
  //
  // TODO: The command line should have its own ConfigSource and they can all be
  // applied in order of precedence.
  auto edenConfig = std::make_unique<EdenConfig>(
      getUserConfigVariables(identity),
      identity.getHomeDirectory(),
      systemConfigDir,
      EdenConfig::SourceVector{
          std::move(systemConfigSource),
          std::move(dynamicConfigSource),
          std::move(userConfigSource)});

  // Determine the location of the Eden state directory, and update this value
  // in the EdenConfig object.  This also creates the directory if it does not
  // exist.
  findEdenDir(*edenConfig);

  return edenConfig;
}

} // namespace facebook::eden
