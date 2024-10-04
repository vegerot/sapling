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
  $ . "${TEST_FIXTURES}/library-xrepo-sync-with-git-submodules.sh"




Setup configuration
  $ run_common_xrepo_sync_with_gitsubmodules_setup
  L_A=b006a2b1425af8612bc80ff4aa9fa8a1a2c44936ad167dd21cb9af2a9a0248c4

# Test how the initial-import command behaves when it runs again after new
# commits have been added to the small repo.
# EXPECTED: it will only sync the new commits and the ancestry will be correct
# in the large repo.
# NOTE: the initial-import command expects that the commits from the small
# repo HAVE NOT YET BEEN MERGED with the master branch of the large repo.
# After the merge, the live sync command should be used.
Create small repo commits
  $ testtool_drawdag -R "$SUBMODULE_REPO_NAME" --no-default-files <<EOF
  > A-B
  > # modify: A "foo/a.txt" "creating foo directory"
  > # modify: A "bar/b.txt" "creating bar directory"
  > # modify: B "bar/c.txt" "random change"
  > # modify: B "foo/d" "another random change"
  > # bookmark: B master
  > EOF
  A=7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36
  B=2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3

# Ignoring lines with `initializing` or `initialized
  $ with_stripped_logs mononoke_x_repo_sync "$SUBMODULE_REPO_ID" "$LARGE_REPO_ID" --log-level=TRACE \
  > initial-import --no-progress-bar --derivation-batch-size 2 -i "$B" --version-name "$LATEST_CONFIG_VERSION_NAME" | \
  > rg -v "nitializ" | rg -v "derive" | rg -v "Upload" | tee $TESTTMP/initial_import.out
  enabled stdlog with level: Error (set RUST_LOG to configure)
  Starting session with id * (glob)
  Starting up X Repo Sync from small repo small_repo to large repo large_repo
  Checking if 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3 is already synced 11->10
  Syncing 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3 for inital import
  Source repo: small_repo / Target repo: large_repo
  Automatic derivation is enabled
  Found 2 unsynced ancestors
  Unsynced ancestors: [
      ChangesetId(
          Blake2(7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36),
      ),
      ChangesetId(
          Blake2(2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3),
      ),
  ]
  CommitSyncer{11->10}: unsafe_sync_commit called for 7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36, with hint: CandidateSelectionHint::Only
  Ancestor 7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36 synced successfully as ac220d3e57adf7c31a869141787d3bc638d79a3f1dd54b0ba54d545c260f14e6
  Root fsnode id from ac220d3e57adf7c31a869141787d3bc638d79a3f1dd54b0ba54d545c260f14e6: 8a7bd43727f4428740b8bd502c6993ad2e5d81037f83eb0a9cdc74aaef52a03d
  CommitSyncer{11->10}: unsafe_sync_commit called for 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3, with hint: CandidateSelectionHint::Only
  get_commit_sync_outcome_with_hint called for 11->10, cs 7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36, hint CandidateSelectionHint::Only
  Ancestor 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3 synced successfully as 85776cdc88303208a1cde5c614996a89441d3a9175a6311dda34d178428ba652
  Root fsnode id from 85776cdc88303208a1cde5c614996a89441d3a9175a6311dda34d178428ba652: bd7918272cd69f6f7946d62d5dddf4dc8687c11b5399f2b73539ab6c375cad5a
  Finished bulk derivation of 2 changesets
  CommitSyncer{11->10}: unsafe_sync_commit called for 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3, with hint: CandidateSelectionHint::Only
  get_commit_sync_outcome_with_hint called for 11->10, cs 7e97054c51a17ea2c03cd5184826b6a7556d141d57c5a1641bbd62c0854d1a36, hint CandidateSelectionHint::Only
  changeset 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3 synced as 85776cdc88303208a1cde5c614996a89441d3a9175a6311dda34d178428ba652 in * (glob)
  successful sync of head 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3
  X Repo Sync execution finished from small repo small_repo to large repo large_repo


  $ SYNCED_HEAD=$(rg ".+synced as (\w+) .+" -or '$1' "$TESTTMP/initial_import.out")
  $ clone_and_log_large_repo "$SYNCED_HEAD"
  o  5e3f6798b6a3 B
  │   smallrepofolder1/bar/c.txt |  1 +
  │   smallrepofolder1/foo/d     |  1 +
  │   2 files changed, 2 insertions(+), 0 deletions(-)
  │
  o  e462fc947f26 A
      smallrepofolder1/bar/b.txt |  1 +
      smallrepofolder1/foo/a.txt |  1 +
      2 files changed, 2 insertions(+), 0 deletions(-)
  
  @  54a6db91baf1 L_A
      file_in_large_repo.txt |  1 +
      1 files changed, 1 insertions(+), 0 deletions(-)
  
  
  
  Running mononoke_admin to verify mapping
  
  RewrittenAs([(ChangesetId(Blake2(2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3)), CommitSyncConfigVersion("INITIAL_IMPORT_SYNC_CONFIG"))])
  
  Deriving all the enabled derived data types

Add more commits to small repo
  $ testtool_drawdag -R "$SUBMODULE_REPO_NAME" --no-default-files <<EOF
  > B-C-D
  > # exists: B $B
  > # modify: C "bar/b.txt" "more changes"
  > # modify: D "bar/c.txt" "more changes"
  > # bookmark: D master
  > EOF
  B=2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3
  C=9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583
  D=d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a



# Ignoring lines with `initializing` or `initialized
  $ with_stripped_logs mononoke_x_repo_sync "$SUBMODULE_REPO_ID" "$LARGE_REPO_ID" --log-level=TRACE \
  > initial-import --no-progress-bar --derivation-batch-size 2 -i "$D" --version-name "$LATEST_CONFIG_VERSION_NAME" | \
  > rg -v "nitializ" | rg -v "derive" | rg -v "Upload"
  enabled stdlog with level: Error (set RUST_LOG to configure)
  Starting session with id * (glob)
  Starting up X Repo Sync from small repo small_repo to large repo large_repo
  Checking if d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a is already synced 11->10
  Syncing d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a for inital import
  Source repo: small_repo / Target repo: large_repo
  Automatic derivation is enabled
  Found 2 unsynced ancestors
  Unsynced ancestors: [
      ChangesetId(
          Blake2(9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583),
      ),
      ChangesetId(
          Blake2(d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a),
      ),
  ]
  CommitSyncer{11->10}: unsafe_sync_commit called for 9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583, with hint: CandidateSelectionHint::Only
  get_commit_sync_outcome_with_hint called for 11->10, cs 2999dcf517994fe94506b62e5a9c54f851abd4c4964f98fdd701c013abd9c0c3, hint CandidateSelectionHint::Only
  Ancestor 9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583 synced successfully as eee07cc327b80fd172bbbe2933615d1f4685a3a032eed0fc52c02c01e8f49c42
  Root fsnode id from eee07cc327b80fd172bbbe2933615d1f4685a3a032eed0fc52c02c01e8f49c42: 64a2b572a34a75970856970b60d6b56bffd10f377736e2b15d14957b710878eb
  CommitSyncer{11->10}: unsafe_sync_commit called for d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a, with hint: CandidateSelectionHint::Only
  get_commit_sync_outcome_with_hint called for 11->10, cs 9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583, hint CandidateSelectionHint::Only
  Ancestor d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a synced successfully as ccfdf094e4710a77de7b36c4324fa7ee64dafba4067726e383db62273553466b
  Root fsnode id from ccfdf094e4710a77de7b36c4324fa7ee64dafba4067726e383db62273553466b: 7e4e5c99dcb5cfc12e6729bf8a6bac22884d21d2ba1de5d4c00563229863053f
  Finished bulk derivation of 2 changesets
  CommitSyncer{11->10}: unsafe_sync_commit called for d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a, with hint: CandidateSelectionHint::Only
  get_commit_sync_outcome_with_hint called for 11->10, cs 9eeb57261a4dfbeeb2e1c06ef6dc3f83b11e314eb34c598f2d042967b1938583, hint CandidateSelectionHint::Only
  changeset d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a synced as ccfdf094e4710a77de7b36c4324fa7ee64dafba4067726e383db62273553466b in * (glob)
  successful sync of head d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a
  X Repo Sync execution finished from small repo small_repo to large repo large_repo

  $ clone_and_log_large_repo "ccfdf094e4710a77de7b36c4324fa7ee64dafba4067726e383db62273553466b"
  abort: destination 'large_repo' is not empty
  o  71fdac6141e7 D
  │   smallrepofolder1/bar/c.txt |  2 +-
  │   1 files changed, 1 insertions(+), 1 deletions(-)
  │
  o  368fd13402ee C
  │   smallrepofolder1/bar/b.txt |  2 +-
  │   1 files changed, 1 insertions(+), 1 deletions(-)
  │
  o  5e3f6798b6a3 B
  │   smallrepofolder1/bar/c.txt |  1 +
  │   smallrepofolder1/foo/d     |  1 +
  │   2 files changed, 2 insertions(+), 0 deletions(-)
  │
  o  e462fc947f26 A
      smallrepofolder1/bar/b.txt |  1 +
      smallrepofolder1/foo/a.txt |  1 +
      2 files changed, 2 insertions(+), 0 deletions(-)
  
  @  54a6db91baf1 L_A
      file_in_large_repo.txt |  1 +
      1 files changed, 1 insertions(+), 0 deletions(-)
  
  
  
  Running mononoke_admin to verify mapping
  
  RewrittenAs([(ChangesetId(Blake2(d2ba11302a912b679610fd60d7e56dd8f01372c130faa3ae72816d5568b25f3a)), CommitSyncConfigVersion("INITIAL_IMPORT_SYNC_CONFIG"))])
  
  Deriving all the enabled derived data types
