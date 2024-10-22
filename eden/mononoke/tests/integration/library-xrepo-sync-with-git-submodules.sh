#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# shellcheck source=fbcode/eden/mononoke/tests/integration/library.sh
. "${TEST_FIXTURES}/library.sh"

# Run initial setup (e.g. sync configs, small & large repos)
REPOTYPE="blob_files"

# Used by integration tests that source this file
# shellcheck disable=SC2034
NEW_BOOKMARK_NAME="SYNCED_HEAD"

LATEST_CONFIG_VERSION_NAME="INITIAL_IMPORT_SYNC_CONFIG"



# By default, the `git_submodules_action` will be `STRIP`, meaning that any
# changes to git submodules will not be synced to the large repo.
function default_small_repo_config {
  local repo_folder=${SMALL_REPO_DIR-smallrepofolder1}
  jq . << EOF
  {
    "repoid": $SUBMODULE_REPO_ID,
    "default_action": "prepend_prefix",
    "default_prefix": "$repo_folder",
    "bookmark_prefix": "$repo_folder/",
    "mapping": {
      "special": "specialsmallrepofolder_after_change"
    },
    "direction": "small_to_large"
  }
EOF
}

# Sets up a config to sync commits from a small repo to a large repo.
# By default, the `git_submodules_action` will be `STRIP`, meaning that any
# changes to git submodules will not be synced to the large repo.
function default_initial_import_config {
  repo_folder=${SMALL_REPO_DIR-smallrepofolder1}
  SMALL_REPO_CFG=$(default_small_repo_config)
  jq . << EOF
  {
    "repos": {
      "large_repo": {
        "versions": [
          {
            "large_repo_id": $LARGE_REPO_ID,
            "common_pushrebase_bookmarks": ["master_bookmark"],
            "small_repos": [
              $SMALL_REPO_CFG
            ],
            "version_name": "$LATEST_CONFIG_VERSION_NAME"
          }
        ],
        "common": {
          "common_pushrebase_bookmarks": ["master_bookmark"],
          "large_repo_id": $LARGE_REPO_ID,
          "small_repos": {
            "$SUBMODULE_REPO_ID": {
              "bookmark_prefix": "$repo_folder/",
              "common_pushrebase_bookmarks_map": { "master_bookmark": "heads/master_bookmark" }
            }
          }
        }
      }
    }
  }
EOF
}

# Update the value for the git submodule action in a small repo config
# e.g. to keep or expand the changes.
function set_git_submodules_action_in_config_version {
  VERSION_NAME=$1
  MOD_SMALL_REPO=$2
  NEW_ACTION=$3

  TEMP_FILE="$TESTTMP/COMMIT_SYNC_CONF_all"

  jq ".repos.large_repo.versions |= map(if .version_name != \"$VERSION_NAME\" then . else  .small_repos |= map(if .repoid == $MOD_SMALL_REPO then . + {\"git_submodules_action\": $NEW_ACTION} else . end) end)" "$COMMIT_SYNC_CONF/all" > "$TEMP_FILE"

  mv "$TEMP_FILE" "$COMMIT_SYNC_CONF/all"
}

function set_git_submodule_dependencies_in_config_version {
  VERSION_NAME=$1
  MOD_SMALL_REPO=$2
  NEW_VALUE=$3

  TEMP_FILE="$TESTTMP/COMMIT_SYNC_CONF_all"

  jq ".repos.large_repo.versions |= map(if .version_name != \"$VERSION_NAME\" then . else  .small_repos |= map(if .repoid == $MOD_SMALL_REPO then . + {\"submodule_dependencies\": $NEW_VALUE} else . end) end)" "$COMMIT_SYNC_CONF/all" > "$TEMP_FILE"

  mv "$TEMP_FILE" "$COMMIT_SYNC_CONF/all"
}

function setup_sync_config_stripping_git_submodules {
  default_initial_import_config  > "$COMMIT_SYNC_CONF/all"
}

function run_common_xrepo_sync_with_gitsubmodules_setup {
  export SM_COMMIT_DATE="1970-1-1 00:00:01"
  # Avoid local clone error "fatal: transport 'file' not allowed" in new Git versions (see CVE-2022-39253).
  export XDG_CONFIG_HOME=$TESTTMP
  git config --global protocol.file.allow always

  INFINITEPUSH_ALLOW_WRITES=true REPOID="$LARGE_REPO_ID" \
    REPONAME="$LARGE_REPO_NAME" setup_common_config "$REPOTYPE"
  # Enable writes in small repo as well, so we can update bookmarks when running gitimport,
  # and set the default commit identity schema to git.
  INFINITEPUSH_ALLOW_WRITES=true REPOID="$SUBMODULE_REPO_ID" \
    REPONAME="$SUBMODULE_REPO_NAME" COMMIT_IDENTITY_SCHEME=3 setup_common_config "$REPOTYPE"

  # Save a copy of the config before the deny_files hook, so we can disable it later
  cp "$TESTTMP/mononoke-config/repos/$LARGE_REPO_NAME/server.toml" "$TESTTMP/old_large_repo_config.toml"
  # Disable pushes to small repo's directory in large repo
  cat >> "$TESTTMP/mononoke-config/repos/$LARGE_REPO_NAME/server.toml" << CONFIG
[[bookmarks]]
name="$MASTER_BOOKMARK_NAME"
[[bookmarks.hooks]]
hook_name="deny_files"
[[hooks]]
name="deny_files"
[hooks.config_string_lists]
  native_push_only_deny_patterns = [
    "^$SMALL_REPO_DIR/",
  ]
CONFIG

  # Set the REPONAME environment variable to the large repo name, so that all
  # sapling commands run with the large repo by default.
  # The small repos don't support sapling, because hg types are not derived in
  # them, since they have submodule file changes.
  export REPONAME=$LARGE_REPO_NAME

  setup_sync_config_stripping_git_submodules

  start_and_wait_for_mononoke_server

  # Create a commit in the large repo
  testtool_drawdag -R "$LARGE_REPO_NAME" --no-default-files <<EOF
L_A
# modify: L_A "file_in_large_repo.txt" "first file"
# bookmark: L_A master_bookmark
EOF

  # Setting up mutable counter for live forward sync
  # NOTE: this might need to be updated/refactored when setting up test for backsyncing
  sqlite3 "$TESTTMP/monsql/sqlite_dbs" "INSERT INTO mutable_counters (repo_id, name, value) VALUES ($LARGE_REPO_ID, 'xreposync_from_$SUBMODULE_REPO_ID', 1)";

  cd "$TESTTMP" || exit
}

function sl_log() {
   hg log --graph -T '{node|short} {desc}\n' "$@"
}

function clone_and_log_large_repo {
  LARGE_BCS_IDS=( "$@" )
  cd "$TESTTMP" || exit
  clone_large_repo

  cd "$LARGE_REPO_NAME" || exit

  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    LARGE_CS_ID=$(mononoke_newadmin convert --from bonsai --to hg -R "$LARGE_REPO_NAME" "$LARGE_BCS_ID" --derive)
    if [ -n "$LARGE_CS_ID" ]; then
      hg pull -q -r "$LARGE_CS_ID"
    fi
  done

  sl_log --stat -r "sort(all(), desc)"

  printf "\n\nRunning mononoke_newadmin to verify mapping\n\n"
  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    quiet_grep RewrittenAs -- with_stripped_logs mononoke_newadmin cross-repo --source-repo-id "$LARGE_REPO_ID" --target-repo-id "$SUBMODULE_REPO_ID" map -i "$LARGE_BCS_ID"
  done

  printf "\nDeriving all the enabled derived data types\n"
  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    quiet mononoke_newadmin derived-data -R "$LARGE_REPO_NAME" derive --all-types \
      -i "$LARGE_BCS_ID" 2>&1| rg "Error" || true # filter to keep only Error line if there is an error
  done
}

# Clone the large repo if it hasn't been cloned yet
function clone_large_repo {
  orig_pwd=$(pwd)
  cd "$TESTTMP" || exit
  if [ ! -d "$LARGE_REPO_NAME" ]; then
    hg clone -q "mono:$LARGE_REPO_NAME" "$LARGE_REPO_NAME"
  fi

  cd "$orig_pwd" || exit
}
