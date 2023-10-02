#debugruntest-compatible

  $ eagerepo

  $ HGFOO=BAR; export HGFOO
  $ readconfig <<'EOF'
  > [alias]
  > # should clobber ci but not commit (issue2993)
  > ci = version
  > myinit = init
  > mycommit = commit
  > optionalrepo = showconfig alias.myinit
  > cleanstatus = status -c
  > unknown = bargle
  > ambiguous = s
  > recursive = recursive
  > disabled = extorder
  > nodefinition =
  > noclosingquotation = '
  > no--cwd = status --cwd elsewhere
  > no-R = status -R elsewhere
  > no--repo = status --repo elsewhere
  > no--repository = status --repository elsewhere
  > no--config = status --config a.config=1
  > mylog = log
  > lognull = log -r null
  > shortlog = log --template '{node|short} | {date|isodate}\n'
  > positional = log --template '{$2} {$1} | {date|isodate}\n'
  > dln = lognull --debug
  > nousage = rollback
  > put = export -r 0 -o "$FOO/%R.diff"
  > blank = !echo
  > self = !echo $0
  > echoall = !echo "$@"
  > echo1 = !echo $1
  > echo2 = !echo $2
  > echo13 = !echo $1 $3
  > echotokens = !printf "%s\n" "$@"
  > count = !hg log -r "$@" --template=. | wc -c | sed -e 's/ //g'
  > mcount = !hg log $@ --template=. | wc -c | sed -e 's/ //g'
  > rt = root
  > idalias = id
  > idaliaslong = id
  > idaliasshell = !echo test
  > parentsshell1 = !echo one
  > parentsshell2 = !echo two
  > escaped1 = !echo 'test$$test'
  > escaped2 = !echo "HGFOO is $$HGFOO"
  > escaped3 = !echo $1 is $$$1
  > escaped4 = !echo \$$0 \$$@
  > exit1 = !sh -c 'exit 1'
  > documented = id
  > documented:doc = an alias for the id command
  > [defaults]
  > mylog = -q
  > lognull = -q
  > log = -v
  > EOF


basic

  $ hg myinit alias


unknown

  $ hg unknown
  unknown command 'bargle'
  (use 'hg help' to get help)
  [255]
  $ hg help unknown
  alias for: bargle
  
  abort: no such help topic: unknown
  (try 'hg help --keyword unknown')
  [255]


ambiguous

  $ hg ambiguous
  unknown command 's'
  (use 'hg help' to get help)
  [255]
  $ hg help ambiguous
  alias for: s
  
  abort: no such help topic: ambiguous
  (try 'hg help --keyword ambiguous')
  [255]


recursive

  $ hg recursive
  unknown command 'recursive'
  (use 'hg help' to get help)
  [255]
  $ hg help recursive
  abort: no such help topic: recursive
  (try 'hg help --keyword recursive')
  [255]


disabled

  $ hg disabled
  unknown command 'extorder'
  (use 'hg help' to get help)
  [255]
  $ hg help disabled
  alias for: extorder
  
  abort: no such help topic: disabled
  (try 'hg help --keyword disabled')
  [255]





no definition

  $ hg nodef
  unknown command 'nodef'
  (use 'hg help' to get help)
  [255]
  $ hg help nodef
  abort: no such help topic: nodef
  (try 'hg help --keyword nodef')
  [255]


no closing quotation

  $ hg noclosing
  unknown command 'noclosing'
  (use 'hg help' to get help)
  [255]
  $ hg help noclosing
  abort: no such help topic: noclosing
  (try 'hg help --keyword noclosing')
  [255]

"--" in alias definition should be preserved

  $ hg --config alias.dash='cat --' -R alias dash -r0
  abort: -r0 not under root '$TESTTMP/alias'
  [255]

invalid options

  $ hg init
  $ hg no--cwd
  abort: option --cwd may not be abbreviated or used in aliases
  [255]
  $ hg help no--cwd
  alias for: status --cwd elsewhere
  
  hg status [OPTION]... [FILE]...
  
  aliases: st
  
  list files with pending changes
  
      Show status of files in the working copy using the following status
      indicators:
  
        M = modified
        A = added
        R = removed
        C = clean
        ! = missing (deleted by a non-hg command, but still tracked)
        ? = not tracked
        I = ignored
          = origin of the previous file (with --copies)
  
      By default, shows files that have been modified, added, removed, deleted,
      or that are unknown (corresponding to the options "-mardu", respectively).
      Files that are unmodified, ignored, or the source of a copy/move operation
      are not listed.
  
      To control the exact statuses that are shown, specify the relevant flags
      (like "-rd" to show only files that are removed or deleted). Additionally,
      specify "-q/--quiet" to hide both unknown and ignored files.
  
      To show the status of specific files, provide a list of files to match. To
      include or exclude files using patterns or filesets, use "-I" or "-X".
  
      If "--rev" is specified and only one revision is given, it is used as the
      base revision. If two revisions are given, the differences between them
      are shown. The "--change" option can also be used as a shortcut to list
      the changed files of a revision from its first parent.
  
      Note:
         'hg status' might appear to disagree with 'hg diff' if permissions have
         changed or a merge has occurred, because the standard diff format does
         not report permission changes and 'hg diff' only reports changes
         relative to one merge parent.
  
      Returns 0 on success.
  
  Options ([+] can be repeated):
  
   -A --all                 show status of all files
   -m --modified            show only modified files
   -a --added               show only added files
   -r --removed             show only removed files
   -d --deleted             show only deleted (but tracked) files
   -c --clean               show only files without changes
   -u --unknown             show only unknown (not tracked) files
   -i --ignored             show only ignored files
   -n --no-status           hide status prefix
   -C --copies              show source of copied files
   -0 --print0              end filenames with NUL, for use with xargs
      --rev REV [+]         show difference from revision
      --change REV          list the changed files of a revision
      --root-relative       show status relative to root
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)
  $ hg no-R
  abort: option -R must appear alone, and --repository may not be abbreviated or used in aliases
  [255]
  $ hg help no-R
  alias for: status -R elsewhere
  
  hg status [OPTION]... [FILE]...
  
  aliases: st
  
  list files with pending changes
  
      Show status of files in the working copy using the following status
      indicators:
  
        M = modified
        A = added
        R = removed
        C = clean
        ! = missing (deleted by a non-hg command, but still tracked)
        ? = not tracked
        I = ignored
          = origin of the previous file (with --copies)
  
      By default, shows files that have been modified, added, removed, deleted,
      or that are unknown (corresponding to the options "-mardu", respectively).
      Files that are unmodified, ignored, or the source of a copy/move operation
      are not listed.
  
      To control the exact statuses that are shown, specify the relevant flags
      (like "-rd" to show only files that are removed or deleted). Additionally,
      specify "-q/--quiet" to hide both unknown and ignored files.
  
      To show the status of specific files, provide a list of files to match. To
      include or exclude files using patterns or filesets, use "-I" or "-X".
  
      If "--rev" is specified and only one revision is given, it is used as the
      base revision. If two revisions are given, the differences between them
      are shown. The "--change" option can also be used as a shortcut to list
      the changed files of a revision from its first parent.
  
      Note:
         'hg status' might appear to disagree with 'hg diff' if permissions have
         changed or a merge has occurred, because the standard diff format does
         not report permission changes and 'hg diff' only reports changes
         relative to one merge parent.
  
      Returns 0 on success.
  
  Options ([+] can be repeated):
  
   -A --all                 show status of all files
   -m --modified            show only modified files
   -a --added               show only added files
   -r --removed             show only removed files
   -d --deleted             show only deleted (but tracked) files
   -c --clean               show only files without changes
   -u --unknown             show only unknown (not tracked) files
   -i --ignored             show only ignored files
   -n --no-status           hide status prefix
   -C --copies              show source of copied files
   -0 --print0              end filenames with NUL, for use with xargs
      --rev REV [+]         show difference from revision
      --change REV          list the changed files of a revision
      --root-relative       show status relative to root
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)
  $ hg no--repo
  abort: option -R must appear alone, and --repository may not be abbreviated or used in aliases
  [255]
  $ hg help no--repo
  alias for: status --repo elsewhere
  
  hg status [OPTION]... [FILE]...
  
  aliases: st
  
  list files with pending changes
  
      Show status of files in the working copy using the following status
      indicators:
  
        M = modified
        A = added
        R = removed
        C = clean
        ! = missing (deleted by a non-hg command, but still tracked)
        ? = not tracked
        I = ignored
          = origin of the previous file (with --copies)
  
      By default, shows files that have been modified, added, removed, deleted,
      or that are unknown (corresponding to the options "-mardu", respectively).
      Files that are unmodified, ignored, or the source of a copy/move operation
      are not listed.
  
      To control the exact statuses that are shown, specify the relevant flags
      (like "-rd" to show only files that are removed or deleted). Additionally,
      specify "-q/--quiet" to hide both unknown and ignored files.
  
      To show the status of specific files, provide a list of files to match. To
      include or exclude files using patterns or filesets, use "-I" or "-X".
  
      If "--rev" is specified and only one revision is given, it is used as the
      base revision. If two revisions are given, the differences between them
      are shown. The "--change" option can also be used as a shortcut to list
      the changed files of a revision from its first parent.
  
      Note:
         'hg status' might appear to disagree with 'hg diff' if permissions have
         changed or a merge has occurred, because the standard diff format does
         not report permission changes and 'hg diff' only reports changes
         relative to one merge parent.
  
      Returns 0 on success.
  
  Options ([+] can be repeated):
  
   -A --all                 show status of all files
   -m --modified            show only modified files
   -a --added               show only added files
   -r --removed             show only removed files
   -d --deleted             show only deleted (but tracked) files
   -c --clean               show only files without changes
   -u --unknown             show only unknown (not tracked) files
   -i --ignored             show only ignored files
   -n --no-status           hide status prefix
   -C --copies              show source of copied files
   -0 --print0              end filenames with NUL, for use with xargs
      --rev REV [+]         show difference from revision
      --change REV          list the changed files of a revision
      --root-relative       show status relative to root
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)
  $ hg no--repository
  abort: option -R must appear alone, and --repository may not be abbreviated or used in aliases
  [255]
  $ hg help no--repository
  alias for: status --repository elsewhere
  
  hg status [OPTION]... [FILE]...
  
  aliases: st
  
  list files with pending changes
  
      Show status of files in the working copy using the following status
      indicators:
  
        M = modified
        A = added
        R = removed
        C = clean
        ! = missing (deleted by a non-hg command, but still tracked)
        ? = not tracked
        I = ignored
          = origin of the previous file (with --copies)
  
      By default, shows files that have been modified, added, removed, deleted,
      or that are unknown (corresponding to the options "-mardu", respectively).
      Files that are unmodified, ignored, or the source of a copy/move operation
      are not listed.
  
      To control the exact statuses that are shown, specify the relevant flags
      (like "-rd" to show only files that are removed or deleted). Additionally,
      specify "-q/--quiet" to hide both unknown and ignored files.
  
      To show the status of specific files, provide a list of files to match. To
      include or exclude files using patterns or filesets, use "-I" or "-X".
  
      If "--rev" is specified and only one revision is given, it is used as the
      base revision. If two revisions are given, the differences between them
      are shown. The "--change" option can also be used as a shortcut to list
      the changed files of a revision from its first parent.
  
      Note:
         'hg status' might appear to disagree with 'hg diff' if permissions have
         changed or a merge has occurred, because the standard diff format does
         not report permission changes and 'hg diff' only reports changes
         relative to one merge parent.
  
      Returns 0 on success.
  
  Options ([+] can be repeated):
  
   -A --all                 show status of all files
   -m --modified            show only modified files
   -a --added               show only added files
   -r --removed             show only removed files
   -d --deleted             show only deleted (but tracked) files
   -c --clean               show only files without changes
   -u --unknown             show only unknown (not tracked) files
   -i --ignored             show only ignored files
   -n --no-status           hide status prefix
   -C --copies              show source of copied files
   -0 --print0              end filenames with NUL, for use with xargs
      --rev REV [+]         show difference from revision
      --change REV          list the changed files of a revision
      --root-relative       show status relative to root
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)
  $ hg no--config
  abort: option --config may not be abbreviated or used in aliases
  [255]
  $ hg no --config alias.no='--repo elsewhere --cwd elsewhere status'
  unknown command '--repo'
  (use 'hg help' to get help)
  [255]
  $ hg no --config alias.no='--repo elsewhere'
  unknown command '--repo'
  (use 'hg help' to get help)
  [255]

optional repository

#if no-outer-repo
  $ hg optionalrepo
  init
#endif
  $ cd alias
  $ cat > .hg/hgrc <<EOF
  > [alias]
  > myinit = init -q
  > EOF
  $ hg optionalrepo
  init -q

no usage

  $ hg nousage
  abort: rollback is dangerous and should not be used
  [255]

  $ echo foo > foo
  $ hg commit -Amfoo
  adding foo

infer repository

  $ cd ..

#if no-outer-repo
  $ hg shortlog alias/foo
  0 e63c23eaa88a | 1970-01-01 00:00 +0000
#endif

  $ cd alias

with opts

  $ hg cleanst
  unknown command 'cleanst'
  (use 'hg help' to get help)
  [255]


with opts and whitespace

  $ hg shortlog
  e63c23eaa88a | 1970-01-01 00:00 +0000

interaction with defaults

  $ hg mylog
  commit:      e63c23eaa88a
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     foo
  
  $ hg lognull
  commit:      000000000000
  user:        
  date:        Thu Jan 01 00:00:00 1970 +0000
  


properly recursive

  $ hg dln
  commit:      0000000000000000000000000000000000000000
  phase:       public
  manifest:    0000000000000000000000000000000000000000
  user:        
  date:        Thu Jan 01 00:00:00 1970 +0000
  extra:       branch=default
  

simple shell aliases

  $ hg blank
  

  $ hg blank foo
  

  $ hg self
  self
  $ hg echoall
  

  $ hg echoall foo
  foo
  $ hg echoall 'test $2' foo
  test $2 foo
  $ hg echoall 'test $@' foo '$@'
  test $@ foo $@
  $ hg echoall 'test "$@"' foo '"$@"'
  test "$@" foo "$@"
  $ hg echo1 foo bar baz
  foo
  $ hg echo2 foo bar baz
  bar
  $ hg echo13 foo bar baz test
  foo baz
  $ hg echo2 foo
  

  $ hg echotokens
  

  $ hg echotokens foo 'bar $1 baz'
  foo
  bar $1 baz
  $ hg echotokens 'test $2' foo
  test $2
  foo
  $ hg echotokens 'test $@' foo '$@'
  test $@
  foo
  $@
  $ hg echotokens 'test "$@"' foo '"$@"'
  test "$@"
  foo
  "$@"
  $ echo bar > bar
  $ hg commit -qA -m bar
  $ hg count .
  1
  $ hg count 'branch(default)'
  2
  $ hg mcount -r '"branch(default)"'
  2

  $ tglog
  @  c0c7cf58edc5 'bar'
  │
  o  e63c23eaa88a 'foo'
  



shadowing

  $ hg i
  unknown command 'i'
  (use 'hg help' to get help)
  [255]
  $ hg id
  c0c7cf58edc5
  $ hg ida
  unknown command 'ida'
  (use 'hg help' to get help)
  [255]
  $ hg idalias
  c0c7cf58edc5
  $ hg idaliasl
  unknown command 'idaliasl'
  (use 'hg help' to get help)
  [255]
  $ hg idaliass
  unknown command 'idaliass'
  (use 'hg help' to get help)
  [255]
  $ hg parentsshell
  unknown command 'parentsshell'
  (use 'hg help' to get help)
  [255]
  $ hg parentsshell1
  one
  $ hg parentsshell2
  two


shell aliases with global options

  $ hg init sub
  $ cd sub
  $ hg count 'branch(default)'
  0
  $ hg -v count 'branch(default)'
  0
  $ hg -R .. count 'branch(default)'
  warning: --repository ignored
  0
  $ hg --cwd .. count 'branch(default)'
  2

global flags after the shell alias name is passed to the shell command, not handled by hg

  $ hg echoall --cwd ..
  abort: option --cwd may not be abbreviated!
  [255]


"--" passed to shell alias should be preserved

  $ hg --config alias.printf='!printf "$@"' printf '%s %s %s\n' -- --cwd ..
  -- --cwd ..

repo specific shell aliases

  $ cat >> .hg/hgrc <<EOF
  > [alias]
  > subalias = !echo sub
  > EOF
  $ cat >> ../.hg/hgrc <<EOF
  > [alias]
  > mainalias = !echo main
  > EOF


shell alias defined in current repo

  $ hg subalias
  sub
  $ hg --cwd .. subalias > /dev/null
  unknown command 'subalias'
  (use 'hg help' to get help)
  [255]
  $ hg -R .. subalias > /dev/null
  unknown command 'subalias'
  (use 'hg help' to get help)
  [255]


shell alias defined in other repo

  $ hg mainalias > /dev/null
  unknown command 'mainalias'
  (use 'hg help' to get help)
  [255]
  $ hg -R .. mainalias
  warning: --repository ignored
  main
  $ hg --cwd .. mainalias
  main

typos get useful suggestions
  $ hg --cwd .. manalias
  unknown command 'manalias'
  (use 'hg help' to get help)
  [255]

shell aliases with escaped $ chars

  $ hg escaped1
  test$test
  $ hg escaped2
  HGFOO is BAR
  $ hg escaped3 HGFOO
  HGFOO is BAR
  $ hg escaped4 test
  $0 $@

abbreviated name, which matches against both shell alias and the
command provided extension, should be aborted.

  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > rebase =
  > EOF
  $ cat >> .hg/hgrc <<'EOF'
  > [alias]
  > rebate = !echo this is rebate $@
  > EOF

  $ hg rebat
  unknown command 'rebat'
  (use 'hg help' to get help)
  [255]
  $ hg rebat --foo-bar
  unknown command 'rebat'
  (use 'hg help' to get help)
  [255]

invalid arguments

  $ hg rt foo
  abort: invalid arguments
  (use '--help' to get help)
  [255]

invalid global arguments for normal commands, aliases, and shell aliases

  $ hg --invalid root
  unknown command '--invalid'
  (use 'hg help' to get help)
  [255]
  $ hg --invalid mylog
  unknown command '--invalid'
  (use 'hg help' to get help)
  [255]
  $ hg --invalid blank
  unknown command '--invalid'
  (use 'hg help' to get help)
  [255]

This should show id:

  $ hg --config alias.log='id' log
  000000000000

This shouldn't:

  $ hg --config alias.log='id' history

  $ cd ../..

return code of command and shell aliases:

  $ hg mycommit -R alias
  nothing changed
  [1]
  $ hg exit1
  [1]

documented aliases

  $ newrepo
  $ hg documented:doc
  unknown command 'documented:doc'
  (use 'hg help' to get help)
  [255]

  $ hg help documented
  [^ ].* (re) (?)
  
  an alias for the id command
  
  hg identify [-nibtB] [-r REV] [SOURCE]
  
  aliases: id
  
  identify the working directory or specified revision
  
      Print a summary identifying the repository state at REV using one or two
      parent hash identifiers, followed by a "+" if the working directory has
      uncommitted changes and a list of bookmarks.
  
      When REV is not given, print a summary of the current state of the
      repository.
  
      Specifying a path to a repository root or Mercurial bundle will cause
      lookup to operate on that repository/bundle.
  
      See 'hg log' for generating more information about specific revisions,
      including full hash identifiers.
  
      Returns 0 if successful.
  
  Options:
  
   -r --rev REV   identify the specified revision
   -n --num       show local revision number
   -i --id        show global revision id
   -B --bookmarks show bookmarks
  
  (some details hidden, use --verbose to show complete help)












  $ hg help commands | grep documented
  [1]
