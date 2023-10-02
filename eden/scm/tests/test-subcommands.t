#debugruntest-compatible

  $ eagerepo
  $ newext testcommands <<EOF
  > from sapling import registrar
  > cmdtable = {}
  > command = registrar.command(cmdtable)
  > @command('test', [], 'hg test SUBCOMMAND', subonly=True)
  > def test(ui, repo):
  >     """test command"""
  >     ui.status("test command called (should not happen)\n")
  > subcmd = test.subcommand(categories=[("First Category", ["one"])])
  > @subcmd('one', [])
  > def testone(ui, repo):
  >     """first test subcommand"""
  >     ui.status("test subcommand one called\n")
  > @subcmd('two', [])
  > def testone(ui, repo):
  >     """second test subcommand"""
  >     ui.status("test subcommand two called\n")
  > @command('othertest', [], 'hg othertest [SUBCOMMAND]')
  > def othertest(ui, repo, parameter):
  >     """other test command"""
  >     ui.status("other test command called with '%s'\n" % parameter)
  > othersubcmd = othertest.subcommand()
  > @othersubcmd('alpha|alfa', [])
  > def othertestalpha(ui, repo, parameter):
  >     """other test subcommand alpha"""
  >     ui.status("other test command alpha called with '%s'\n" % parameter)
  > nestedsubcmd = othertestalpha.subcommand()
  > @nestedsubcmd('beta', [])
  > def othertestalphabeta(ui, repo):
  >     """other test subcommand alpha subcommand beta"""
  >     ui.status("other test command alpha/beta called\n")
  > EOF

  $ readconfig <<EOF
  > [alias]
  > xt = test
  > xt1 = test one
  > xt0 = test nonexistent
  > yt = othertest
  > yta = othertest alpha
  > ytf = othertest foo
  > EOF

  $ hg init

  $ hg test
  hg test: subcommand required
  [255]


  $ hg test one
  test subcommand one called
  $ hg test two
  test subcommand two called
  $ hg test nonexistent
  hg test: unknown subcommand 'nonexistent'
  [255]


  $ hg tes o
  unknown command 'tes'
  (use 'hg help' to get help)
  [255]

  $ hg xt
  hg test: subcommand required
  [255]


  $ hg xt one
  test subcommand one called
  $ hg xt too
  hg test: unknown subcommand 'too'
  [255]
  $ hg xt1
  test subcommand one called

  $ hg xt0
  hg test: unknown subcommand 'nonexistent'
  [255]

  $ hg othertest
  hg othertest: invalid arguments
  (use 'hg othertest -h' to get help)
  [255]
  $ hg othertest foo
  other test command called with 'foo'
  $ hg othertest alpha
  hg othertest alpha: invalid arguments
  (use 'hg othertest alpha -h' to get help)
  [255]
  $ hg othertest alfa foo
  other test command alpha called with 'foo'
  $ hg othertest alpha beta
  other test command alpha/beta called
  $ hg yt
  hg othertest: invalid arguments
  (use 'hg othertest -h' to get help)
  [255]
  $ hg yta foo
  other test command alpha called with 'foo'
  $ hg ytf
  other test command called with 'foo'

  $ hg help test
  hg test SUBCOMMAND
  
  test command
  
  First Category:
  
   one           first test subcommand
  
  Other Subcommands:
  
   two           second test subcommand
  
  (use 'hg help test SUBCOMMAND' to show complete subcommand help)
  
  (some details hidden, use --verbose to show complete help)


  $ hg help test --quiet
  hg test SUBCOMMAND
  
  test command
  
  First Category:
  
   one           first test subcommand
  
  Other Subcommands:
  
   two           second test subcommand


  $ hg help test one
  hg test one
  
  first test subcommand
  
  (some details hidden, use --verbose to show complete help)


  $ hg help test one --quiet
  hg test one
  
  first test subcommand

  $ hg help test two --verbose
  hg test two
  
  second test subcommand
  
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



  $ hg help test nonexistent
  abort: 'test' has no such subcommand: nonexistent
  (run 'hg help test' to see available subcommands)
  [255]
  $ hg othertest --help --verbose
  hg othertest [SUBCOMMAND]
  
  other test command
  
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
  
  Subcommands:
  
   alpha, alfa   other test subcommand alpha
  
  (use 'hg help othertest SUBCOMMAND' to show complete subcommand help)


  $ hg help xt
  alias for: test
  
  hg test SUBCOMMAND
  
  test command
  
  First Category:
  
   one           first test subcommand
  
  Other Subcommands:
  
   two           second test subcommand
  
  (use 'hg help test SUBCOMMAND' to show complete subcommand help)
  
  (some details hidden, use --verbose to show complete help)


  $ hg help xt one
  alias for: test one
  
  hg test one
  
  first test subcommand
  
  (some details hidden, use --verbose to show complete help)


  $ hg help xt1
  alias for: test one
  
  hg test one
  
  first test subcommand
  
  (some details hidden, use --verbose to show complete help)


  $ hg othertest alpha beta --help
  hg othertest alpha beta
  
  other test subcommand alpha subcommand beta
  
  (some details hidden, use --verbose to show complete help)


