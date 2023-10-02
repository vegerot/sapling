#require git no-windows
#debugruntest-compatible

  $ eagerepo
  $ . $TESTDIR/git.sh

Make a commit with a non-GMT timezone:

  $ hg init --git gitrepo1
  $ cd gitrepo1
  $ hg commit -d '2022-11-23 17:47:30 -0800' -m A --config ui.allowemptycommit=1
  $ hg bookmark -q A

Timezone parsed by hg:

  $ hg log -r . -T '{date|isodatesec}\n'
  2022-11-23 17:47:30 -0800

Template keywords. Note the committerdate is a constant 0 in tests:

  $ hg log -r . -T '{authordate|isodatesec}\n{committerdate|isodatesec}\n'
  2022-11-23 17:47:30 -0800
  1970-01-01 00:00:00 +0000

Timezone parsed by Git:

  $ git --git-dir=.hg/store/git log --format=%ai refs/heads/A
  2022-11-23 17:47:30 -0800
