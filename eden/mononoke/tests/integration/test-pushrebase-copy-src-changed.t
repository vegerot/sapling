# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ setup_common_config "blob_files"
  $ cd $TESTTMP

setup common configuration
  $ cat >> $HGRCPATH <<EOF
  > [ui]
  > ssh="$DUMMYSSH"
  > [extensions]
  > amend=
  > EOF

setup repo
  $ hg init repo-hg
  $ cd repo-hg
  $ setup_hg_server
  $ hg debugdrawdag <<EOF
  > C
  > |
  > B
  > |
  > A
  > EOF

Clone the repo
  $ cd ..
  $ hgclone_treemanifest ssh://user@dummy/repo-hg repo2 --noupdate --config extensions.remotenames= -q
  $ cd repo2
  $ setup_hg_client
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > remotenames =
  > EOF


Modify a file
  $ cd ../repo-hg
  $ hg up -q tip
  $ echo B > A
  $ hg ci -m 'modify copy source'

create master bookmark

  $ hg bookmark master_bookmark -r tip

blobimport them into Mononoke storage and start Mononoke
  $ cd ..
  $ blobimport repo-hg/.hg repo

start mononoke
  $ start_and_wait_for_mononoke_server
Create a copy on a client and push it
  $ cd repo2
  $ hg up -q tip
  $ hg cp A D
  $ hg ci -m 'make a copy'
  $ hgmn push -r . --to master_bookmark
  pushing rev 726a45528732 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     pushrebase failed Conflicts([PushrebaseConflict { left: NonRootMPath("A"), right: NonRootMPath("A") }])
  remote: 
  remote:   Root cause:
  remote:     pushrebase failed Conflicts([PushrebaseConflict { left: NonRootMPath("A"), right: NonRootMPath("A") }])
  remote: 
  remote:   Debug context:
  remote:     "pushrebase failed Conflicts([PushrebaseConflict { left: NonRootMPath(\"A\"), right: NonRootMPath(\"A\") }])"
  abort: unexpected EOL, expected netstring digit
  [255]
