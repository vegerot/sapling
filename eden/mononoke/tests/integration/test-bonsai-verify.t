# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ setup_common_config
  $ cd $TESTTMP

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
  $ hg log -r. -T '{node}\n'
  3903775176ed42b1458a6281db4a0ccf4d9f287a

blobimport

  $ cd ..
  $ blobimport repo/.hg repo

smoke test to ensure bonsai_verify works

  $ bonsai_verify round-trip 3903775176ed42b1458a6281db4a0ccf4d9f287a 2>&1 | grep valid
  * 100.00% valid, summary: , total: 1, valid: 1, errors: 0, ignored: 0 (glob)
