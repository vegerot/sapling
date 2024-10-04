# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library-push-redirector.sh"
  $ export COMMIT_SCRIBE_CATEGORY=mononoke_commits
  $ export BOOKMARK_SCRIBE_CATEGORY=mononoke_bookmark

  $ create_large_small_repo
  Adding synced mapping entry
  $ setup_configerator_configs
  $ enable_pushredirect 1
  $ start_large_small_repo
  Starting Mononoke server
  $ init_local_large_small_clones

Push a merge from a large repo
  $ cd "$TESTTMP/large-hg-client"
  $ hg update null
  0 files updated, 0 files merged, 2 files removed, 0 files unresolved
  $ mkdir smallrepofolder/
  $ echo 1 > smallrepofolder/newrepo
  $ hg addremove -q
  $ hg ci -m "newrepo"
  $ NODE="$(hg log -r . -T '{node}')"
  $ hg up -q master_bookmark^
  $ hg merge -r "$NODE" -q
  $ hg ci -m 'merge commit from large repo'
  $ hg push -r . --to master_bookmark -q

Push a merge that will not add any new files to the small repo
  $ hg up null
  0 files updated, 0 files merged, 2 files removed, 0 files unresolved
  $ mkdir someotherrepo/
  $ echo 1 > someotherrepo/newrepo
  $ hg addremove -q
  $ hg ci -m "second newrepo"
  $ NODE="$(hg log -r . -T '{node}')"
  $ hg up -q master_bookmark
  $ hg merge -r "$NODE" -q
  $ hg ci -m 'merge commit no new files'
  $ hg push -r . --to master_bookmark -q

Backsync to a small repo
  $ backsync_large_to_small 2>&1 | grep "syncing bookmark"
  * syncing bookmark master_bookmark to * (glob)
  * syncing bookmark master_bookmark to * (glob)
  $ flush_mononoke_bookmarks

Pull from a small repo. Check that both merges are synced
although the second one became non-merge commit
  $ cd "$TESTTMP/small-hg-client"
  $ hg pull -q
  $ log -r :
  o  merge commit no new files [public;rev=4;534a740cd266] default/master_bookmark
  │
  o    merge commit from large repo [public;rev=3;246c2e616e99]
  ├─╮
  │ o  newrepo [public;rev=2;64d197011743]
  │
  @  first post-move commit [public;rev=1;11f848659bfc]
  │
  o  pre-move commit [public;rev=0;fc7ae591de0e]
  $
  $ hg up -q master_bookmark
  $ hg show master_bookmark
  commit:      534a740cd266
  bookmark:    default/master_bookmark
  hoistedname: master_bookmark
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  description:
  merge commit no new files
  
  
  




Make sure we have directory from the first move, but not from the second
  $ ls
  file.txt
  filetoremove
  newrepo
  $ cat "$TESTTMP/scribe_logs/$COMMIT_SCRIBE_CATEGORY" | jq --compact-output '[.repo_name, .changeset_id, .bookmark, .is_public]' | sort
  ["large-mon","097273394f9ce820e56caaadbea092b73e7639666b63294e098a311e33af04c5","master_bookmark",true]
  ["large-mon","2e4090631ddfa0b3a2fe26c5d2560c615ebc2b77533e6a2039afcfbd3424c3ac","master_bookmark",true]
  ["large-mon","3c4cdb0a6b145deb53f79178ba48c6fd1316058982bf6ab618402b91901de75e","master_bookmark",true]
  ["large-mon","71634714290726837d0f66eff98add40cd621b0aa745b5bfc587cbc89b2fc94f","master_bookmark",true]
  ["small-mon","77970017bd96dfbfd8b2cf217420bd45c55b6f5ad0073d19db9025e30367ab9f","master_bookmark",true]
  ["small-mon","a353e74b22a1347ee50cd20a8b9a916f213f7c8e4148dce66c5c2a5c273abf5c","master_bookmark",true]
  ["small-mon","dfbd8e50164e761bbf2f8ecedbc9e8cf8641cb6e4679fb0487f48311c01ab0a5","master_bookmark",true]
