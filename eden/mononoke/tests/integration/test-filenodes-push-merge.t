# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ BLOB_TYPE="blob_files" default_setup
  hg repo
  o  C [draft;rev=2;26805aba1e60]
  │
  o  B [draft;rev=1;112478962961]
  │
  o  A [draft;rev=0;426bada5c675]
  $
  blobimporting
  starting Mononoke
  cloning repo in hg client 'repo2'


Creating a merge commit
  $ cd "$TESTTMP/repo2"
  $ hg up -q null
  $ echo 1 > tomerge
  $ hg -q addremove
  $ hg ci -m 'tomerge'
  $ NODE="$(hg log -r . -T '{node}')"
  $ hg up -q master_bookmark
  $ hg merge -q -r "$NODE"
  $ hg ci -m 'merge'

Pushing a merge
  $ hg push -r . --to master_bookmark
  pushing rev 7d332475050d to destination mono:repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark
  $ mononoke_admin filenodes validate "$(hg log -r master_bookmark -T '{node}')"
  * using repo "repo" repoid RepositoryId(0) (glob)
  * changeset resolved as: * (glob)
