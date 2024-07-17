# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

# Create a repository
  $ setup_common_config
  $ REPOID=1 FILESTORE=1 FILESTORE_CHUNK_SIZE=10 setup_mononoke_repo_config lfs1
  $ LIVE_CONFIG="${LOCAL_CONFIGERATOR_PATH}/live.json"
  $ cat > "$LIVE_CONFIG" << EOF
  > {
  >   "track_bytes_sent": true,
  >   "enable_consistent_routing": false,
  >   "disable_hostname_logging": true,
  >   "enforce_acl_check": false
  > }
  > EOF

# Start an LFS server for this repository
  $ SCUBA="$TESTTMP/scuba.json"
  $ lfs_log="$TESTTMP/lfs.log"
  $ lfs_root="$(lfs_server --log "$lfs_log" --scuba-dataset "file://$SCUBA" --live-config "$(get_configerator_relative_path "${LIVE_CONFIG}")")"

# Get the config
  $ curltest -fs "${lfs_root}/config" | jq -S .
  {
    "disable_compression": false,
    "disable_compression_identities": [],
    "disable_hostname_logging": true,
    "enable_consistent_routing": false,
    "enforce_acl_check": false,
    "enforce_authentication": false,
    "loadshedding_limits": [],
    "object_popularity": null,
    "track_bytes_sent": true
  }

# Send some data
  $ yes A 2>/dev/null | head -c 2KiB | hg --config extensions.lfs= debuglfssend "${lfs_root}/lfs1"
  ab02c2a1923c8eb11cb3ddab70320746d71d32ad63f255698dc67c3295757746 2048

# Check that Scuba logs *do not* contain `client_hostname`
  $ wait_for_json_record_count "$SCUBA" 3
  $ jq -S .normal.client_hostname < "$SCUBA"
  null
  null
  null

# Update the config
  $ cat > "$LIVE_CONFIG" << EOF
  > {
  >   "track_bytes_sent": false,
  >   "enable_consistent_routing": false,
  >   "disable_hostname_logging": false,
  >   "enforce_acl_check": false
  > }
  > EOF

# Wait for the config to be updated
  $ sleep 2

# Send some data
  $ yes A 2>/dev/null | head -c 2KiB | hg --config extensions.lfs= debuglfssend "${lfs_root}/lfs1"
  ab02c2a1923c8eb11cb3ddab70320746d71d32ad63f255698dc67c3295757746 2048

# Check that Scuba logs contain `client_hostname`
  $ wait_for_json_record_count "$SCUBA" 4
  $ jq -S .normal.client_hostname < "$SCUBA"
  null
  null
  null
  "localhost"
