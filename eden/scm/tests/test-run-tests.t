#chg-compatible
#debugruntest-incompatible

  $ export DEBUGRUNTEST_DEFAULT_DISABLED=1
  $ eagerepo
  $ setconfig devel.segmented-changelog-rev-compat=true
This file tests the behavior of run-tests.py itself.

Avoid interference from actual test env:

  $ . "$TESTDIR/helper-runtests.sh"

Smoke test with install
============

  $ run-tests.py $HGTEST_RUN_TESTS_PURE --with-hg=$HGTEST_HG
  
  ----------------------------------------------------------------------
  # Ran 0 tests, 0 skipped, 0 failed.

Define a helper to avoid the install step
=============
  $ rt()
  > {
  >     run-tests.py --with-hg=`which hg` -j 1 "$@"
  > }

error paths

#if symlink
  $ ln -s `which true` hg
  $ run-tests.py --with-hg=./hg
  
  ----------------------------------------------------------------------
  # Ran 0 tests, 0 skipped, 0 failed.
  $ rm hg
#endif

#if execbit
  $ touch hg
  $ run-tests.py --with-hg=./hg
  usage: run-tests.py [options] [tests]
  run-tests.py: error: --with-hg must specify an executable hg script
  [2]
  $ rm hg
#endif

Features for testing optional lines
===================================

  $ cat > hghaveaddon.py <<EOF
  > import hghave
  > @hghave.check("custom", "custom hghave feature")
  > def has_custom():
  >     return True
  > @hghave.check("missing", "missing hghave feature")
  > def has_missing():
  >     return False
  > EOF

an empty test
=======================

  $ touch test-empty.t
  $ rt
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
  $ rm test-empty.t

a succesful test
=======================

  $ cat > test-success.t << EOF
  >   $ echo babar
  >   babar
  >   $ echo xyzzy
  >   dont_print (?)
  >   nothing[42]line (re) (?)
  >   never*happens (glob) (?)
  >   more_nothing (?)
  >   xyzzy
  >   nor this (?)
  >   $ touch x
  >   $ printf 'abc\ndef\nxyz\n'
  >   123 (?)
  >   abc
  >   def (?)
  >   456 (?)
  >   xyz
  >   $ printf 'zyx\nwvu\ntsr\n'
  >   abc (?)
  >   zyx (custom !)
  >   wvu
  >   no_print (no-custom !)
  >   tsr (no-missing !)
  >   missing (missing !)
  >   $ rm x
  > EOF

  $ rt
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

failing test
==================

test churn with globs
  $ cat > test-failure.t <<EOF
  >   $ echo "bar-baz"; echo "bar-bad"; echo foo
  >   bar*bad (glob)
  >   bar*baz (glob)
  >   | fo (re)
  > EOF
  $ rt test-failure.t
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,4 +1,4 @@
     $ echo "bar-baz"; echo "bar-bad"; echo foo
  +  bar*baz (glob)
     bar*bad (glob)
  -  bar*baz (glob)
  -  | fo (re)
  +  foo
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ cat > test-failure.t << EOF
  >   $ true
  >   should go away (true !)
  >   $ true
  >   should stay (false !)
  > 
  > Should remove first line, not second or third
  >   $ echo 'testing'
  >   baz*foo (glob) (true !)
  >   foobar*foo (glob) (false !)
  >   te*ting (glob) (true !)
  > 
  > Should keep first two lines, remove third and last
  >   $ echo 'testing'
  >   test.ng (re) (true !)
  >   foo.ar (re) (false !)
  >   b.r (re) (true !)
  >   missing (?)
  >   awol (true !)
  > 
  > The "missing" line should stay, even though awol is dropped
  >   $ echo 'testing'
  >   test.ng (re) (true !)
  >   foo.ar (?)
  >   awol
  >   missing (?)
  > EOF
  $ rt test-failure.t
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,11 +1,9 @@
     $ true
  -  should go away (true !)
     $ true
     should stay (false !)
   
   Should remove first line, not second or third
     $ echo 'testing'
  -  baz*foo (glob) (true !)
     foobar*foo (glob) (false !)
     te*ting (glob) (true !)
   
     foo.ar (re) (false !)
     missing (?)
  @@ -13,13 +11,10 @@
     $ echo 'testing'
     test.ng (re) (true !)
     foo.ar (re) (false !)
  -  b.r (re) (true !)
     missing (?)
  -  awol (true !)
   
   The "missing" line should stay, even though awol is dropped
     $ echo 'testing'
     test.ng (re) (true !)
     foo.ar (?)
  -  awol
     missing (?)
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

basic failing test
  $ cat > test-failure.t << EOF
  >   $ echo babar
  >   rataxes
  > This is a noop statement so that
  > this test is still more bytes than success.
  > pad pad pad pad............................................................
  > pad pad pad pad............................................................
  > pad pad pad pad............................................................
  > pad pad pad pad............................................................
  > pad pad pad pad............................................................
  > pad pad pad pad............................................................
  > EOF

  >>> fh = open('test-failure-unicode.t', 'wb')
  >>> fh.write(u'  $ echo babar\u03b1\n'.encode('utf-8')) and None
  >>> fh.write(u'  l\u03b5\u03b5t\n'.encode('utf-8')) and None

  $ rt
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !.
  --- test-failure-unicode.t
  +++ test-failure-unicode.t.err
  @@ -1,2 +1,2 @@
     $ echo babar\xce\xb1 (esc)
  -  l\xce\xb5\xce\xb5t (esc)
  +  babar\xce\xb1 (esc)
  
  ERROR: test-failure-unicode.t output changed
  !
  ----------------------------------------------------------------------
  Failed 2 tests (output changed):
    test-failure-unicode.t
    test-failure.t
  
  # Ran 3 tests, 0 skipped, 2 failed.
  python hash seed: * (glob)
  [1]

test --retry

  $ cat > test-failure-short.t << EOF
  >   $ false
  > EOF
  $ rt --retry 3 test-failure-short.t test-success.t
  .
  --- test-failure-short.t
  +++ test-failure-short.t.err
  @@ -1 +1,2 @@
     $ false
  +  [1]
  
  ERROR: test-failure-short.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure-short.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  
  --- test-failure-short.t
  +++ test-failure-short.t.err
  @@ -1 +1,2 @@
     $ false
  +  [1]
  
  ERROR: test-failure-short.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure-short.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  
  --- test-failure-short.t
  +++ test-failure-short.t.err
  @@ -1 +1,2 @@
     $ false
  +  [1]
  
  ERROR: test-failure-short.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure-short.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  
  --- test-failure-short.t
  +++ test-failure-short.t.err
  @@ -1 +1,2 @@
     $ false
  +  [1]
  
  ERROR: test-failure-short.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure-short.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ rm test-failure-short.t

test --outputdir
  $ mkdir output
  $ rt --outputdir output
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !.
  --- test-failure-unicode.t
  +++ test-failure-unicode.t.err
  @@ -1,2 +1,2 @@
     $ echo babar\xce\xb1 (esc)
  -  l\xce\xb5\xce\xb5t (esc)
  +  babar\xce\xb1 (esc)
  
  ERROR: test-failure-unicode.t output changed
  !
  ----------------------------------------------------------------------
  Failed 2 tests (output changed):
    test-failure-unicode.t
    test-failure.t
  
  # Ran 3 tests, 0 skipped, 2 failed.
  python hash seed: * (glob)
  [1]
  $ LC_ALL=C ls -a output
  .
  ..
  .testtimes
  test-failure-unicode.t.err
  test-failure.t.err

test --xunit support
  $ rt --xunit=xunit.xml
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !.
  --- test-failure-unicode.t
  +++ test-failure-unicode.t.err
  @@ -1,2 +1,2 @@
     $ echo babar\xce\xb1 (esc)
  -  l\xce\xb5\xce\xb5t (esc)
  +  babar\xce\xb1 (esc)
  
  ERROR: test-failure-unicode.t output changed
  !
  ----------------------------------------------------------------------
  Failed 2 tests (output changed):
    test-failure-unicode.t
    test-failure.t
  
  # Ran 3 tests, 0 skipped, 2 failed.
  python hash seed: * (glob)
  [1]

  $ cat .testtimes
  test-failure-unicode.t * (glob)
  test-failure.t * (glob)
  test-success.t * (glob)

  $ rt --list-tests
  test-failure-unicode.t
  test-failure.t
  test-success.t

  $ rt --list-tests --json
  test-failure-unicode.t
  test-failure.t
  test-success.t
  $ cat report.json
  {
      "test-failure-unicode.t": {
          "result": "success"
      },
      "test-failure.t": {
          "result": "success"
      },
      "test-success.t": {
          "result": "success"
      }
  } (no-eol)

  $ rt --list-tests --xunit=xunit.xml
  test-failure-unicode.t
  test-failure.t
  test-success.t

  $ rt --list-tests test-failure* --json --xunit=xunit.xml --outputdir output
  test-failure-unicode.t
  test-failure.t
  $ cat output/report.json
  {
      "test-failure-unicode.t": {
          "result": "success"
      },
      "test-failure.t": {
          "result": "success"
      }
  } (no-eol)

  $ rm test-failure-unicode.t

test for --retest
====================

  $ rt --retest
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

--retest works with --outputdir
  $ rm -r output
  $ mkdir output
  $ mv test-failure.t.err output
  $ rt --retest --outputdir output
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

Selecting Tests To Run
======================

successful

  $ rt test-success.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

success w/ keyword
  $ rt -k xyzzy
  .
  ----------------------------------------------------------------------
  # Ran 2 tests, 1 skipped, 0 failed.

failed

  $ rt test-failure.t
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

failure w/ keyword
  $ rt -k rataxes
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

Verify that when a process fails to start we show a useful message
==================================================================

  $ cat > test-serve-fail.t <<EOF
  >   $ echo 'abort: child process failed to start blah'
  > EOF
  $ rt test-serve-fail.t
  
  ERROR: test-serve-fail.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (server failed to start (HGPORT=20062)):
    test-serve-fail.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ rm test-serve-fail.t

Verify that we can try other ports
===================================
This test is commented out since it could be flaky under stress run. Tests
using "hg serve" has been changed to use "-p 0 --port-file X; HGPORT=`cat X`"
to avoid race conditions between free port detection and actual usage.
#  $ hg init inuse
#  $ hg serve -R inuse -p 0 --port-file $TESTTMP/.port -d --pid-file=blocks.pid
#  $ HGPORT=`cat $TESTTMP/.port`
#  $ cat blocks.pid >> $DAEMON_PIDS
#  $ cat > test-serve-inuse.t <<EOF
#  >   $ hg serve -R `pwd`/inuse -p \$HGPORT -d --pid-file=hg.pid (glob)
#  >   $ cat hg.pid >> \$DAEMON_PIDS
#  > EOF
#  $ rt test-serve-inuse.t
#  .
#  # Ran 1 tests, 0 skipped, 0 failed.
#  $ rm test-serve-inuse.t
#  $ killdaemons.py $DAEMON_PIDS
#  $ rm $DAEMON_PIDS

Running In Debug Mode
======================

  $ rt --debug 2>&1 | grep SALT
  + echo *SALT* 0 0 (glob)
  *SALT* 0 0 (glob)
  + echo *SALT* 10 0 (glob)
  *SALT* 10 0 (glob)
  *+ echo *SALT* 0 0 (glob)
  *SALT* 0 0 (glob)
  + echo *SALT* 2 0 (glob)
  *SALT* 2 0 (glob)
  + echo *SALT* 9 0 (glob)
  *SALT* 9 0 (glob)
  + echo *SALT* 10 0 (glob)
  SALT* 10 0 (glob)
  + echo *SALT* 16 0 (glob)
  SALT* 16 0 (glob)
  + echo *SALT* 23 0 (glob)
  SALT* 23 0 (glob)
  + echo *SALT* 24 0 (glob)
  SALT* 24 0 (glob)

Parallel runs
==============

(duplicate the failing test to get predictable output)
  $ cp test-failure.t test-failure-copy.t

  $ rt --jobs 2 test-failure*.t -n
  !!
  ----------------------------------------------------------------------
  Failed 2 tests (output changed):
    test-failure-copy.t
    test-failure.t
  
  # Ran 2 tests, 0 skipped, 2 failed.
  python hash seed: * (glob)
  [1]

(delete the duplicated test file)
  $ rm test-failure-copy.t


Interactive run
===============

(backup the failing test)
  $ cp test-failure.t backup

Refuse the fix

  $ echo 'n' | rt -i
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  Accept this change? [n]o/yes/all 
  ERROR: test-failure.t output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ cat test-failure.t
    $ echo babar
    rataxes
  This is a noop statement so that
  this test is still more bytes than success.
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................

Interactive with custom view

  $ echo 'n' | rt -i --view echo
  $TESTTMP/test-failure.t $TESTTMP/test-failure.t.err
  Accept this change? [n]o/yes/all 
  ERROR: test-failure.t output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

View the fix

  $ echo 'y' | rt --view echo
  $TESTTMP/test-failure.t $TESTTMP/test-failure.t.err
  
  ERROR: test-failure.t output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

Accept the fix

  $ cat >> test-failure.t <<EOF
  >   $ echo 'saved backup bundle to \$TESTTMP/foo.hg'
  >   saved backup bundle to \$TESTTMP/foo.hg
  >   $ echo 'saved backup bundle to \$TESTTMP/foo.hg'
  >   saved backup bundle to $TESTTMP\\foo.hg
  >   $ echo 'saved backup bundle to \$TESTTMP/foo.hg'
  >   saved backup bundle to \$TESTTMP/*.hg (glob)
  > EOF
  $ echo 'y' | rt -i 2>&1
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  @@ -11,6 +11,6 @@
     $ echo 'saved backup bundle to $TESTTMP/foo.hg'
     saved backup bundle to $TESTTMP/foo.hg
     $ echo 'saved backup bundle to $TESTTMP/foo.hg'
  -  saved backup bundle to $TESTTMP\foo.hg
  +  saved backup bundle to $TESTTMP/foo.hg
     $ echo 'saved backup bundle to $TESTTMP/foo.hg'
     saved backup bundle to $TESTTMP/*.hg (glob)
  Accept this change? [n]o/yes/all ..
  ----------------------------------------------------------------------
  # Ran 2 tests, 0 skipped, 0 failed.

  $ sed -e 's,(glob)$,&<,g' test-failure.t
    $ echo babar
    babar
  This is a noop statement so that
  this test is still more bytes than success.
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
  pad pad pad pad............................................................
    $ echo 'saved backup bundle to $TESTTMP/foo.hg'
    saved backup bundle to $TESTTMP/foo.hg
    $ echo 'saved backup bundle to $TESTTMP/foo.hg'
    saved backup bundle to $TESTTMP/foo.hg
    $ echo 'saved backup bundle to $TESTTMP/foo.hg'
    saved backup bundle to $TESTTMP/*.hg (glob)<

Race condition - test file was modified when test is running

  $ TESTRACEDIR=`pwd`
  $ export TESTRACEDIR
  $ cat > test-race.t <<EOF
  >   $ echo 1
  >   $ echo "# a new line" >> $TESTRACEDIR/test-race.t
  > EOF

  $ rt -i test-race.t
  
  --- test-race.t
  +++ test-race.t.err
  @@ -1,2 +1,3 @@
     $ echo 1
  +  1
     $ echo "# a new line" >> $TESTTMP/test-race.t
  Reference output has changed (run again to prompt changes)
  ERROR: test-race.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-race.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ rm test-race.t

When "#testcases" is used in .t files

  $ cat >> test-cases.t <<EOF
  > #testcases a b
  > #if a
  >   $ echo 1
  > #endif
  > #if b
  >   $ echo 2
  > #endif
  > EOF

  $ cat <<EOF | rt -i test-cases.t 2>&1
  > y
  > y
  > EOF
  
  --- test-cases.t
  +++ test-cases.t.a.err
  @@ -1,6 +1,7 @@
   #testcases a b
   #if a
     $ echo 1
  +  1
   #endif
   #if b
     $ echo 2
  Accept this change? [n]o/yes/all .
  --- test-cases.t
  +++ test-cases.t.b.err
  @@ -5,4 +5,5 @@
   #endif
   #if b
     $ echo 2
  +  2
   #endif
  Accept this change? [n]o/yes/all .
  ----------------------------------------------------------------------
  # Ran 2 tests, 0 skipped, 0 failed.

  $ cat test-cases.t
  #testcases a b
  #if a
    $ echo 1
    1
  #endif
  #if b
    $ echo 2
    2
  #endif

  $ cat >> test-cases.t <<'EOF'
  > #if a
  >   $ NAME=A
  > #else
  >   $ NAME=B
  > #endif
  >   $ echo $NAME
  >   A (a !)
  >   B (b !)
  > EOF
  $ rt test-cases.t
  ..
  ----------------------------------------------------------------------
  # Ran 2 tests, 0 skipped, 0 failed.

  $ rm test-cases.t

(reinstall)
  $ mv backup test-failure.t

No Diff
===============

  $ rt --nodiff
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

test --tmpdir support
  $ rt --tmpdir=$TESTTMP/keep test-success.t
  testtmp dir: $TESTTMP/keep/child1/test-success.t 
  
  Keeping testtmp dir: $TESTTMP/keep/child1/test-success.t
  Keeping threadtmp dir: $TESTTMP/keep/child1 
  
  Set up config environment by:
    export HGRCPATH=* (glob)
    export SL_CONFIG_PATH=* (glob)
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

#if git no-bucktest
(bucktest produces .scm.sqlite file)
test --record support
  $ rt --tmpdir=$TESTTMP/record --record test-success.t
  testtmp dir: $TESTTMP/record/child1/test-success.t 
  
  Keeping testtmp dir: $TESTTMP/record/child1/test-success.t
  Keeping threadtmp dir: $TESTTMP/record/child1 
  
  Set up config environment by:
    export HGRCPATH=$TESTTMP/record/child1/.hgrc
    export SL_CONFIG_PATH=$TESTTMP/record/child1/.hgrc 
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
  $ GIT_DIR=$TESTTMP/record/child1/test-success.t/.git git log --stat
  commit 7dcd55e4b90850b75e85e73b0fa636d3f76247e2
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      EOF (line 24)
  
   x | 0
   1 file changed, 0 insertions(+), 0 deletions(-)
  
  commit f6293e577088469cb68d70eaebe572c396a89486
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ rm x (line 23)
  
  commit d30656b881527f71305a5cb0d36735bf04a8824d
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ printf 'zyx\nwvu\ntsr\n' (line 16)
  
  commit 749d47a07cbcc6ef6823ebb81e8749aa7728ee58
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ printf 'abc\ndef\nxyz\n' (line 10)
  
   x | 0
   1 file changed, 0 insertions(+), 0 deletions(-)
  
  commit bc24df25828a37502fd35fed6d03ee25839dbdc7
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ touch x (line 9)
  
  commit 66e869113273061d628421e8a8167bf7361c5938
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ echo xyzzy (line 2)
  
  commit 47537d28c96c68884b66c5fd3d3aea8bf44624f1
  Author: test <test@localhost>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      $ echo babar (line 0)
#endif

timeouts
========
  $ cat > test-timeout.t <<EOF
  >   $ sleep 10
  >   $ echo pass
  >   pass
  > EOF
  > echo '#require slow' > test-slow-timeout.t
  > cat test-timeout.t >> test-slow-timeout.t
  $ rt --timeout=1 --slowtimeout=3 test-timeout.t test-slow-timeout.t
  st
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: allow slow tests (use --allow-slow-tests)):
    test-slow-timeout.t
  
  Failed 1 tests (timed out):
    test-timeout.t
  
  # Ran 1 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ rt --timeout=1 --slowtimeout=3 \
  > test-timeout.t test-slow-timeout.t --allow-slow-tests
  .t
  ----------------------------------------------------------------------
  Failed 1 tests (timed out):
    test-timeout.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ rm test-timeout.t test-slow-timeout.t

test for --time
==================

  $ rt test-success.t --time
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
  # Producing time report
  start   end     cuser   csys    real      Test
  \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   test-success.t (re)

test for --time with --job enabled
====================================

  $ rt test-success.t --time --jobs 2
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
  # Producing time report
  start   end     cuser   csys    real      Test
  \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   \s*[\d\.]{5}   test-success.t (re)

Skips
================
  $ cat > test-skip.t <<EOF
  >   $ echo xyzzy
  > #require false
  > EOF
  $ rt --nodiff
  !.s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ rt --keyword xyzzy
  .s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  # Ran 2 tests, 2 skipped, 0 failed.

Skips with xml
  $ rt --keyword xyzzy \
  >  --xunit=xunit.xml
  .s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  # Ran 2 tests, 2 skipped, 0 failed.

Missing skips or blacklisted skips don't count as executed:
  $ echo test-failure.t > blacklist
  $ rt --blacklist=blacklist --json\
  >   test-failure.t test-bogus.t
  ss
  ----------------------------------------------------------------------
  Skipped 1 tests (Doesn't exist):
    test-bogus.t
  
  Skipped 1 tests (blacklisted):
    test-failure.t
  
  # Ran 0 tests, 2 skipped, 0 failed.
  $ cat report.json
  {
      "test-bogus.t": {
          "result": "skip"
      },
      "test-failure.t": {
          "result": "skip"
      }
  } (no-eol)

Whitelist trumps blacklist
  $ echo test-failure.t > whitelist
  $ rt --blacklist=blacklist --whitelist=whitelist --json\
  >   test-failure.t test-bogus.t
  s
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !
  ----------------------------------------------------------------------
  Skipped 1 tests (Doesn't exist):
    test-bogus.t
  
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 1 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

Ensure that --test-list causes only the tests listed in that file to
be executed.
  $ echo test-success.t >> onlytest
  $ rt --test-list=onlytest
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
  $ echo test-bogus.t >> anothertest
  $ rt --test-list=onlytest --test-list=anothertest
  s.
  ----------------------------------------------------------------------
  Skipped 1 tests (Doesn't exist):
    test-bogus.t
  
  # Ran 1 tests, 1 skipped, 0 failed.
  $ rm onlytest anothertest

test for --json
==================

  $ rt --json
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !.s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ cat report.json
  {
      "test-failure.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "---.+\+\+\+.+", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "failure", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-skip.t": {
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "skip", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-success.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "success", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }
  } (no-eol)
--json with --outputdir

  $ rm report.json
  $ rm -r output
  $ mkdir output
  $ rt --json --outputdir output
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  
  ERROR: test-failure.t output changed
  !.s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  Failed 1 tests (output changed):
    test-failure.t
  
  # Ran 2 tests, 1 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ f report.json
  report.json: file not found
  $ cat output/report.json
  {
      "test-failure.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "---.+\+\+\+.+", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "failure", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-skip.t": {
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "skip", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-success.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "success", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }
  } (no-eol)
  $ LC_ALL=C ls -a output
  .
  ..
  .testtimes
  report.json
  test-failure.t.err

Test that failed test accepted through interactive are properly reported:

  $ cp test-failure.t backup
  $ echo y | rt --json -i
  
  --- test-failure.t
  +++ test-failure.t.err
  @@ -1,5 +1,5 @@
     $ echo babar
  -  rataxes
  +  babar
   This is a noop statement so that
   this test is still more bytes than success.
   pad pad pad pad............................................................
  Accept this change? [n]o/yes/all ..s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: nail clipper):
    test-skip.t
  
  # Ran 2 tests, 1 skipped, 0 failed.

  $ cat report.json
  {
      "test-failure.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "success", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-skip.t": {
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "skip", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }, ? (re)
      "test-success.t": [\{] (re)
          "csys": "\s*[\d\.]{4,5}", ? (re)
          "cuser": "\s*[\d\.]{4,5}", ? (re)
          "diff": "", ? (re)
          "end": "\s*[\d\.]{4,5}", ? (re)
          "result": "success", ? (re)
          "start": "\s*[\d\.]{4,5}", ? (re)
          "time": "\s*[\d\.]{4,5}" (re)
      }
  } (no-eol)
  $ mv backup test-failure.t

backslash on end of line with glob matching is handled properly

  $ cat > test-glob-backslash.t << EOF
  >   $ echo 'foo bar \\'
  >   foo * \ (glob)
  > EOF

  $ rt test-glob-backslash.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

  $ rm -f test-glob-backslash.t

Test globbing of local IP addresses
  $ echo 172.16.18.1
  $LOCALIP (glob)
  $ echo dead:beef::1
  $LOCALIP (glob)

Test reusability for third party tools
======================================

  $ mkdir "$TESTTMP"/anothertests
  $ cd "$TESTTMP"/anothertests

test that `run-tests.py` can execute hghave, even if it runs not in
Mercurial source tree.

  $ cat > test-hghave.t <<EOF
  > #require true
  >   $ echo foo
  >   foo
  > EOF
  $ rt test-hghave.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

#if execbit

test that TESTDIR is referred in PATH

  $ cat > custom-command.sh <<EOF
  > #!/bin/sh
  > echo "hello world"
  > EOF
  $ chmod +x custom-command.sh
  $ cat > test-testdir-path.t <<EOF
  >   $ custom-command.sh
  >   hello world
  > EOF
  $ rt test-testdir-path.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

#endif

test support for --allow-slow-tests
  $ cat > test-very-slow-test.t <<EOF
  > #require slow
  >   $ echo pass
  >   pass
  > EOF
  $ rt test-very-slow-test.t
  s
  ----------------------------------------------------------------------
  Skipped 1 tests (missing feature: allow slow tests (use --allow-slow-tests)):
    test-very-slow-test.t
  
  # Ran 0 tests, 1 skipped, 0 failed.
  $ rt $HGTEST_RUN_TESTS_PURE --allow-slow-tests test-very-slow-test.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

support for running a test outside the current directory
  $ mkdir nonlocal
  $ cat > nonlocal/test-is-not-here.t << EOF
  >   $ echo pass
  >   pass
  > EOF
  $ rt nonlocal/test-is-not-here.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

support for automatically discovering test if arg is a folder
  $ mkdir tmp && cd tmp

  $ cat > test-uno.t << EOF
  >   $ echo line
  >   line
  > EOF

  $ cp test-uno.t test-dos.t
  $ cd ..
  $ cp -R tmp tmpp
  $ cp tmp/test-uno.t test-solo.t

  $ rt tmp/ test-solo.t tmpp
  .....
  ----------------------------------------------------------------------
  # Ran 5 tests, 0 skipped, 0 failed.
  $ rm -rf tmp tmpp

support for running run-tests.py from another directory
  $ mkdir tmp && cd tmp

  $ cat > useful-file.sh << EOF
  > important command
  > EOF

  $ cat > test-folder.t << EOF
  >   $ cat \$TESTDIR/useful-file.sh
  >   important command
  > EOF

  $ cat > test-folder-fail.t << EOF
  >   $ cat \$TESTDIR/useful-file.sh
  >   important commando
  > EOF

  $ cd ..
  $ rt tmp/test-*.t
  
  --- test-folder-fail.t
  +++ test-folder-fail.t.err
  @@ -1,2 +1,2 @@
     $ cat $TESTDIR/useful-file.sh
  -  important commando
  +  important command
  
  ERROR: test-folder-fail.t output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-folder-fail.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

support for bisecting failed tests automatically
  $ hg init bisect
  $ cd bisect
  $ cat >> test-bisect.t <<EOF
  >   $ echo pass
  >   pass
  > EOF
  $ hg add test-bisect.t
  $ hg ci -m 'good'
  $ cat >> test-bisect.t <<EOF
  >   $ echo pass
  >   fail
  > EOF
  $ hg ci -m 'bad'
  $ rt --known-good-rev=0 test-bisect.t
  
  --- test-bisect.t
  +++ test-bisect.t.err
  @@ -1,4 +1,4 @@
     $ echo pass
     pass
     $ echo pass
  -  fail
  +  pass
  
  ERROR: test-bisect.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-bisect.t
  
  test-bisect.t broken by 72cbf122d116 (bad)
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ cd ..

support bisecting a separate repo

  $ hg init bisect-dependent
  $ cd bisect-dependent
  $ cat > test-bisect-dependent.t <<EOF
  >   $ tail -1 \$TESTDIR/../bisect/test-bisect.t
  >     pass
  > EOF
  $ hg commit -Am dependent test-bisect-dependent.t

  $ rt --known-good-rev=0 test-bisect-dependent.t
  
  --- test-bisect-dependent.t
  +++ test-bisect-dependent.t.err
  @@ -1,2 +1,2 @@
     $ tail -1 $TESTDIR/../bisect/test-bisect.t
  -    pass
  +    fail
  
  ERROR: test-bisect-dependent.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-bisect-dependent.t
  
  Failed to identify failure point for test-bisect-dependent.t
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ rt --bisect-repo=../test-bisect test-bisect-dependent.t
  usage: run-tests.py [options] [tests]
  run-tests.py: error: --bisect-repo cannot be used without --known-good-rev
  [2]

  $ rt --known-good-rev=0 --bisect-repo=../bisect test-bisect-dependent.t
  
  --- test-bisect-dependent.t
  +++ test-bisect-dependent.t.err
  @@ -1,2 +1,2 @@
     $ tail -1 $TESTDIR/../bisect/test-bisect.t
  -    pass
  +    fail
  
  ERROR: test-bisect-dependent.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-bisect-dependent.t
  
  test-bisect-dependent.t broken by 72cbf122d116 (bad)
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

  $ cd ..

Test a broken #if statement doesn't break run-tests threading.
==============================================================
  $ mkdir broken
  $ cd broken
  $ cat > test-broken.t <<EOF
  > true
  > #if notarealhghavefeature
  >   $ false
  > #endif
  > EOF
  $ for f in 1 2 3 4 ; do
  > cat > test-works-$f.t <<EOF
  > This is test case $f
  >   $ sleep 1
  > EOF
  > done
  $ rt -j 2
  
  ERROR: test-broken.t output changed
  * (glob)
  ----------------------------------------------------------------------
  Failed 1 tests (feature unknown to hghave: ['notarealhghavefeature']):
    test-broken.t
  
  # Ran 5 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]
  $ cd ..
  $ rm -rf broken

Test cases in .t files
======================
  $ mkdir cases
  $ cd cases
  $ cat > test-cases-abc.t <<'EOF'
  > #testcases A B C
  >   $ V=B
  > #if A
  >   $ V=A
  > #endif
  > #if C
  >   $ V=C
  > #endif
  >   $ echo $V | sed 's/A/C/'
  >   C
  > #if C
  >   $ [ $V = C ]
  > #endif
  > #if A
  >   $ [ $V = C ]
  >   [1]
  > #endif
  > #if no-C
  >   $ [ $V = C ]
  >   [1]
  > #endif
  >   $ [ $V = D ]
  >   [1]
  > EOF
  $ rt
  .
  --- test-cases-abc.t
  +++ test-cases-abc.t.B.err
  @@ -7,7 +7,7 @@
     $ V=C
   #endif
     $ echo $V | sed 's/A/C/'
  -  C
  +  B
   #if C
     $ [ $V = C ]
   #endif
  
  ERROR: test-cases-abc.t (case B) output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-cases-abc.t
  
  # Ran 3 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

--restart works

  $ rt --restart
  
  --- test-cases-abc.t
  +++ test-cases-abc.t.B.err
  @@ -7,7 +7,7 @@
     $ V=C
   #endif
     $ echo $V | sed 's/A/C/'
  -  C
  +  B
   #if C
     $ [ $V = C ]
   #endif
  
  ERROR: test-cases-abc.t (case B) output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-cases-abc.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

--restart works with outputdir

  $ mkdir output
  $ mv test-cases-abc.t.B.err output
  $ rt --restart --outputdir output
  
  --- test-cases-abc.t
  +++ test-cases-abc.t.B.err
  @@ -7,7 +7,7 @@
     $ V=C
   #endif
     $ echo $V | sed 's/A/C/'
  -  C
  +  B
   #if C
     $ [ $V = C ]
   #endif
  
  ERROR: test-cases-abc.t (case B) output changed
  !.
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-cases-abc.t
  
  # Ran 2 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

Test automatic pattern replacement

  $ cat << EOF >> common-pattern.py
  > substitutions = [
  >     (br'foo-(.*)\\b',
  >      br'\$XXX=\\1\$'),
  >     (br'bar\\n',
  >      br'\$YYY$\\n'),
  > ]
  > EOF

  $ cat << EOF >> test-substitution.t
  >   $ echo foo-12
  >   \$XXX=12$
  >   $ echo foo-42
  >   \$XXX=42$
  >   $ echo bar prior
  >   bar prior
  >   $ echo lastbar
  >   last\$YYY$
  >   $ echo foo-bar foo-baz
  > EOF

  $ rt test-substitution.t
  
  --- test-substitution.t
  +++ test-substitution.t.err
  @@ -7,3 +7,4 @@
     $ echo lastbar
     last$YYY$
     $ echo foo-bar foo-baz
  +  $XXX=bar foo-baz$
  
  ERROR: test-substitution.t output changed
  !
  ----------------------------------------------------------------------
  Failed 1 tests (output changed):
    test-substitution.t
  
  # Ran 1 tests, 0 skipped, 1 failed.
  python hash seed: * (glob)
  [1]

--extra-config-opt works

  $ cat << EOF >> test-config-opt.t
  >   $ hg init test-config-opt
  >   $ hg -R test-config-opt purge
  > EOF

  $ rt --extra-config-opt extensions.rebase= test-config-opt.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.

--extra-rcpath works

  $ cat << EOF > test-rcpath.t
  >   $ hg config a
  >   a.b=1
  >   a.c=2
  > EOF

  $ cat << EOF > ab.rc
  > [a]
  > b=1
  > EOF

  $ cat << EOF > ac.rc
  > [a]
  > c=2
  > EOF

  $ rt --extra-rcpath `pwd`/ab.rc --extra-rcpath `pwd`/ac.rc test-rcpath.t
  .
  ----------------------------------------------------------------------
  # Ran 1 tests, 0 skipped, 0 failed.
