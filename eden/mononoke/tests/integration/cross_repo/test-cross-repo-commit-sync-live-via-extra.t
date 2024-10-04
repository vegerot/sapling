# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

This is a fork of test-cross-repo-commit-sync-live.t that brings the via-extra mode
to be fully able to deal with mapping changes regardless of sync direction. I will
replace that file once fully fixed.
  $ export LARGE_REPO_ID=0
  $ export SMALL_REPO_ID=1
  $ . "${TEST_FIXTURES}/library.sh"
  $ . "${TEST_FIXTURES}/library-push-redirector.sh"

  $ merge_just_knobs <<EOF
  > {
  >   "bools": {
  >     "scm/mononoke:cross_repo_skip_backsyncing_ordinary_empty_commits": true
  >   }
  > }
  > EOF

-- Init Mononoke thingies
  $ create_large_small_repo
  Adding synced mapping entry
  $ setup_configerator_configs
  $ enable_pushredirect 1 false false
  $ XREPOSYNC=1 start_large_small_repo
  Starting Mononoke server
  $ init_local_large_small_clones

-- Start up the sync job in the background
  $ mononoke_x_repo_sync_forever $REPOIDSMALL $REPOIDLARGE

Before the change
-- push to a small repo
  $ cd "$TESTTMP/small-hg-client"
  $ hg up -q master_bookmark
  $ mkdir -p non_path_shifting
  $ echo a > foo
  $ echo b > non_path_shifting/bar
  $ hg ci -Aqm "before config change"
  $ hg push -r . --to master_bookmark -q
  $ log
  @  before config change [public;rev=2;bc6a206054d0] default/master_bookmark
  │
  o  first post-move commit [public;rev=1;11f848659bfc]
  │
  o  pre-move commit [public;rev=0;fc7ae591de0e]
  $

-- wait a little to give sync job some time to catch up
  $ wait_for_xrepo_sync 2
  $ flush_mononoke_bookmarks

-- check the same commit in the large repo
  $ cd "$TESTTMP/large-hg-client"
  $ hg pull -q
  $ hg up -q master_bookmark
  $ log -r master_bookmark
  @  before config change [public;rev=3;c76f6510b5c1] default/master_bookmark
  │
  ~
  $ hg log -r master_bookmark -T "{files % '{file}\n'}"
  non_path_shifting/bar
  smallrepofolder/foo
-- prepare for config change by making the state match both old and new config versions
  $ hg cp -q smallrepofolder smallrepofolder_after
  $ hg commit -m "prepare for config change"
  $ hg push -q --to master_bookmark

Make a config change
  $ update_commit_sync_map_first_option
  $ MONONOKE_ADMIN_ALWAYS_ALLOW_MAPPING_CHANGE_VIA_EXTRA=1 \
  > quiet mononoke_admin_source_target $REPOIDLARGE $REPOIDSMALL crossrepo pushredirection change-mapping-version \
  > --author author \
  > --large-repo-bookmark master_bookmark \
  > --via-extra \
  > --date 2002-10-02T21:38:00-05:00 \
  > --version-name new_version
  $ flush_mononoke_bookmarks
Find the hash of mapping change commit in the large repo
  $ cd "$TESTTMP/large-hg-client"
  $ hg pull -q
  $ hg up -q master_bookmark

After the change
-- an empty, mapping changing commit from large repo shouldn't be backsynced when forward sync is on
  $ X=$(x_repo_lookup large-mon small-mon "$(hg whereami)")
  $ with_stripped_logs mononoke_admin_source_target 0 1 crossrepo map $(hg whereami)
  using repo "large-mon" repoid RepositoryId(0)
  using repo "small-mon" repoid RepositoryId(1)
  changeset resolved as: ChangesetId(Blake2(a45c6ed3a8522811955be9b4eb0b80f29d2229eeeb43f7f017b2411c0feab955))
  EquivalentWorkingCopyAncestor(ChangesetId(Blake2(cdd50b2d186ce87fe6d2428b01caf9994a98ac51e65f7d6bb43c6a0f6e8d7a56)), CommitSyncConfigVersion("new_version"))

  $ cd "$TESTTMP/small-hg-client"
  $ hg pull -r $X
  pulling from mono:small-mon
  no changes found
  adding changesets
  adding manifests
  adding file changes
  $ hg up -q $X
  $ log -r .^::.
  @  before config change [public;rev=2;bc6a206054d0] default/master_bookmark
  │
  o  first post-move commit [public;rev=1;11f848659bfc]
  │
  ~

-- push to a small repo
  $ cd "$TESTTMP/small-hg-client"
  $ hg pull -q
  $ hg up -q master_bookmark
  $ echo a > boo
  $ echo b > non_path_shifting/baz
  $ hg ci -Aqm "after config change from small"
  $ hg push -r . --to master_bookmark -q
  $ log -r master_bookmark^::master_bookmark
  @  after config change from small [public;rev=3;6bfa38885cea] default/master_bookmark
  │
  o  before config change [public;rev=2;bc6a206054d0] (glob)
  │
  ~

-- push to a large repo
  $ cd "$TESTTMP/large-hg-client"
  $ hg pull -q
  $ hg up -q master_bookmark
  $ echo a > after_change
  $ hg ci -Aqm "after config change from large"
  $ hg push -r . --to master_bookmark -q

-- trigger xrepo sync and show that can sync commit over the config change
  $ with_stripped_logs wait_for_xrepo_sync 3
Rest of this test won't pass as we failed the previous command so is commented out.
  $ flush_mononoke_bookmarks
-- check the same commit in the large repo
  $ cd "$TESTTMP/large-hg-client"
  $ hg pull -q
  $ hg up -q master_bookmark
  $ log -r "master_bookmark^::master_bookmark"
  @  after config change from large [public;rev=?;ad029e9c7735] default/master_bookmark (glob)
  │
  o  after config change from small [public;rev=?;9a1a082f2f8e] (glob)
  │
  ~
  $ hg log -r master_bookmark -T "{files % '{file}\n'}"
  after_change
-- Verify the working copy state after the operation
  $ with_stripped_logs verify_wc $(hg whereami)

-- Show the list of files in the repo after the operation
  $ hg files
  after_change
  non_path_shifting/bar
  non_path_shifting/baz
  smallrepofolder/file.txt
  smallrepofolder/filetoremove
  smallrepofolder/foo
  smallrepofolder_after/boo
  smallrepofolder_after/file.txt
  smallrepofolder_after/filetoremove
  smallrepofolder_after/foo

-- Show the actual mapping version used for rewriting of small repo change
  $ with_stripped_logs mononoke_admin_source_target 0 1 crossrepo map $(hg log -T "{node}" -r .^)
  using repo "large-mon" repoid RepositoryId(0)
  using repo "small-mon" repoid RepositoryId(1)
  changeset resolved as: ChangesetId(Blake2(*)) (glob)
  RewrittenAs([(ChangesetId(Blake2(e7a0827177ac9caf3578f2c5e4307f3d11a8954ccaa576c3813f166d174f4e64)), CommitSyncConfigVersion("new_version"))])
