# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ POPULATE_GIT_MAPPING=1 setup_common_config

setup repo
  $ cd $TESTTMP
  $ hginit_treemanifest repo-hg
  $ cd repo-hg
  $ touch a
  $ hg add a
  $ hg ci -ma --extra convert_revision=37b0a167e07f2b84149c918cec818ffeb183dddd --extra hg-git-rename-source=git
  $ touch b
  $ hg add b
  $ hg ci -mb --extra convert_revision=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb --extra hg-git-rename-source=git
  $ hg log -r '.^::.' -T '{node}\n'
  d5b0942fd0ec9189debf6915e9505390564e1247
  4f4a1f2b7bdc23710132eeb620424bf195f95568
  $ hg book -r d5b0942fd0ec9189debf6915e9505390564e1247 _gitlookup_git_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb

blobimport
  $ cd $TESTTMP
  $ blobimport repo-hg/.hg repo

start mononoke
  $ start_and_wait_for_mononoke_server
  $ cd repo-hg
  $ hg up -q "min(all())"

  $ hgmn paths
  default = mononoke://$LOCALIP:$LOCAL_PORT/repo
  $ hgmn id -r _gitlookup_git_37b0a167e07f2b84149c918cec818ffeb183dddd mononoke://$(mononoke_address)/repo
  d5b0942fd0ec
  $ hgmn id -r _gitlookup_hg_d5b0942fd0ec9189debf6915e9505390564e1247 mononoke://$(mononoke_address)/repo
  37b0a167e07f
  $ hgmn id -r _gitlookup_hg_4f4a1f2b7bdc23710132eeb620424bf195f95568 mononoke://$(mononoke_address)/repo
  bbbbbbbbbbbb

We have bookmark with the same name which points to d5b0942fd0ec9189debf6915e9505390564e1247
Make sure that git lookup takes preference
  $ hgmn id -r _gitlookup_git_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb mononoke://$(mononoke_address)/repo
  4f4a1f2b7bdc
