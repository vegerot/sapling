# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

-- Define the large and small repo ids and names before calling any helpers
  $ export LARGE_REPO_NAME="large_repo"
  $ export LARGE_REPO_ID=10
  $ export SUBMODULE_REPO_NAME="small_repo"
  $ export SUBMODULE_REPO_ID=11

  $ . "${TEST_FIXTURES}/library.sh"
  $ . "${TEST_FIXTURES}/library-push-redirector.sh"
  $ . "${TEST_FIXTURES}/library-xrepo-sync-with-git-submodules.sh"



Setup configuration
  $ run_common_xrepo_sync_with_gitsubmodules_setup
  L_A=b006a2b1425af8612bc80ff4aa9fa8a1a2c44936ad167dd21cb9af2a9a0248c4
# Action 1 is to Keep submodules
  $ set_git_submodules_action_in_config_version "$LATEST_CONFIG_VERSION_NAME" "$SUBMODULE_REPO_ID" 1


# Test that if, for some reason, we want to keep the git submodules in the
# future, we're able to do so.
# This will sync the commits and keep the git sub-modules changes, so
# when the large repo is cloned, there'll be a failure because hg derived data
# can't be derived for them.
Create commit that modifies git submodule in small repo
  $ testtool_drawdag -R "$SUBMODULE_REPO_NAME" --no-default-files <<EOF
  > A-B-C
  > # modify: A "foo/a.txt" "creating foo directory"
  > # modify: A "bar/b.txt" "creating bar directory"
  > # modify: B "bar/c.txt" "random change"
  > # modify: B "foo/git_submodule" git-submodule "creating git submodule"
  > # copy: C "foo/b.txt" "copying file from bar into foo" B "bar/b.txt"
  > # bookmark: C master
  > EOF
  A=7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36
  B=cd6bd41f62adb809024156682965586754610ac4687b2833317151c239a58b71
  C=ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be

  $ with_stripped_logs mononoke_x_repo_sync "$SUBMODULE_REPO_ID"  "$LARGE_REPO_ID" \
  > initial-import --no-progress-bar -i "$C" \
  > --version-name "$LATEST_CONFIG_VERSION_NAME" --no-automatic-derivation | tee $TESTTMP/initial_import.out
  Starting session with id * (glob)
  Starting up X Repo Sync from small repo small_repo to large repo large_repo
  Checking if ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be is already synced 11->10
  Syncing ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be for inital import
  Source repo: small_repo / Target repo: large_repo
  Found 3 unsynced ancestors
  changeset ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be synced as f299e57c379932297b130d60f6d86e54c87c8e02507bf0867783e23d7d8f8a50 in * (glob)
  successful sync of head ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be
  X Repo Sync execution finished from small repo small_repo to large repo large_repo

# NOTE: this command is expected to fail because some types can't be derived 
# for bonsais with git submodules.
  $ SYNCED_HEAD=$(rg ".+synced as (\w+) .+" -or '$1' "$TESTTMP/initial_import.out")
  $ clone_and_log_large_repo "$SYNCED_HEAD"
  Error: Failed to derive Mercurial changeset
  
  Caused by:
      0: failed to derive hgchangesets batch (start:ac220d3e57adf7c31a869141787d3bc638d79a3f1dd54b0ba54d545c260f14e6, end:f299e57c379932297b130d60f6d86e54c87c8e02507bf0867783e23d7d8f8a50)
      1: failed deriving stack of Some(ChangesetId(Blake2(ac220d3e57adf7c31a869141787d3bc638d79a3f1dd54b0ba54d545c260f14e6))) to Some(ChangesetId(Blake2(3fa05e617e5bd79190a61e16cc23669825b57f36474df1902a63c071998b181d)))
      2: Git submodules not supported
  @  54a6db91baf1 L_A
      file_in_large_repo.txt |  1 +
      1 files changed, 1 insertions(+), 0 deletions(-)
  
  
  
  Running mononoke_admin to verify mapping
  
  RewrittenAs([(ChangesetId(Blake2(ab5bf42dd164f61fa2bcb2de20224d8ffb60f12619bb3692f69d7c171dc1c3be)), CommitSyncConfigVersion("INITIAL_IMPORT_SYNC_CONFIG"))])
  
  Deriving all the enabled derived data types
  Error: * (glob)

  $ REPOIDLARGE=$LARGE_REPO_ID REPOIDSMALL=$SUBMODULE_REPO_ID verify_wc "f299e57c379932297b130d60f6d86e54c87c8e02507bf0867783e23d7d8f8a50"
