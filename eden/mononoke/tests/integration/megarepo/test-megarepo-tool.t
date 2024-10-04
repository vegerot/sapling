# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration

  $ REPOTYPE="blob_files"
  $ REPOID=0 REPONAME=repo setup_common_config $REPOTYPE
  $ REPOID=1 REPONAME=repo1 setup_common_config $REPOTYPE
  $ REPOID=2 REPONAME=repo2 setup_common_config $REPOTYPE
  $ setup_commitsyncmap

  $ cd $TESTTMP

setup hg server repo

  $ hginit_treemanifest repo
  $ cd repo
  $ function createfile { mkdir -p "$(dirname  $1)" && echo "$1" > "$1" && hg add "$1"; }

-- create some semblance of fbsource
  $ createfile fbcode/fbcodfile_fbsource
  $ createfile fbobjc/fbobjcfile_fbsource
  $ createfile fbandroid/fbandroidfile_fbsource
  $ createfile xplat/xplatfile_fbsource
  $ createfile arvr/arvrfile_fbsource
  $ createfile third-party/thirdpartyfile_fbsource
  $ hg ci -m "fbsource-like commit"
  $ hg book -r . fbsource_master

-- create some semblance of ovrsource
  $ hg up null -q
  $ createfile fbcode/fbcodfile_ovrsource
  $ createfile fbobjc/fbobjcfile_ovrsource
  $ createfile fbandroid/fbandroidfile_ovrsource
  $ createfile xplat/xplatfile_ovrsource
  $ createfile arvr/arvrfile_ovrsource
  $ createfile third-party/thirdpartyfile_ovrsource
  $ createfile Software/softwarefile_ovrsource
  $ createfile Research/researchfile_ovrsource
  $ hg ci -m "ovrsource-like commit"
  $ hg book -r . ovrsource_master

  $ hg log -T "{node} {bookmarks}\n" -r "all()"
  4da689e6447cf99bbc121eaa7b05ea1504cf2f7c fbsource_master
  4d79e7d65a781c6c80b3ee4faf63452e8beafa97 ovrsource_master

  $ cd $TESTTMP

setup repo-pull
  $ hg clone -q mono:repo repo-pull --noupdate

blobimport
  $ blobimport repo/.hg repo

  $ export COMMIT_DATE="1985-09-04T00:00:00.00Z"
move things in fbsource
  $ RUST_BACKTRACE=1 megarepo_tool move 1 fbsource_master user "fbsource move" --mark-public --commit-date-rfc3339 "$COMMIT_DATE" --bookmark fbsource_move --mapping-version-name TEST_VERSION_NAME
  * using repo "repo" repoid RepositoryId(0) (glob)
  * changeset resolved as: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "fbsource_move" * to point to * (glob)
  * Setting bookmark * "fbsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of *: HgChangesetId(HgNodeHash(Sha1(*))) (glob)

move things in ovrsource in a stack
  $ megarepo_tool move 2 ovrsource_master user "ovrsource stack move" --mark-public --commit-date-rfc3339 "$COMMIT_DATE" --max-num-of-moves-in-commit 1 --bookmark ovrsource_move --mapping-version-name TEST_VERSION_NAME
  * using repo "repo" repoid RepositoryId(0) (glob)
  * changeset resolved as: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "ovrsource_move" * to point to * (glob)
  * Setting bookmark * "ovrsource_move" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of * is: * (glob)
  * created 7 commits, with the last commit * (glob)

merge things in both repos
  $ megarepo_tool merge fbsource_move ovrsource_move user "megarepo merge" --mark-public --commit-date-rfc3339 "$COMMIT_DATE" --bookmark master
  * using repo "repo" repoid RepositoryId(0) (glob)
  * changeset resolved as: * (glob)
  * changeset resolved as: * (glob)
  * Creating a merge commit (glob)
  * Checking if there are any path conflicts (glob)
  * Done checking path conflicts (glob)
  * Creating a merge bonsai changeset with parents: * (glob)
  * Marked as public * (glob)
  * Setting bookmark * "master" * to point to * (glob)
  * Setting bookmark * "master" * finished (glob)
  * Generating an HG equivalent of * (glob)
  * Hg equivalent of *: HgChangesetId(HgNodeHash(Sha1(*))) (glob)

start mononoke server
  $ start_and_wait_for_mononoke_server
pull the result
  $ cd $TESTTMP/repo-pull
  $ hg -q pull && hg -q up master
  $ ls -1
  arvr
  arvr-legacy
  fbandroid
  fbcode
  fbobjc
  third-party
  xplat
  $ ls -1 fbcode fbandroid fbobjc xplat arvr arvr-legacy
  arvr:
  arvrfile_ovrsource
  
  arvr-legacy:
  Research
  Software
  third-party
  
  fbandroid:
  fbandroidfile_fbsource
  
  fbcode:
  fbcodfile_fbsource
  
  fbobjc:
  fbobjcfile_fbsource
  
  xplat:
  xplatfile_fbsource

test pre-merge deletes functionality
  $ cd "$TESTTMP"
  $ megarepo_tool pre-merge-delete master author "merge preparation" --even-chunk-size 4 --commit-date-rfc3339 "$COMMIT_DATE" 2>/dev/null
  32d2e80ff176b65df5cdeadec6dc52fdf8b66264965b001b91fab99dfb7aad75
  8807f350542a43aa815abc0c250c4a79ba35fd5bb68594e3ce6555e6630d81c2
  090a140adb3da3f4a629014cd9625055d8bd992a967ad7fc7e4e4d74892c4b71
