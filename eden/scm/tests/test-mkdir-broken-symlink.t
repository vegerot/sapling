# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

#require symlink no-windows no-eden

  $ eagerepo
  $ mkdir -p a
  $ ln -s a/b a/c
  $ hg debugshell -c 's.util.makedirs("a/c/e/f")'
  abort: Symlink '$TESTTMP/a/c' points to non-existed destination 'a/b' during makedir: $TESTTMP/a/c/e
  [255]
