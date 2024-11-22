# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ setup_common_config
  $ REPOID=1 REPONAME=disabled_repo ENABLED=false setup_mononoke_config
  $ cd $TESTTMP
  $ setconfig remotenames.selectivepulldefault=master_bookmark,master_bookmark2

setup common configuration
  $ cat >> $HGRCPATH <<EOF
  > [ui]
  > ssh="$DUMMYSSH"
  > EOF


setup repo

  $ hginit_treemanifest repo
  $ cd repo

  $ touch a
  $ hg add a
  $ hg ci -ma
  $ hg log
  commit:      3903775176ed
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     a
   (re)
  $ hg book master_bookmark
  $ cd $TESTTMP

setup repo2
  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > remotefilelog=
  > [remotefilelog]
  > cachepath=$TESTTMP/cachepath
  > EOF
  $ hg clone -q mono:repo repo2 --noupdate
  $ cd repo2
  $ hg pull -q

  $ cd $TESTTMP
  $ cd repo
  $ touch b
  $ hg add b
  $ hg ci -mb
  $ echo content > c
  $ hg add c
  $ hg ci -mc
  $ mkdir dir
  $ echo 1 > dir/1
  $ mkdir dir2
  $ echo 2 > dir/2
  $ hg addremove
  adding dir/1
  adding dir/2
  $ hg ci -m 'new directory'
  $ echo cc > c
  $ hg addremove
  $ hg ci -m 'modify file'
  $ hg mv dir/1 dir/rename
  $ hg ci -m 'rename'
  $ hg debugdrawdag <<'EOS'
  >   D  # D/D=1\n2\n
  >  /|  # B/D=1\n
  > B C  # C/D=2\n
  > |/   # A/D=x\n
  > A
  > EOS
  $ hg log --graph -T '{node|short} {desc}'
  o    e635b24c95f7 D
  ├─╮
  │ o  d351044ef463 C
  │ │
  o │  9a827afb7e25 B
  ├─╯
  o  af6aa0dfdf3d A
   (re)
  @  9f8e7242d9fa rename
  │
  o  586ef37a04f7 modify file
  │
  o  e343d2f326cf new directory
  │
  o  3e19bf519e9a c
  │
  o  0e067c57feba b
  │
  o  3903775176ed a
   (re)

setup master bookmarks

  $ hg bookmark master_bookmark -r e635b24c95f7 -f
  $ hg bookmark master_bookmark2 -r 9f8e7242d9fa

blobimport

  $ cd ..
  $ blobimport repo/.hg repo

start mononoke

  $ start_and_wait_for_mononoke_server
  $ hg debugwireargs mono:disabled_repo one two --three three
  remote: Requested repo "disabled_repo" does not exist or is disabled
  abort: unexpected EOL, expected netstring digit
  [255]
  $ hg debugwireargs mono:repo one two --three three
  one two three None None

  $ cd repo2
  $ hg up -q "min(all())"
Test a pull of one specific revision
  $ hg pull -r 3e19bf519e9af6c66edf28380101a92122cbea50 -q
Pull the rest
  $ hg pull -q

  $ hg log -r '3903775176ed::586ef37a04f7' --graph  -T '{node|short} {desc}'
  o  586ef37a04f7 modify file
  │
  o  e343d2f326cf new directory
  │
  o  3e19bf519e9a c
  │
  o  0e067c57feba b
  │
  @  3903775176ed a
   (re)
  $ ls
  a
  $ hg up 9f8e7242d9fa -q
  $ ls
  a
  b
  c
  dir
  $ cat c
  cc
  $ hg up 9f8e7242d9fa -q
  $ hg log c -T '{node|short} {desc}\n'
  warning: file log can be slow on large repos - use -f to speed it up
  586ef37a04f7 modify file
  3e19bf519e9a c
  $ cat dir/rename
  1
  $ cat dir/2
  2
  $ hg log dir/rename -f -T '{node|short} {desc}\n'
  9f8e7242d9fa rename
  e343d2f326cf new directory
  $ hg st --change 9f8e7242d9fa -C
  A dir/rename
    dir/1
  R dir/1

  $ hg up -q e635b24c95f7

Sort the output because it may be unpredictable because of the merge
  $ hg log D --follow -T '{node|short} {desc}\n' | sort
  9a827afb7e25 B
  af6aa0dfdf3d A
  d351044ef463 C
  e635b24c95f7 D

Create a new bookmark and try and send it over the wire
Test commented while we have no bookmark support in blobimport or easy method
to create a fileblob bookmark
#  $ cd ../repo
#  $ hg bookmark test-bookmark
#  $ hg bookmarks
#   * test-bookmark             0:3903775176ed
#  $ cd ../repo2
#  $ hg pull mono:repo
#  pulling from ssh://user@dummy/repo
#  searching for changes
#  no changes found
#  adding remote bookmark test-bookmark
#  $ hg bookmarks
#     test-bookmark             0:3903775176ed

Do a clone of the repo
  $ hg clone mono:repo repo-streamclone
  fetching lazy changelog
  populating main commit graph
  updating to tip
  5 files updated, 0 files merged, 0 files removed, 0 files unresolved
