
#require no-eden


  $ eagerepo
  $ configure mutation
  $ enable undo remotenames
  $ setconfig extensions.extralog="$TESTDIR/extralog.py"
  $ setconfig experimental.narrow-heads=true ui.interactive=true

  $ newrepo
  $ drawdag << 'EOS'
  > B
  > |
  > A
  > EOS

  $ drawdag << 'EOS'
  >   C
  >  /
  > A
  > EOS

  $ hg book -r $C book-C
  $ hg undo
  undone to *, before book -r * book-C (glob)
  $ hg undo
  undone to *, before debugdrawdag * (glob)
  $ hg log -GT '{desc}'
  o  B
  │
  o  A
  
  $ hg redo
  undone to *, before undo (glob)
  $ hg log -GT '{desc}'
  o  C
  │
  │ o  B
  ├─╯
  o  A
  
