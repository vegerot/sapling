#chg-compatible
#debugruntest-incompatible

  $ eagerepo
  $ enable fbcodereview

Setup repo

  $ hg init repo
  $ cd repo

Test phabdiff template mapping

  $ echo a > a
  $ hg commit -Aqm "Differential Revision: https://phabricator.fb.com/D1234
  > Task ID: 2312"
  $ hg log --template "{phabdiff}\n"
  D1234

  $ echo c > c
  $ hg commit -Aqm "Differential Revision: http://phabricator.intern.facebook.com/D1245
  > Task ID: 2312"
  $ hg log -r . --template "{phabdiff}\n"
  D1245

  $ echo b > b
  $ hg commit -Aqm "Differential Revision: https://phabricator.fb.com/D5678
  > Tasks:32, 44    55"
  $ hg log -r . --template "{phabdiff}: {tasks}\n"
  D5678: 32 44 55

  $ echo d > d
  $ hg commit -Aqm "Differential Revision: http://phabricator.intern.facebook.com/D1245
  > Task: t123456,456"
  $ hg log -r . --template "{phabdiff}: {tasks}\n"
  D1245: 123456 456

Only match the Differential Revision label at the start of a line

  $ echo e > e
  $ hg commit -Aqm "Test Commit
  > Test Plan: tested on Differential Revision: http://phabricator.intern.facebook.com/D1000
  > Differential Revision: http://phabricator.intern.facebook.com/D6789
  > "
  $ hg log -r . --template "{phabdiff}\n"
  D6789

Test reviewers label

  $ echo f > f
  $ hg commit -Aqm "Differential Revision: http://phabricator.intern.facebook.com/D9876
  > Reviewers: xlwang, quark durham, rmcelroy"
  $ hg log -r . --template '{reviewers}\n'
  xlwang quark durham rmcelroy
  $ hg log -r . --template '{reviewers % "- {reviewer}\n"}\n'
  - xlwang
  - quark
  - durham
  - rmcelroy
  
  $ echo g > g
  $ hg commit -Aqm "Differential Revision: http://phabricator.intern.facebook.com/D9876
  > Reviewers: xlwang quark"
  $ hg log -r . --template "{join(reviewers, ', ')}\n"
  xlwang, quark

Test reviewers for working copy

  $ enable debugcommitmessage
  $ hg debugcommitmessage --config 'committemplate.changeset={reviewers}' --config 'committemplate.reviewers=foo, {x}' --config 'committemplate.x=bar'
  foo, bar (no-eol)

  $ hg debugcommitmessage --config 'committemplate.changeset=A{reviewers}B'
  AB (no-eol)

Make sure the template keywords are documented correctly

  $ hg help templates | grep -E 'phabdiff|tasks'
      phabdiff      String. Return the phabricator diff id for a given hg rev.
      tasks         String. Return the tasks associated with given hg rev.
      blame_phabdiffid

Check singlepublicbase

  $ hg log -r . --template "{singlepublicbase}\n"
  

  $ hg debugmakepublic -r ::2480b7b497e0af879a40a0d4d960ceb748d27085

  $ hg log -r . --template "{singlepublicbase}\n"
  2480b7b497e0af879a40a0d4d960ceb748d27085

Check hg backout template listing the diff properly
  $ echo h > h
  $ hg commit -Aqm "Differential Revision: https://phabricator.intern.facebook.com/D98765"
  $ hg log -l 1 --template "{phabdiff}\n"
  D98765
  $ hg backout -r . -m "Some default message to avoid the interactive editor" -q
  $ hg log -l 1 --template '{desc}' | grep "Original Phabricator Diff"
  Original Phabricator Diff: D98765
