# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ setup_common_config
  $ cd $TESTTMP

setup repo

  $ hginit_treemanifest repo
  $ cd repo
  $ echo "a" > a
  $ echo "b" > b
  $ hg addremove && hg ci -q -ma
  adding a
  adding b
  $ hg log -T '{node}\n'
  0cd96de13884b090099512d4794ae87ad067ea8e

create master bookmark
  $ hg bookmark master_bookmark -r tip

setup repo-push and repo-pull
  $ cd $TESTTMP
  $ hg clone -q mono:repo repo-push --noupdate
  $ hg clone -q mono:repo repo-pull --noupdate

blobimport

  $ blobimport repo/.hg repo

start mononoke

  $ start_and_wait_for_mononoke_server
push some files with copy/move files

  $ cd $TESTTMP/repo-push
  $ hg up master_bookmark
  2 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg cp a a_copy
  $ hg mv b b_move
  $ hg addremove && hg ci -q -mb
  recording removal of b as rename to b_move (100% similar)
  $ hg push --to master_bookmark
  pushing rev 4b747ca852a4 to destination mono:repo bookmark master_bookmark
  searching for changes
  updating bookmark master_bookmark

pull them

  $ cd $TESTTMP/repo-pull
  $ hg up master_bookmark
  2 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg log -T '{node}\n'
  0cd96de13884b090099512d4794ae87ad067ea8e
  $ hg pull
  pulling from mono:repo
  imported commit graph for 1 commit (1 segment)
  $ hg log -T '{node}\n'
  4b747ca852a40a105b9bb71cd4d07248ea80f704
  0cd96de13884b090099512d4794ae87ad067ea8e

push files that modify copied and moved files

  $ cd $TESTTMP/repo-push
  $ echo "aa" >> a_copy
  $ echo "bb" >> b_move
  $ hg addremove && hg ci -q -mc
  $ hg push --to master_bookmark
  pushing rev 8b374fd7e2ef to destination mono:repo bookmark master_bookmark
  searching for changes
  updating bookmark master_bookmark

pull them

  $ cd $TESTTMP/repo-pull
  $ hg log -T '{node}\n'
  4b747ca852a40a105b9bb71cd4d07248ea80f704
  0cd96de13884b090099512d4794ae87ad067ea8e
  $ hg pull
  pulling from mono:repo
  imported commit graph for 1 commit (1 segment)
  $ hg log -T '{node}\n'
  8b374fd7e2ef1cc418b9c68f484ebd2cb6c6c6a1
  4b747ca852a40a105b9bb71cd4d07248ea80f704
  0cd96de13884b090099512d4794ae87ad067ea8e
  $ hg up master_bookmark
  2 files updated, 0 files merged, 1 files removed, 0 files unresolved
  $ cat a_copy
  a
  aa
  $ cat b_move
  b
  bb
