
#require no-eden

# Copyright (c) Meta Platforms, Inc. and affiliates.
# Copyright (c) Mercurial Contributors.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

# https://bz.mercurial-scm.org/660 and:
# https://bz.mercurial-scm.org/322

  $ setconfig devel.segmented-changelog-rev-compat=true
  $ setconfig commands.update.check=none
  $ eagerepo

  $ hg init repo
  $ cd repo
  $ echo a > a
  $ mkdir b
  $ echo b > b/b
  $ hg commit -A -m 'a is file, b is dir'
  adding a
  adding b/b

# File replaced with directory:

  $ rm a
  $ mkdir a
  $ echo a > a/a

# Should fail - would corrupt dirstate:

  $ hg add a/a
  abort: file 'a' in dirstate clashes with 'a/a'
  [255]

# Removing shadow:

  $ hg rm --mark a

# Should succeed - shadow removed:

  $ hg add a/a

# Directory replaced with file:

  $ rm -r b
  $ echo b > b

# Should fail - would corrupt dirstate:

  $ hg add b
  abort: directory 'b' already in dirstate
  [255]

# Removing shadow:

  $ hg rm --mark b/b

# Should succeed - shadow removed:

  $ hg add b

# Look what we got:

  $ hg st
  A a/a
  A b
  R a
  R b/b

# Revert reintroducing shadow - should fail:

  $ rm -r a b
  $ hg revert b/b
  abort: file 'b' in dirstate clashes with 'b/b'
  [255]

# Revert all - should succeed:

  $ hg revert --all
  undeleting a
  forgetting a/a
  forgetting b
  undeleting b/b

  $ hg st

# Issue3423:

  $ hg forget a
  $ echo zed > a
  $ hg revert a
  $ hg st
  ? a.orig
  $ rm a.orig

# addremove:

  $ rm -r a b
  $ mkdir a
  $ echo a > a/a
  $ echo b > b

  $ hg addremove -s 0
  removing a
  adding a/a
  adding b
  removing b/b

  $ hg st
  A a/a
  A b
  R a
  R b/b

# commit:

  $ hg ci -A -m 'a is dir, b is file'
  $ hg st --all
  C a/a
  C b

# Long directory replaced with file:

  $ mkdir d
  $ mkdir d/d
  $ echo d > d/d/d
  $ hg commit -A -m 'd is long directory'
  adding d/d/d

  $ rm -r d
  $ echo d > d

# Should fail - would corrupt dirstate:

  $ hg add d
  abort: directory 'd' already in dirstate
  [255]

# Removing shadow:

  $ hg rm --mark d/d/d

# Should succeed - shadow removed:

  $ hg add d
  $ hg ci -md

# Update should work at least with clean working directory:

  $ rm -r a b d
  $ hg up -r 0
  2 files updated, 0 files merged, 0 files removed, 0 files unresolved

  $ hg st --all
  C a
  C b/b

  $ rm -r a b
  $ hg up -r 1
  2 files updated, 0 files merged, 0 files removed, 0 files unresolved

  $ hg st --all
  C a/a
  C b
