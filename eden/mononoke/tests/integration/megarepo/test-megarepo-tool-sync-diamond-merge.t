# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > rebase=
  > pushrebase=
  > EOF

setup configuration

  $ REPOTYPE="blob_files"
  $ REPOID=0 REPONAME=meg setup_common_config $REPOTYPE
  $ REPOID=1 REPONAME=with_merge setup_common_config $REPOTYPE
  $ REPOID=2 REPONAME=another setup_common_config $REPOTYPE
  $ setup_commitsyncmap
  $ setup_configerator_configs
  $ setconfig remotenames.selectivepulldefault=master_bookmark,with_merge_master,with_merge_pre_big_merge,merge_with_preserved,another_master

  $ cd "$TESTTMP"
  $ hginit_treemanifest with_merge
  $ cd with_merge
  $ echo 1 > somefilebeforemerge
  $ hg add somefilebeforemerge
  $ hg ci -m 'first commit in small repo with merge'
  $ hg book -i -r . with_merge_master
  $ echo 2 > someotherfilebeforemerge
  $ hg add someotherfilebeforemerge
  $ hg ci -m "commit, supposed to be preserved"
  $ hg book -ir . with_merge_pre_big_merge
  $ hg up with_merge_master -q

  $ cd "$TESTTMP"
  $ hginit_treemanifest another
  $ cd another
  $ echo 1 > file.txt
  $ hg add file.txt
  $ hg ci -m 'first commit in another small repo'
  $ hg book -r . another_master

Setup client repos
  $ cd "$TESTTMP"
  $ hg clone -q mono:with_merge with_merge_hg --noupdate
  $ hg clone -q mono:another another_hg --noupdate
  $ hg clone -q mono:with_merge meg_hg --noupdate

blobimport hg servers repos into Mononoke repos
  $ cd "$TESTTMP"
  $ REPOID=0 blobimport with_merge/.hg meg
  $ REPOID=0 blobimport another/.hg meg --no-create

  $ REPOID=1 blobimport with_merge/.hg with_merge
  $ REPOID=2 blobimport another/.hg another

  $ export COMMIT_DATE="1985-09-04T00:00:00.00Z"
move things in small repo with merge
  $ quiet mononoke_admin megarepo move-commit --repo-id 0 --source-repo-id 1 \
  > -B with_merge_master -a user -m "with merge move" --mark-public \
  > --commit-date-rfc3339 "$COMMIT_DATE" --set-bookmark with_merge_move \
  > --mapping-version-name TEST_VERSION_NAME

move things in another small repo
  $ quiet mononoke_admin megarepo move-commit --repo-id 0 --source-repo-id 2 \
  > -B another_master -a user -m "another move" --mark-public \
  > --commit-date-rfc3339 "$COMMIT_DATE" --set-bookmark another_move \
  > --mapping-version-name TEST_VERSION_NAME

merge things in both repos
  $ mononoke_admin megarepo merge --repo-id 0 -B with_merge_move -B another_move -a user \
  > -m "megarepo merge" --mark-public --commit-date-rfc3339 "$COMMIT_DATE" \
  > --set-bookmark master_bookmark &> /dev/null

start mononoke server
  $ start_and_wait_for_mononoke_server
Record current master and the first commit in the preserved stack
  $ WITH_MERGE_PRE_MERGE_PRESERVED=$(mononoke_admin bookmarks --repo-id 1 get with_merge_pre_big_merge)
  $ WITH_MERGE_C1=$(mononoke_admin bookmarks --repo-id 1 get with_merge_master)

Create marker commits, so that we don't have to add $WITH_MERGE_C1 and $MEGAREPO_MERGE to the mapping
(as it's not correct: $WITH_MERGE_C1 is supposed to be preserved)
  $ cd "$TESTTMP/with_merge_hg"
  $ hg pull -q
  $ hg up -q with_merge_master
  $ hg ci -m "marker commit" --config ui.allowemptycommit=True
  $ hg push -r . --to with_merge_master -q
  $ WITH_MERGE_MARKER=$(mononoke_admin bookmarks --repo-id 1 get with_merge_master)

  $ cd "$TESTTMP/meg_hg"
  $ setconfig paths.default=mono:meg
  $ hg pull -q
  $ hg up -q master_bookmark
  $ hg ci -m "marker commit" --config ui.allowemptycommit=True
  $ hg push -r . --to master_bookmark -q
  $ MEGAREPO_MARKER=$(mononoke_admin bookmarks --repo-id 0 get master_bookmark)

insert sync mapping entry
  $ ANOTHER_C1=$(mononoke_admin bookmarks --repo-id 2 get another_master)
  $ MEGAREPO_MERGE=$(mononoke_admin bookmarks --repo-id 0 get master_bookmark)
  $ add_synced_commit_mapping_entry 2 $ANOTHER_C1 0 $MEGAREPO_MERGE TEST_VERSION_NAME
  $ add_synced_commit_mapping_entry 1 $WITH_MERGE_MARKER 0 $MEGAREPO_MARKER TEST_VERSION_NAME

Preserve commits from with_merge
  $ add_synced_commit_mapping_entry 1 $WITH_MERGE_C1 0 $WITH_MERGE_C1 TEST_VERSION_NAME
  $ add_synced_commit_mapping_entry 1 $WITH_MERGE_PRE_MERGE_PRESERVED 0 $WITH_MERGE_PRE_MERGE_PRESERVED TEST_VERSION_NAME

Do a test pull
  $ cd "$TESTTMP"/meg_hg
  $ hg pull -q
  $ hg up -q master_bookmark
  $ ls
  arvr-legacy
  somefilebeforemerge
  $ ls arvr-legacy
  file.txt

Create a branch merge in a small repo
  $ cd "$TESTTMP"/with_merge_hg
  $ drawdag <<'EOF'
  >   D
  >   |\
  >   | C
  >   | |
  >   Y B
  >   |/
  >   A
  > EOF
  $ hg rebase -s $A -d with_merge_master -q

  $ hg log -r 'sort(all(),topo)' -G -T '{node|short}'
  o    62dba675d1b3
  ├─╮
  │ o  be5140c7bfcc
  │ │
  │ o  7a7632995e68
  │ │
  o │  23aa3f5a6de2
  ├─╯
  o  68360e2c98f0
  │
  @  a35acba7f331
  │
  │ o  9aaf98d9f7d2
  ├─╯
  o  2fa76efd599c
  


  $ hg log -r 68360e2c98f0
  commit:      68360e2c98f0
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     A
  


  $ cd "$TESTTMP"/with_merge_hg
  $ hg up -q tip
  $ ls -R
  .:
  A
  B
  C
  Y
  somefilebeforemerge

Push a single premerge commit and sync it to megarepo
  $ hg push -r 68360e2c98f0 --to with_merge_master -q
  $ mononoke_x_repo_sync 1 0 once --target-bookmark master_bookmark -B with_merge_master  &> /dev/null

Push a commit from another small repo that modifies existing file
  $ cd "$TESTTMP"/another_hg
  $ hg up -q another_master
  $ echo 2 > file.txt
  $ hg ci -m 'modify file.txt'
  $ hg push -r . --to another_master -q

  $ mononoke_x_repo_sync 2 0 once --target-bookmark master_bookmark -B another_master  &> /dev/null

  $ cd "$TESTTMP"/with_merge_hg
Push and sync commits before a diamond commit
  $ hg push -r 7a7632995e68 --to with_merge_master -q
  $ mononoke_x_repo_sync 1 0 once --target-bookmark master_bookmark -B with_merge_master  &> /dev/null
  $ hg push -r be5140c7bfcc --to with_merge_master -q
  $ mononoke_x_repo_sync 1 0 once --target-bookmark master_bookmark -B with_merge_master  &> /dev/null

Push one more commit from another small repo
  $ cd "$TESTTMP"/another_hg
  $ hg up -q another_master
  $ echo 3 > file.txt
  $ hg ci -m 'second modification of file.txt'
  $ hg push -r . --to another_master -q

  $ mononoke_x_repo_sync 2 0 once --target-bookmark master_bookmark -B another_master  &> /dev/null

Push diamond commit
  $ cd "$TESTTMP"/with_merge_hg
  $ hg log -r 62dba675d1b3 -T '{p1node|short} {p2node|short}'
  be5140c7bfcc 23aa3f5a6de2 (no-eol)
  $ hg push -r 62dba675d1b3 --to with_merge_master -q &> /dev/null

Try to sync it automatically, it's expected to fail
  $ mononoke_x_repo_sync 1 0 once --target-bookmark master_bookmark -B with_merge_master 2>&1 | grep 'unsupported merge'
  * unsupported merge - only merges of new repos are supported (glob)

Now sync with the tool
  $ cd "$TESTTMP"

  $ mononoke_admin megarepo sync-diamond-merge --bookmark with_merge_master --source-repo-id 1 --target-repo-id 0 --onto-bookmark master_bookmark |& grep -v "using repo"
  * changeset resolved as: ChangesetId(Blake2(46c0f70c6300f4168cb70321839ac0079c74b6d3295adb81eeb1932be4f80e9d)) (glob)
  * Preparing to sync a merge commit 46c0f70c6300f4168cb70321839ac0079c74b6d3295adb81eeb1932be4f80e9d... (glob)
  * 1 new commits are going to be merged in (glob)
  * syncing commit from new branch 0feeed653ec98bb533a2ad7fc8940ce07c4105326f07b20fcc68ebac0607abf2 (glob)
  * uploading merge commit 39f339283c1910b404b40271e69e72e9de2a962e903ce9d4fe9f4488f5896242 (glob)
  * It is recommended to run 'mononoke_admin cross-repo verify-working-copy' for 39f339283c1910b404b40271e69e72e9de2a962e903ce9d4fe9f4488f5896242! (glob)
-- a mapping should've been created for the synced merge commit
  $ mononoke_admin cross-repo --source-repo-id 0 --target-repo-id 1 map -B master_bookmark |& grep -v "using repo"
  RewrittenAs([(ChangesetId(Blake2(46c0f70c6300f4168cb70321839ac0079c74b6d3295adb81eeb1932be4f80e9d)), CommitSyncConfigVersion("TEST_VERSION_NAME"))])
  $ flush_mononoke_bookmarks


Pull from megarepo
  $ cd "$TESTTMP"/meg_hg
  $ hg pull -q
  $ hg up -q master_bookmark
  $ ls -R
  .:
  A
  B
  C
  Y
  arvr-legacy
  somefilebeforemerge
  
  ./arvr-legacy:
  file.txt


  $ cat arvr-legacy/file.txt
  3

Make sure that we have correct parents
  $ hg log -r 'parents(master_bookmark)' -T '{node} {desc}\n'
  5d847b3916ac8084ef15846268fb0d9a25d35406 Y
  5ce263ccda875529fde8209141ceaded95b95e68 second modification of file.txt

Merge with preserved ancestors
  $ cd "$TESTTMP"/with_merge_hg

-- check the mapping for p2's parent
  $ mononoke_admin cross-repo --source-repo-id 1 --target-repo-id 0 map -i $(hg log -T "{node}" -r with_merge_pre_big_merge)
  RewrittenAs([(ChangesetId(Blake2(d27a299389c7bedbe3e4dc01b7d4e7ac2162d935401c5d8462b7e1663dfee0e4)), CommitSyncConfigVersion("TEST_VERSION_NAME"))])

-- create a p2, based on a pre-merge commit
  $ hg up with_merge_pre_big_merge -q
  $ echo preserved_pre_big_merge_file > preserved_pre_big_merge_file
  $ hg ci -Aqm "preserved_pre_big_merge_file"
  $ hg book -r . pre_merge_p2

-- create a p1, based on a master
  $ hg up with_merge_master -q
  $ echo ababagalamaga > ababagalamaga
  $ hg ci -Aqm "ababagalamaga"
  $ hg book -r . pre_merge_p1

-- create a merge commit
  $ hg merge pre_merge_p2 -q
  $ hg ci -qm "merge with preserved p2"
  $ hg log -r . -T "{node} {desc}\np1: {p1node}\np2: {p2node}\n"
  18f03e551cee2ec38449f0960a586adcb869cb7a merge with preserved p2
  p1: b5bdb045c12bcaf8b2645438285a4512d7cf823d
  p2: 67d0696c2845433765c450939263a8a128fec229
  $ hg book -r . merge_with_preserved

-- push these folks to the server-side repo
  $ hg push --to with_merge_master 2>&1 | grep updating
  updating bookmark with_merge_master

-- sync p1
  $ cd "$TESTTMP"
  $ mononoke_x_repo_sync 1 0 once --target-bookmark master_bookmark --commit-id $(hg log -T "{node}" -r pre_merge_p1 --cwd "$TESTTMP/with_merge_hg") |& grep -v "using repo"
  * Starting session with id * (glob)
  * Starting up X Repo Sync from small repo with_merge to large repo meg (glob)
  * Syncing 1 commits and all of their unsynced ancestors (glob)
  * Checking if 87924512f63d088d5b6bb5368bfef8016246e59927fe9d06d8ea657bc94e993d is already synced 1->0 (glob)
  * 1 unsynced ancestors of 87924512f63d088d5b6bb5368bfef8016246e59927fe9d06d8ea657bc94e993d (glob)
  * syncing 87924512f63d088d5b6bb5368bfef8016246e59927fe9d06d8ea657bc94e993d via pushrebase for master_bookmark (glob)
  * changeset 87924512f63d088d5b6bb5368bfef8016246e59927fe9d06d8ea657bc94e993d synced as 321d5cb2cf4c5e1bf7cb2e809b3aaf181a0907aa63bc69ee3575a7e6313b92e7 in * (glob)
  * successful sync (glob)
  * X Repo Sync execution finished from small repo with_merge to large repo meg (glob)
  $ mononoke_admin cross-repo --source-repo-id 1 --target-repo-id 0 map -i 87924512f63d088d5b6bb5368bfef8016246e59927fe9d06d8ea657bc94e993d
  RewrittenAs([(ChangesetId(Blake2(321d5cb2cf4c5e1bf7cb2e809b3aaf181a0907aa63bc69ee3575a7e6313b92e7)), CommitSyncConfigVersion("TEST_VERSION_NAME"))])

-- sync the merge
  $ cd "$TESTTMP"
  $ mononoke_admin megarepo sync-diamond-merge --bookmark with_merge_master --source-repo-id 1 --target-repo-id 0 --onto-bookmark master_bookmark
  * using repo "with_merge" repoid RepositoryId(1) (glob)
  * using repo "meg" repoid RepositoryId(0) (glob)
  * changeset resolved as: ChangesetId(Blake2(3f71f093fcfbebcc47c981c847cd80c7d0bf063c5022aba53fab95244e4c4f1c)) (glob)
  * Preparing to sync a merge commit 3f71f093fcfbebcc47c981c847cd80c7d0bf063c5022aba53fab95244e4c4f1c... (glob)
  * 2 new commits are going to be merged in (glob)
  * syncing commit from new branch d27a299389c7bedbe3e4dc01b7d4e7ac2162d935401c5d8462b7e1663dfee0e4 (glob)
  * syncing commit from new branch 89c0603366c60ae4bf8d8dca6da7581c741b7e89a6fcc3f49a44fdd248de3b1d (glob)
  * uploading merge commit 0958bd58a03b8c799664bc0767b095a97003ee41eaa7814343ad9dcc6f90bc16 (glob)
  * It is recommended to run 'mononoke_admin cross-repo verify-working-copy' for 0958bd58a03b8c799664bc0767b095a97003ee41eaa7814343ad9dcc6f90bc16! (glob)

-- check that p2 was synced as preserved (note identical hashes)
  $ mononoke_admin cross-repo --source-repo-id 1 --target-repo-id 0 map -i $(hg log -r pre_merge_p2 -T "{node}" --cwd "$TESTTMP/with_merge_hg")
  RewrittenAs([(ChangesetId(Blake2(89c0603366c60ae4bf8d8dca6da7581c741b7e89a6fcc3f49a44fdd248de3b1d)), CommitSyncConfigVersion("TEST_VERSION_NAME"))])

-- check that merge was synced
  $ mononoke_admin cross-repo --source-repo-id 1 --target-repo-id 0 map -B with_merge_master
  RewrittenAs([(ChangesetId(Blake2(0958bd58a03b8c799664bc0767b095a97003ee41eaa7814343ad9dcc6f90bc16)), CommitSyncConfigVersion("TEST_VERSION_NAME"))])

--verify the working copy
  $ mononoke_admin cross-repo --source-repo-name with_merge --target-repo-name meg verify-working-copy $(mononoke_admin bookmarks -R meg get master_bookmark)
  * target repo cs id: 3f71f093fcfbebcc47c981c847cd80c7d0bf063c5022aba53fab95244e4c4f1c, mapping version: TEST_VERSION_NAME (glob)
  * ### (glob)
  * ### Checking that all the paths from the repo meg are properly rewritten to with_merge (glob)
  * ### (glob)
  * ### (glob)
  * ### Checking that all the paths from the repo with_merge are properly rewritten to meg (glob)
  * ### (glob)
  * all is well! (glob)
