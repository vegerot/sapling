#chg-compatible

#require test-repo normal-layout

  $ eagerepo
  $ . "$TESTDIR/helpers-testrepo.sh"
  $ enable amend undo
  $ cat <<'EOF' > scanhelptopics.py
  > from __future__ import absolute_import, print_function
  > import re
  > import sys
  > if sys.platform == "win32":
  >     import os, msvcrt
  >     msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
  > topics = set()
  > topicre = re.compile(r':prog:`help ([a-z0-9\-.]+)`')
  > for fname in sys.argv:
  >     with open(fname) as f:
  >         topics.update(m.group(1) for m in topicre.finditer(f.read()))
  > for s in sorted(topics):
  >     print(s)
  > EOF

  $ cd "$TESTDIR"/..

Check if ":prog:`help TOPIC`" is valid:
(use "xargs -n1 -t" to see which help commands are executed)

  $ NPROC=`hg debugpython -- -c 'import multiprocessing; print(str(multiprocessing.cpu_count()))'`
  $ testrepohg files 'glob:sapling/**/*.py' \
  > | sed 's|\\|/|g' \
  > | xargs $PYTHON "$TESTTMP/scanhelptopics.py" > $TESTTMP/topics
