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
  jq . << EOF
  {
    "repoid": $SUBMODULE_REPO_ID,
    "default_action": "prepend_prefix",
    "default_prefix": "smallrepofolder1",
    "bookmark_prefix": "bookprefix1/",
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
  SMALL_REPO_CFG=$(default_small_repo_config)
  jq . << EOF
  {
    "repos": {
      "large_repo": {
        "versions": [
          {
            "large_repo_id": $LARGE_REPO_ID,
            "common_pushrebase_bookmarks": ["master"],
            "small_repos": [
              $SMALL_REPO_CFG
            ],
            "version_name": "$LATEST_CONFIG_VERSION_NAME"
          }
        ],
        "common": {
          "common_pushrebase_bookmarks": ["master"],
          "large_repo_id": $LARGE_REPO_ID,
          "small_repos": {
            "$SUBMODULE_REPO_ID": {
              "bookmark_prefix": "bookprefix1/",
              "common_pushrebase_bookmarks_map": { "master": "heads/master" }
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
  INFINITEPUSH_ALLOW_WRITES=true ENABLE_API_WRITES=1 REPOID="$LARGE_REPO_ID" \
    REPONAME="$LARGE_REPO_NAME" setup_common_config "$REPOTYPE"
  # Enable writes in small repo as well, so we can update bookmarks when running gitimport
  INFINITEPUSH_ALLOW_WRITES=true ENABLE_API_WRITES=1 REPOID="$SUBMODULE_REPO_ID" \
    REPONAME="$SUBMODULE_REPO_NAME" setup_common_config "$REPOTYPE"

  setup_sync_config_stripping_git_submodules

  start_and_wait_for_mononoke_server

  # Setting up mutable counter for live forward sync
  # NOTE: this might need to be updated/refactored when setting up test for backsyncing
  sqlite3 "$TESTTMP/monsql/sqlite_dbs" "INSERT INTO mutable_counters (repo_id, name, value) VALUES ($LARGE_REPO_ID, 'xreposync_from_$SUBMODULE_REPO_ID', 1)";

  cd "$TESTTMP" || exit
}

function clone_and_log_large_repo {
  LARGE_BCS_IDS=( "$@" )
  cd "$TESTTMP" || exit
  REPONAME="$LARGE_REPO_NAME" hgmn_clone "mononoke://$(mononoke_address)/$LARGE_REPO_NAME" "$LARGE_REPO_NAME"
  cd "$LARGE_REPO_NAME" || exit


  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    LARGE_CS_ID=$(mononoke_newadmin convert --from bonsai --to hg -R "$LARGE_REPO_NAME" "$LARGE_BCS_ID" --derive)
    if [ -n "$LARGE_CS_ID" ]; then
      hg pull -q -r "$LARGE_CS_ID"
    fi
  done

  hg log --graph -T '{node|short} {desc}\n' --stat -r "sort(all(), desc)"

  printf "\n\nRunning mononoke_admin to verify mapping\n\n"
  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    quiet_grep RewrittenAs -- with_stripped_logs mononoke_admin_source_target "$LARGE_REPO_ID" "$SUBMODULE_REPO_ID" crossrepo map "$LARGE_BCS_ID"
  done

  printf "\nDeriving all the enabled derived data types\n"
  for LARGE_BCS_ID in "${LARGE_BCS_IDS[@]}"; do
    quiet mononoke_newadmin derived-data -R "$LARGE_REPO_NAME" derive --all-types \
      -i "$LARGE_BCS_ID" 2>&1| rg "Error" || true # filter to keep only Error line if there is an error
  done
}
