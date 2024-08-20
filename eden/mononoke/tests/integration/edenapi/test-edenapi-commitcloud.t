# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.


# Commit Cloud Test with as much Edenapi integration as possible
# the most important parts are:
#    pull.httpbookmarks    ENABLED
#    pull.httpcommitgraph2    ENABLED
#    pull.httphashprefix    ENABLED
#    pull.httpmutation      ENABLED
#    exchange.httpcommitlookup    ENABLED
# Sync of remote bookmarks is also enabled in this test

  $ . "${TEST_FIXTURES}/library.sh"
  $ configure modern
  $ cat >> "$ACL_FILE" << ACLS
  > {
  >   "repos": {
  >     "repo": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "bypass_readonly": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"]
  >       }
  >     }
  >   },
  >  "workspaces": {
  >     "repo/user/test/default": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"]
  >       }
  >     }
  >   }
  > }
  > ACLS
  $ setconfig ui.ignorerevnum=false
  $ setconfig pull.httpcommitgraph2=true pull.use-commit-graph=true clone.use-rust=true clone.use-commit-graph=true
  $ setconfig remotenames.selectivepull=True remotenames.selectivepulldefault=master

setup custom smartlog
  $ function smartlog {
  >  sl log -G -T "{node|short} {phase} '{desc|firstline}' {bookmarks} {remotebookmarks} {join(mutations % '(Rewritten using {operation} into {join(successors % \'{node|short}\', \', \')})', ' ')}" "$@"
  > }

setup configuration
  $ export READ_ONLY_REPO=1
  $ export ACL_NAME=repo
  $ export LOG=pull
  $ INFINITEPUSH_ALLOW_WRITES=true \
  >   setup_common_config
  $ cd $TESTTMP

setup common configuration for these tests
  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > amend =
  > commitcloud =
  > infinitepush =
  > rebase =
  > remotenames =
  > share =
  > [remotefilelog]
  > reponame=repo
  > [infinitepush]
  > server=False
  > httpbookmarks=true
  > [visibility]
  > enabled = true
  > [mutation]
  > record = true
  > enabled = true
  > date = 0 0
  > [pull]
  > httphashprefix = true
  > httpbookmarks = true
  > [exchange]
  > httpcommitlookup = true
  > EOF

#testcases http edenapi

#if http
  $ cat >> $HGRCPATH <<EOF
  > [commitcloud]
  > servicetype = local
  > EOF
#else 
  $ cat >> $HGRCPATH <<EOF
  > [commitcloud]
  > servicetype = edenapi
  > fallback = local
  > EOF
#endif

  $ cat >> $HGRCPATH <<EOF
  > hostname = testhost
  > servicelocation = $TESTTMP
  > owner_team = The Test Team
  > updateonmove = true
  > usehttpupload = true
  > remotebookmarkssync = true
  > EOF

setup repo

  $ hginit_treemanifest repo
  $ cd repo
  $ mkcommit "base_commit"
  $ hg log -T '{short(node)}\n'
  8b2dca0c8a72
  $ hg bookmark master -r tip

Import and start mononoke
  $ cd $TESTTMP
  $ blobimport repo/.hg repo
  $ mononoke
  $ wait_for_mononoke

Clone 1 and 2
  $ sl clone "mononoke://$(mononoke_address)/repo" client1 -q
  $ sl clone "mononoke://$(mononoke_address)/repo" client2 -q

Connect client 1 and client 2 to Commit Cloud
  $ cd client1
  $ sl cloud join -q
  $ sl up master -q

  $ cd ..

  $ cd client2
  $ sl cloud join -q
  $ sl up master -q


Make commits in the first client, and sync it
  $ cd ../client1
  $ mkcommitedenapi "A"
  $ mkcommitedenapi "B"
  $ mkcommitedenapi "C"
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: head 'c4f3cf0b6f49' hasn't been uploaded yet
  edenapi: queue 3 commits for upload
  edenapi: queue 3 files for upload
  edenapi: uploaded 3 files
  edenapi: queue 3 trees for upload
  edenapi: uploaded 3 trees
  edenapi: uploaded 3 changesets
  commitcloud: commits synchronized
  finished in * (glob)

  $ smartlog
  @  c4f3cf0b6f49 draft 'C'
  │
  o  e3133a4a05d5 draft 'B'
  │
  o  929f2b9071cf draft 'A'
  │
  o  8b2dca0c8a72 public 'base_commit'  remote/master
  

Sync from the second client - the commits should appear
  $ cd ../client2
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  pulling c4f3cf0b6f49 from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  DEBUG pull::httpgraph: edenapi fetched 3 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 3 draft commits
  commitcloud: commits synchronized
  finished in * (glob)

  $ smartlog
  o  c4f3cf0b6f49 draft 'C'
  │
  o  e3133a4a05d5 draft 'B'
  │
  o  929f2b9071cf draft 'A'
  │
  @  8b2dca0c8a72 public 'base_commit'  remote/master
  


Make commits from the second client and sync it
  $ mkcommitedenapi "D"
  $ mkcommitedenapi "E"
  $ mkcommitedenapi "F"
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: head 'c981069f3f05' hasn't been uploaded yet
  edenapi: queue 3 commits for upload
  edenapi: queue 3 files for upload
  edenapi: uploaded 3 files
  edenapi: queue 3 trees for upload
  edenapi: uploaded 3 trees
  edenapi: uploaded 3 changesets
  commitcloud: commits synchronized
  finished in * (glob)


On the first client, make a bookmark, then sync - the bookmark and the new commits should be synced
  $ cd ../client1
  $ sl bookmark -r "min(all())" new_bookmark
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  pulling c981069f3f05 from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  DEBUG pull::httpgraph: edenapi fetched 3 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 3 draft commits
  commitcloud: commits synchronized
  finished in * (glob)

  $ smartlog
  o  c981069f3f05 draft 'F'
  │
  o  5267c897028e draft 'E'
  │
  o  4594cad5305d draft 'D'
  │
  │ @  c4f3cf0b6f49 draft 'C'
  │ │
  │ o  e3133a4a05d5 draft 'B'
  │ │
  │ o  929f2b9071cf draft 'A'
  ├─╯
  o  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  
#if edenapi
  $ sl cloud sl
  commitcloud: searching draft commits for the 'user/test/default' workspace for the 'repo' repo
  Smartlog:
  
    o  commit:      c981069f3f05
    │  user:        test
    │  date:        Thu Jan 01 00:00:00 1970 +0000
    │  summary:     F
    │
    o  commit:      5267c897028e
    │  user:        test
    │  date:        Thu Jan 01 00:00:00 1970 +0000
    │  summary:     E
    │
    o  commit:      4594cad5305d
  ╭─╯  user:        test
  │    date:        Thu Jan 01 00:00:00 1970 +0000
  │    summary:     D
  │
  │ @  commit:      c4f3cf0b6f49
  │ │  user:        test
  │ │  date:        Thu Jan 01 00:00:00 1970 +0000
  │ │  summary:     C
  │ │
  │ o  commit:      e3133a4a05d5
  │ │  user:        test
  │ │  date:        Thu Jan 01 00:00:00 1970 +0000
  │ │  summary:     B
  │ │
  │ o  commit:      929f2b9071cf
  ├─╯  user:        test
  │    date:        Thu Jan 01 00:00:00 1970 +0000
  │    summary:     A
  │
  o  commit:      8b2dca0c8a72
     bookmark:    new_bookmark
     bookmark:    remote/master
     hoistedname: master
     user:        test
     date:        Thu Jan 01 00:00:00 1970 +0000
     summary:     base_commit
  

#endif

On the first client rebase the stack
  $ sl rebase -s 4594cad5305d -d c4f3cf0b6f49
  rebasing 4594cad5305d "D"
  rebasing 5267c897028e "E"
  rebasing c981069f3f05 "F"
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: head 'f5aa28a22f7b' hasn't been uploaded yet
  edenapi: queue 3 commits for upload
  edenapi: queue 0 files for upload
  edenapi: queue 3 trees for upload
  edenapi: uploaded 3 trees
  edenapi: uploaded 3 changesets
  commitcloud: commits synchronized
  finished in * (glob)


On the second client sync it
  $ cd ../client2
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  pulling f5aa28a22f7b from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  DEBUG pull::httpgraph: edenapi fetched 3 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 3 draft commits
  commitcloud: commits synchronized
  finished in * (glob)
  commitcloud: current revision c981069f3f05 has been moved remotely to f5aa28a22f7b
  updating to f5aa28a22f7b
  3 files updated, 0 files merged, 0 files removed, 0 files unresolved

  $ smartlog
  @  f5aa28a22f7b draft 'F'
  │
  o  8da26d088b8f draft 'E'
  │
  o  d79a28423f14 draft 'D'
  │
  o  c4f3cf0b6f49 draft 'C'
  │
  o  e3133a4a05d5 draft 'B'
  │
  o  929f2b9071cf draft 'A'
  │
  o  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  

Check mutation markers
  $ sl up c981069f3f05 -q
  $ smartlog
  o  f5aa28a22f7b draft 'F'
  │
  o  8da26d088b8f draft 'E'
  │
  o  d79a28423f14 draft 'D'
  │
  │ @  c981069f3f05 draft 'F'   (Rewritten using rebase into f5aa28a22f7b)
  │ │
  │ x  5267c897028e draft 'E'   (Rewritten using rebase into 8da26d088b8f)
  │ │
  │ x  4594cad5305d draft 'D'   (Rewritten using rebase into d79a28423f14)
  │ │
  o │  c4f3cf0b6f49 draft 'C'
  │ │
  o │  e3133a4a05d5 draft 'B'
  │ │
  o │  929f2b9071cf draft 'A'
  ├─╯
  o  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  


On the second client hide all draft commits
  $ sl hide -r 'draft()'
  hiding commit 929f2b9071cf "A"
  hiding commit e3133a4a05d5 "B"
  hiding commit c4f3cf0b6f49 "C"
  hiding commit 4594cad5305d "D"
  hiding commit 5267c897028e "E"
  hiding commit c981069f3f05 "F"
  hiding commit d79a28423f14 "D"
  hiding commit 8da26d088b8f "E"
  hiding commit f5aa28a22f7b "F"
  0 files updated, 0 files merged, 3 files removed, 0 files unresolved
  working directory now at 8b2dca0c8a72
  9 changesets hidden
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: commits synchronized
  finished in * (glob)
  $ sl up master -q

  $ smartlog
  @  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  


On the first client check that all commits were hidden
  $ cd ../client1
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: commits synchronized
  finished in * (glob)
  $ sl up master -q

  $ smartlog
  @  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  

Test sync of remote bookmarks.
Create two "expensive" remote bookmarks and another regular remote bookmark at the first client and push those. Create couple of draft commits as well.
Sync on the first client, sync on the second client.
The purpose of the test is to check syncing of remote bookmarks and to verify that expensive bookmarks are pulled separately (prefetched).
  $ mkcommitedenapi e1
  $ mkcommitedenapi e2
  $ sl push -r . --to expensive --force --create --pushvars "BYPASS_READONLY=true"
  pushing rev 98eac947fc54 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark expensive
  searching for changes
  exporting bookmark expensive
  $ sl up master -q
  $ mkcommitedenapi e3
  $ mkcommitedenapi e4
  $ sl push -r . --to expensive_other --force --create --pushvars "BYPASS_READONLY=true"
  pushing rev 8537bcdeff72 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark expensive_other
  searching for changes
  exporting bookmark expensive_other

  $ mkcommitedenapi e_draft

  $ sl up master -q
  $ mkcommitedenapi o1
  $ mkcommitedenapi o2
  $ sl push -r . --to regular --force --create --pushvars "BYPASS_READONLY=true"
  pushing rev 22f66edbeb8e to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark regular
  searching for changes
  exporting bookmark regular

  $ mkcommitedenapi o_draft

  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: head '2c6d1f3b1bd6' hasn't been uploaded yet
  commitcloud: head 'f141e512974a' hasn't been uploaded yet
  edenapi: queue 2 commits for upload
  edenapi: queue 2 files for upload
  edenapi: uploaded 2 files
  edenapi: queue 2 trees for upload
  edenapi: uploaded 2 trees
  edenapi: uploaded 2 changesets
  commitcloud: commits synchronized
  finished in * (glob)
  $ smartlog
  @  f141e512974a draft 'o_draft'
  │
  o  22f66edbeb8e draft 'o2'
  │
  o  b22b11c36d16 draft 'o1'
  │
  │ o  2c6d1f3b1bd6 draft 'e_draft'
  │ │
  │ o  8537bcdeff72 draft 'e4'
  │ │
  │ o  5b7437b33959 draft 'e3'
  ├─╯
  │ o  98eac947fc54 draft 'e2'
  │ │
  │ o  6733e9fe3e4b draft 'e1'
  ├─╯
  o  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  
(Unfortunately, remote bookmarks are not updated on push)
  $ sl pull -B expensive -B expensive_other -B regular
  pulling from mononoke://$LOCALIP:$LOCAL_PORT/repo
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'expensive': '98eac947fc545fda4c6fc8531b18250aca738ca0', 'expensive_other': '8537bcdeff72ae8456e99f835f7cd3ce5e382772', 'regular': '22f66edbeb8ed912d75fab074df8b3069c91424a'}

  $ smartlog
  @  f141e512974a draft 'o_draft'
  │
  o  22f66edbeb8e public 'o2'  remote/regular
  │
  o  b22b11c36d16 public 'o1'
  │
  │ o  2c6d1f3b1bd6 draft 'e_draft'
  │ │
  │ o  8537bcdeff72 public 'e4'  remote/expensive_other
  │ │
  │ o  5b7437b33959 public 'e3'
  ├─╯
  │ o  98eac947fc54 public 'e2'  remote/expensive
  │ │
  │ o  6733e9fe3e4b public 'e1'
  ├─╯
  o  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: commits synchronized
  finished in 0.00 sec

  $ cd ../client2

  $ setconfig commitcloud.expensive_bookmarks="expensive, expensive_other"
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: fetching remote bookmark(s) remote/expensive, remote/expensive_other. Sorry, this may take a while...
  pulling 8537bcdeff72 98eac947fc54 from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  DEBUG pull::httpgraph: edenapi fetched 4 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 0 draft commits
  pulling 22f66edbeb8e 2c6d1f3b1bd6 f141e512974a from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  DEBUG pull::httpgraph: edenapi fetched 4 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 2 draft commits
  commitcloud: commits synchronized
  finished in * (glob)

XXX: We can't use `sl` here because output ordering is flaky.
  $ sl log -T "{node|short} {phase} '{desc|firstline}' {bookmarks} {remotebookmarks}\n" -r "sort(all(), desc)"
  8b2dca0c8a72 public 'base_commit' new_bookmark remote/master
  6733e9fe3e4b public 'e1'  
  98eac947fc54 public 'e2'  remote/expensive
  5b7437b33959 public 'e3'  
  8537bcdeff72 public 'e4'  remote/expensive_other
  2c6d1f3b1bd6 draft 'e_draft'  
  b22b11c36d16 public 'o1'  
  22f66edbeb8e public 'o2'  remote/regular
  f141e512974a draft 'o_draft'  


  $ sl bookmark -d new_bookmark

  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: commits synchronized
  finished in 0.00 sec

  $ cd ../client1
  $ sl cloud sync
  commitcloud: synchronizing 'repo' with 'user/test/default'
  commitcloud: nothing to upload
  commitcloud: commits synchronized
  finished in 0.00 sec
  $ sl log -T "{node|short} {phase} '{desc|firstline}' {bookmarks} {remotebookmarks}\n" -r "sort(all(), desc)"
  8b2dca0c8a72 public 'base_commit'  remote/master
  6733e9fe3e4b public 'e1'  
  98eac947fc54 public 'e2'  remote/expensive
  5b7437b33959 public 'e3'  
  8537bcdeff72 public 'e4'  remote/expensive_other
  2c6d1f3b1bd6 draft 'e_draft'  
  b22b11c36d16 public 'o1'  
  22f66edbeb8e public 'o2'  remote/regular
  f141e512974a draft 'o_draft'  

  $ sl cloud list
  commitcloud: searching workspaces for the 'repo' repo
  the following commitcloud workspaces are available:
          default (connected)
  run `hg cloud sl -w <workspace name>` to view the commits
  run `hg cloud switch -w <workspace name>` to switch to a different workspace
