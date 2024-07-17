
#require no-eden

# coding=utf-8
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# Set up test environment.

  $ eagerepo
  $ setconfig devel.segmented-changelog-rev-compat=true
  $ setconfig workingcopy.rust-checkout=true
  $ cat >> $HGRCPATH << 'EOF'
  > [extensions]
  > amend=
  > rebase=
  > [experimental]
  > evolution = obsolete
  > [mutation]
  > enabled=true
  > record=false
  > [visibility]
  > enabled=true
  > EOF

# Test that rebases that cause an orphan commit are not a problem.

  $ hg init repo
  $ cd repo
  $ hg debugbuilddag -m '+3 *3'
  $ showgraph
  o  e5d56d7a7894 r3
  │
  │ o  c175bafe34cb r2
  │ │
  │ o  22094967a90d r1
  ├─╯
  o  1ad88bca4140 r0
  $ hg rebase -r 1 -d 3
  rebasing 22094967a90d "r1"
  merging mf
  $ showgraph
  o  89cc0c77a33f r1
  │
  o  e5d56d7a7894 r3
  │
  │ o  c175bafe34cb r2
  │ │
  │ x  22094967a90d r1
  ├─╯
  o  1ad88bca4140 r0
