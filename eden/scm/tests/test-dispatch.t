
#require no-eden

#inprocess-hg-incompatible

test command parsing and dispatch

  $ hg init a
  $ cd a

Redundant options used to crash (issue436):
  $ hg -v log -v
  $ hg -v log -v x

  $ echo a > a
  $ hg ci -Ama
  adding a

Missing arg:

  $ hg cat
  hg cat: invalid arguments
  (use 'hg cat -h' to get help)
  [255]

Missing parameter for early option:

  $ hg log -R 2>&1 | grep 'hg log'
  hg log: option -R requires argument
  (use 'hg log -h' to get help)

"--" may be an option value:

  $ hg -R -- log
  abort: repository -- not found!
  [255]
  $ hg log -R --
  abort: repository -- not found!
  [255]
  $ hg log -T --
  -- (no-eol)
  $ hg log -T -- -k nomatch

Parsing of early options should stop at "--":

  $ hg debug-args -- --config=hooks.pre-cat=false
  ["--config=hooks.pre-cat=false"]
  $ hg debug-args -- --debugger
  ["--debugger"]

Unparsable form of early options:

  $ hg cat --debugg
  abort: option --debugger may not be abbreviated or used in aliases
  [255]

Parsing failure of early options should be detected before executing the
command:

  $ hg log -b '--config=hooks.pre-log=false' default
  abort: option --config may not be abbreviated, used in aliases, or used as a value for another option
  [255]
  $ hg log -b -R. default
  abort: option -R must appear alone, and --repository may not be abbreviated or used in aliases
  [255]
  $ hg log --cwd .. -b --cwd=. default
  abort: option --cwd may not be abbreviated or used in aliases
  [255]

However, we can't prevent it from loading extensions and configs:

  $ cat <<EOF > bad.py
  > raise Exception('bad')
  > EOF
  $ hg log -b '--config=extensions.bad=bad.py' default
  warning: extension bad is disabled because it cannot be imported from bad.py: bad
  abort: option --config may not be abbreviated, used in aliases, or used as a value for another option
  [255]

  $ mkdir -p badrepo/.hg
  $ touch badrepo/.hg/requires
  $ echo 'invalid-syntax' > badrepo/.hg/hgrc
  $ hg log -b -Rbadrepo default
  hg: parse errors: "$TESTTMP/a/badrepo/.hg/hgrc":
  line 1: expect '[section]' or 'name = value'
  
  [255]

(XXX: Rust io::Error does not contain path information)
  $ hg log -b --cwd=inexistent default
  abort: $ENOENT$
  [255]

  $ hg log -b '--config=ui.traceback=yes' 2>&1 | grep '^Traceback'
  Traceback (most recent call last):
  $ hg log -b '--config=profiling.enabled=yes' 2>&1 | grep '[Ss]ample'
  Sample count: .*|No samples recorded\. (re)

Early options can't be specified in [aliases] and [defaults] because they are
applied before the command name is resolved:

  $ hg log -b '--config=alias.log=log --config=hooks.pre-log=false'
  abort: option --config may not be abbreviated, used in aliases, or used as a value for another option
  [255]

  $ hg log -b '--config=defaults.log=--config=hooks.pre-log=false'
  abort: option --config may not be abbreviated, used in aliases, or used as a value for another option
  [255]

XXX: Should we support this?
Shell aliases bypass any command parsing rules but for the early one:

  $ hg log -b '--config=alias.log=!echo howdy'
  abort: option --config may not be abbreviated, used in aliases, or used as a value for another option
  [255]

For compatibility reasons, HGPLAIN=+strictflags is not enabled by plain HGPLAIN:

  $ HGPLAIN= hg log --config='hooks.pre-log=false' -b default
  abort: pre-log hook exited with status 1
  [255]
  $ HGPLAINEXCEPT= hg log --cwd .. -q -Ra -b default
  cb9a9f314b8b

[defaults]

  $ hg cat a
  a
  $ cat >> $HGRCPATH <<EOF
  > [defaults]
  > cat = -r null
  > EOF
  $ hg cat a
  [1]

  $ cd "$TESTTMP"

OSError "No such file or directory" / "The system cannot find the path
specified" should include filename even when it is empty

  $ hg -R a archive ''
  abort:* (glob)
  [255]

#if no-outer-repo

No repo:

  $ hg cat
  abort: '$TESTTMP' is not inside a repository, but this command requires a repository!
  (use 'cd' to go to a directory inside a repository and try again)
  [255]

#endif

#if rmcwd bash

Current directory removed:

  $ mkdir $TESTTMP/repo1
  $ cd $TESTTMP/repo1
  $ rm -rf $TESTTMP/repo1

The output could be one of the following and something else:
 chg: abort: failed to getcwd (errno = *) (glob)
 abort: error getting current working directory: * (glob)
 sh: 0: getcwd() failed: $ENOENT$
Since the exact behavior depends on the shell, only check it returns non-zero.
  $ HGDEMANDIMPORT=disable hg version -q 2>/dev/null || false
  [1]

#endif
