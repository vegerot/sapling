#require git no-windows

  $ . $TESTDIR/git.sh
  $ setconfig diff.git=true ui.allowemptycommit=true
  $ setconfig workingcopy.rust-checkout=true

Prepare git repo

  $ git init -q -b main git-repo

  $ cd git-repo
  $ HGIDENTITY=sl drawdag --no-bookmarks << 'EOS'
  > A..C
  > EOS

Go forward

  $ sl go -q $A
  $ sl go -q $B

Status should be clean

  $ sl status

Go backward

  $ sl go -q $A
