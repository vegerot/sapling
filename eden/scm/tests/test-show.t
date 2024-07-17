
#require no-eden



We assume that log basically works (it has its own tests). This just covers uses
of show that might break even if log works.

Show on empty repository: checking consistency

  $ hg init empty
  $ cd empty
  $ hg show
  commit:      000000000000
  user:        
  date:        Thu Jan 01 00:00:00 1970 +0000
  
  

Add log alias to and make sure show still works
  $ hg show --config alias.log=log
  commit:      000000000000
  user:        
  date:        Thu Jan 01 00:00:00 1970 +0000
  
  

  $ hg show 1
  abort: unknown revision '1'!
  [255]
  $ hg show 'branch(name)'
  abort: unknown revision branch(name)
  (if branch(name) is a file, try `hg show . branch(name)`)
  [255]
  $ hg show null -q
  commit:      000000000000
  user:        
  date:        Thu Jan 01 00:00:00 1970 +0000
  
  
Check various git-like options:

  $ hg init gitlike
  $ echo one > one
  $ echo two > two
  $ hg commit -qAm twofiles
  $ hg show --template status
  commit:      bf7b98b60f6f
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  description:
  twofiles
  
  files:
  A one
  A two
  
  diff -r 000000000000 -r bf7b98b60f6f one
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/one	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +one
  diff -r 000000000000 -r bf7b98b60f6f two
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/two	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +two
  

Check that the command parser always treats the first argument as a revision:

  $ hg show two
  abort: unknown revision 'two'!
  [255]
  $ hg show . two
  commit:      bf7b98b60f6f
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       one two
  description:
  twofiles
  
  
  diff -r 000000000000 -r bf7b98b60f6f two
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/two	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +two
  

Check --stat

  $ hg init stat
  $ cd stat
  $ echo show > x
  $ hg commit -qAm x
  $ hg show --stat
  commit:      852a8d467a01
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       x
  description:
  x
  
  
   x |  1 +
   1 files changed, 1 insertions(+), 0 deletions(-)
  




  $ echo more >> x
  $ hg commit -qAm longer
  $ hg show --stat
  commit:      b73358b94785
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       x
  description:
  longer
  
  
   x |  1 +
   1 files changed, 1 insertions(+), 0 deletions(-)
  




  $ echo remove > x
  $ hg commit -qAm remove
  $ hg show --stat
  commit:      3d74ea61c11c
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       x
  description:
  remove
  
  
   x |  3 +--
   1 files changed, 1 insertions(+), 2 deletions(-)
  



  $ hg show --stat 'desc(x)'
  commit:      852a8d467a01
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       x
  description:
  x
  
  
   x |  1 +
   1 files changed, 1 insertions(+), 0 deletions(-)
  
Check --unified and -U

  $ hg init diff
  $ cd diff
  $ cat >file <<EOF
  > line1
  > line2
  > line3
  > line4
  > line5
  > EOF
  $ hg commit -qAm file
  $ cat >>file <<EOF
  > line6
  > line7
  > line8
  > line9
  > line10
  > EOF
  $ hg commit -qm file
  $ hg show --unified=1
  commit:      8e33115c1596
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r fd78c1ae39e0 -r 8e33115c1596 file
  --- a/file	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file	Thu Jan 01 00:00:00 1970 +0000
  @@ -5,1 +5,6 @@
   line5
  +line6
  +line7
  +line8
  +line9
  +line10
  
  $ hg show --unified=2
  commit:      8e33115c1596
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r fd78c1ae39e0 -r 8e33115c1596 file
  --- a/file	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file	Thu Jan 01 00:00:00 1970 +0000
  @@ -4,2 +4,7 @@
   line4
   line5
  +line6
  +line7
  +line8
  +line9
  +line10
  

Check behavior with nonsensical integers.
  $ hg show --unified=-7
  commit:      8e33115c1596
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r fd78c1ae39e0 -r 8e33115c1596 file
  --- a/file	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file	Thu Jan 01 00:00:00 1970 +0000
  @@ -13,-14 +13,-9 @@
  +line6
  +line7
  +line8
  +line9
  +line10
  



Check whitespace handling options
  $ hg init whitespace
  $ cd whitespace
  $ echo "some  text" > file
  $ hg commit -qAm file
  $ echo "some text " > file
  $ hg commit -qAm file
  $ hg show
  commit:      6dbf2c12e2e2
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r 5b445d2a372e -r 6dbf2c12e2e2 file
  --- a/file	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file	Thu Jan 01 00:00:00 1970 +0000
  @@ -1,1 +1,1 @@
  -some  text
  +some text 
  
  $ hg show -b
  commit:      6dbf2c12e2e2
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  
  $ echo "some text" > file
  $ hg commit -qAm file
  $ hg show -Z
  commit:      600038806867
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  
  $ echo "some text " > file
  $ hg commit -qAm file
  $ hg show -Z
  commit:      747594f0817c
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  

  $ printf "some\n\ntext" > file
  $ hg commit -qAm file
  $ printf "some\ntext" > file
  $ hg commit -qAm file
  $ hg show -B
  commit:      10f3fc1d00d6
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  

Check --git and -g

  $ hg init git
  $ cd git
  $ echo git > file
  $ hg commit -qAm file
  $ hg show --git
  commit:      2a575d662478
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff --git a/file b/file
  new file mode 100644
  --- /dev/null
  +++ b/file
  @@ -0,0 +1,1 @@
  +git
  


  $ echo more >> file
  $ hg commit -qAm file
  $ hg show -g
  commit:      a23f7b259024
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff --git a/file b/file
  --- a/file
  +++ b/file
  @@ -1,1 +1,2 @@
   git
  +more
  


  $ hg show -g 2a575d662478590c06bc0cb3988882b46c0b2fee
  commit:      2a575d662478
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff --git a/file b/file
  new file mode 100644
  --- /dev/null
  +++ b/file
  @@ -0,0 +1,1 @@
  +git
  


Check nodates
  $ hg show --nodates
  commit:      a23f7b259024
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r 2a575d662478 -r a23f7b259024 file
  --- a/file
  +++ b/file
  @@ -1,1 +1,2 @@
   git
  +more
  

Check noprefix
  $ hg show --noprefix
  commit:      a23f7b259024
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       file
  description:
  file
  
  
  diff -r 2a575d662478 -r a23f7b259024 file
  --- file	Thu Jan 01 00:00:00 1970 +0000
  +++ file	Thu Jan 01 00:00:00 1970 +0000
  @@ -1,1 +1,2 @@
   git
  +more
  

Check hg show '' fails to parse the revision

  $ hg show ''
  hg: parse error: empty query
  [255]

Confirm that --help works (it didn't when we used an alias)

  $ hg show --help
  hg show [OPTION]... [REV [FILE]...]
  
  show commit in detail
  
      Show the commit message and contents for the specified commit. If no
      commit is specified, shows the current commit.
  
      'hg show' behaves similarly to 'hg log -vp -r REV [OPTION]... [FILE]...',
      or if called without a "REV", 'hg log -vp -r . [OPTION]...' Use 'hg log'
      for more powerful operations than supported by 'hg show'.
  
  Options ([+] can be repeated):
  
      --nodates             omit dates from diff headers (but keeps it in commit
                            header)
      --noprefix            omit a/ and b/ prefixes from filenames
      --stat                output diffstat-style summary of changes
   -g --git                 use git extended diff format
   -U --unified VALUE       number of lines of diff context to show (default: 3)
   -w --ignore-all-space    ignore white space when comparing lines
   -b --ignore-space-change ignore changes in the amount of white space
   -B --ignore-blank-lines  ignore changes whose lines are all blank
   -Z --ignore-space-at-eol ignore changes in whitespace at EOL
   -T --template TEMPLATE   display with template
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)
  $ hg show --help --verbose
  hg show [OPTION]... [REV [FILE]...]
  
  show commit in detail
  
      Show the commit message and contents for the specified commit. If no
      commit is specified, shows the current commit.
  
      'hg show' behaves similarly to 'hg log -vp -r REV [OPTION]... [FILE]...',
      or if called without a "REV", 'hg log -vp -r . [OPTION]...' Use 'hg log'
      for more powerful operations than supported by 'hg show'.
  
  Options ([+] can be repeated):
  
      --nodates             omit dates from diff headers (but keeps it in commit
                            header)
      --noprefix            omit a/ and b/ prefixes from filenames
      --stat                output diffstat-style summary of changes
   -g --git                 use git extended diff format
   -U --unified VALUE       number of lines of diff context to show (default: 3)
   -w --ignore-all-space    ignore white space when comparing lines
   -b --ignore-space-change ignore changes in the amount of white space
   -B --ignore-blank-lines  ignore changes whose lines are all blank
   -Z --ignore-space-at-eol ignore changes in whitespace at EOL
      --style STYLE         display using template map file (DEPRECATED)
   -T --template TEMPLATE   display with template
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  Global options ([+] can be repeated):
  
   -R --repository REPO       repository root directory or name of overlay
                              bundle file
      --cwd DIR               change working directory
   -y --noninteractive        do not prompt, automatically pick the first choice
                              for all prompts
   -q --quiet                 suppress output
   -v --verbose               enable additional output
      --color TYPE            when to colorize (boolean, always, auto, never, or
                              debug)
      --config CONFIG [+]     set/override config option (use
                              'section.name=value')
      --configfile FILE [+]   enables the given config file
      --debug                 enable debugging output
      --debugger              start debugger
      --encoding ENCODE       set the charset encoding (default: utf-8)
      --encodingmode MODE     set the charset encoding mode (default: strict)
      --insecure              do not verify server certificate
      --outputencoding ENCODE set the output encoding (default: utf-8)
      --traceback             always print a traceback on exception
      --trace                 enable more detailed tracing
      --time                  time how long the command takes
      --profile               print command execution profile
      --version               output version information and exit
   -h --help                  display help and exit
      --hidden                consider hidden changesets
      --pager TYPE            when to paginate (boolean, always, auto, or never)
                              (default: auto)
      --reason VALUE [+]      why this runs, usually set by automation
                              (ADVANCED)
