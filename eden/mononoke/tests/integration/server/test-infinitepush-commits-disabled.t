# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ setup_common_config
  $ cd $TESTTMP

setup common configuration for these tests

  $ enable amend infinitepush commitcloud

setup repo

  $ hginit_treemanifest repo
  $ cd repo
  $ touch a && hg addremove && hg ci -q -ma
  adding a
  $ hg log -T '{short(node)}\n'
  3903775176ed

create master bookmark
  $ hg bookmark master_bookmark -r tip

  $ cd $TESTTMP

setup repo-push and repo-pull
  $ hg clone -q mono:repo repo-push --noupdate
  $ hg clone -q mono:repo repo-pull --noupdate

blobimport

  $ blobimport repo/.hg repo

start mononoke

  $ start_and_wait_for_mononoke_server

Do infinitepush (aka commit cloud) push
  $ cd repo-push
  $ cat >> .hg/hgrc <<EOF
  > [infinitepush]
  > server=False
  > branchpattern=re:scratch/.+
  > EOF
  $ hg up tip
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ echo new > newfile
  $ hg addremove -q
  $ hg ci -m new
  $ hg push -r . --to "scratch/123"
  pushing to mono:repo
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     bundle2_resolver error
  remote: 
  remote:   Root cause:
  remote:     Infinitepush is not enabled on this server. Contact Source Control @ FB.
  remote: 
  remote:   Caused by:
  remote:     While resolving Changegroup
  remote:   Caused by:
  remote:     Infinitepush is not enabled on this server. Contact Source Control @ FB.
  remote: 
  remote:   Debug context:
  remote:     Error {
  remote:         context: "bundle2_resolver error",
  remote:         source: Error {
  remote:             context: "While resolving Changegroup",
  remote:             source: "Infinitepush is not enabled on this server. Contact Source Control @ FB.",
  remote:         },
  remote:     }
  abort: unexpected EOL, expected netstring digit
  [255]

  $ tglogp
  @  47da8b81097c draft 'new'
  │
  o  3903775176ed public 'a'
  

Bookmark push should have been ignored
  $ sqlite3 "$TESTTMP/monsql/sqlite_dbs" 'SELECT name, hg_kind, HEX(changeset_id) FROM bookmarks;'
  master_bookmark|pull_default|E10EC6CD13B1CBCFE2384F64BD37FC71B4BF9CFE21487D2EAF5064C1B3C0B793

Commit should have been rejected
  $ cd ../repo-pull
  $ cat >> .hg/hgrc <<EOF
  > [infinitepush]
  > server=False
  > branchpattern=re:scratch/.+
  > EOF
  $ hg pull -r 47da8b81097c5534f3eb7947a8764dd323cffe3d
  pulling from mono:repo
  abort: 47da8b81097c5534f3eb7947a8764dd323cffe3d not found!
  [255]
