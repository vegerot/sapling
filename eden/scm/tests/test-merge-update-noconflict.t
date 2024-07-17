#testcases rustcheckout pythoncheckout pythonrustcheckout

#if rustcheckout
  $ setconfig checkout.use-rust=true
#endif

#if pythoncheckout
  $ setconfig checkout.use-rust=false
  $ setconfig workingcopy.rust-checkout=false
#endif

#if pythonrustcheckout
  $ setconfig checkout.use-rust=false
  $ setconfig workingcopy.rust-checkout=true
#endif

  $ eagerepo
  $ enable amend rebase
  $ setconfig commands.update.check=noconflict

Updating w/ noconflict prints the conflicting changes:
  $ newrepo
  $ hg debugdrawdag <<'EOS'
  > c            # c/b = foo
  > |            # c/a = bar
  > b            # c/z = foo
  > |            # c/y = bar
  > |            # b/z = base
  > |            # b/y = base
  > a
  > EOS
  $ hg up b
  4 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (activating bookmark b)
  $ echo "conflict" | tee a b y z
  conflict
  $ hg up c
  abort: 4 conflicting file changes:
   a
   b
   y
   z
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]
