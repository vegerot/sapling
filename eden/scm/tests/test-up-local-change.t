#debugruntest-compatible
  $ setconfig experimental.allowfilepeer=True

  $ HGMERGE=true; export HGMERGE

  $ eagerepo

  $ hg init r1 --config format.use-eager-repo=True
  $ cd r1
  $ echo a > a
  $ hg addremove
  adding a
  $ hg commit -m "1"
  $ hg book main

  $ newclientrepo r2 ~/r1 main
  $ hg up
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ echo abc > a
  $ hg diff --nodates
  diff -r c19d34741b0a a
  --- a/a
  +++ b/a
  @@ -1,1 +1,1 @@
  -a
  +abc

  $ cd ../r1
  $ echo b > b
  $ echo a2 > a
  $ hg addremove
  adding b
  $ hg commit -m "2"

  $ cd ../r2
  $ hg pull
  pulling from $TESTTMP/r1
  searching for changes
  $ hg status
  M a
  $ hg parents
  commit:      c19d34741b0a
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     1

  $ hg --debug up 'desc(2)' --merge
    searching for copies back to c19d34741b0a
    unmatched files in other:
     b
  resolving manifests
   branchmerge: False, force: False
   ancestor: c19d34741b0a, local: c19d34741b0a+, remote: 1e71731e6fbb
   preserving a for resolve of a
   a: versions differ -> m (premerge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  merging a
  my a@c19d34741b0a+ other a@1e71731e6fbb ancestor a@c19d34741b0a
   a: versions differ -> m (merge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  my a@c19d34741b0a+ other a@1e71731e6fbb ancestor a@c19d34741b0a
  launching merge tool: true *$TESTTMP/r2/a* * * (glob)
  merge tool returned: 0
  1 files updated, 1 files merged, 0 files removed, 0 files unresolved
  $ hg parents
  commit:      1e71731e6fbb
  bookmark:    remote/main
  hoistedname: main
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     2
  $ hg --debug up 'desc(1)' --merge
    searching for copies back to c19d34741b0a
    unmatched files in local (from topological common ancestor):
     b
  resolving manifests
   branchmerge: False, force: False
   ancestor: 1e71731e6fbb, local: 1e71731e6fbb+, remote: c19d34741b0a
   preserving a for resolve of a
   a: versions differ -> m (premerge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  merging a
  my a@1e71731e6fbb+ other a@c19d34741b0a ancestor a@1e71731e6fbb
   a: versions differ -> m (merge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  my a@1e71731e6fbb+ other a@c19d34741b0a ancestor a@1e71731e6fbb
  launching merge tool: true *$TESTTMP/r2/a* * * (glob)
  merge tool returned: 0
  0 files updated, 1 files merged, 1 files removed, 0 files unresolved
  $ hg parents
  commit:      c19d34741b0a
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     1
  
  $ hg --debug up
    searching for copies back to c19d34741b0a
    unmatched files in other:
     b
  resolving manifests
   branchmerge: False, force: False
   ancestor: c19d34741b0a, local: c19d34741b0a+, remote: 1e71731e6fbb
   preserving a for resolve of a
   a: versions differ -> m (premerge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  merging a
  my a@c19d34741b0a+ other a@1e71731e6fbb ancestor a@c19d34741b0a
   a: versions differ -> m (merge)
  picktool() hgmerge true
  picked tool 'true' for path=a binary=False symlink=False changedelete=False
  my a@c19d34741b0a+ other a@1e71731e6fbb ancestor a@c19d34741b0a
  launching merge tool: true *$TESTTMP/r2/a* * * (glob)
  merge tool returned: 0
  1 files updated, 1 files merged, 0 files removed, 0 files unresolved
  $ hg parents
  commit:      1e71731e6fbb
  bookmark:    remote/main
  hoistedname: main
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     2
  $ hg -v history
  commit:      1e71731e6fbb
  bookmark:    remote/main
  hoistedname: main
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       a b
  description:
  2
  
  
  commit:      c19d34741b0a
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       a
  description:
  1
  $ hg diff --nodates
  diff -r 1e71731e6fbb a
  --- a/a
  +++ b/a
  @@ -1,1 +1,1 @@
  -a2
  +abc


create a second head

  $ cd ../r1
  $ hg up 'desc(1)'
  1 files updated, 0 files merged, 1 files removed, 0 files unresolved
  (leaving bookmark main)
  $ echo b2 > b
  $ echo a3 > a
  $ hg addremove
  adding b
  $ hg commit -m "3"

  $ cd ../r2
  $ hg -q pull ../r1
  $ hg status
  M a
  $ hg parents
  commit:      1e71731e6fbb
  bookmark:    remote/main
  hoistedname: main
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     2
  $ hg --debug up main
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved

test conflicting untracked files

  $ hg up -qC 'desc(1)'
  $ echo untracked > b
  $ hg st
  ? b
  $ hg up 'desc(2)'
  b: untracked file differs
  abort: untracked files in working directory differ from files in requested revision
  [255]
  $ rm b

test conflicting untracked ignored file

  $ hg up -qC 'desc(1)'
  $ echo ignored > .gitignore
  $ hg add .gitignore
  $ hg ci -m 'add .gitignore'
  $ echo ignored > ignored
  $ hg add ignored
  $ hg ci -m 'add ignored file'

  $ hg up -q 'desc("add .gitignore")'
  $ echo untracked > ignored
  $ hg st
  $ hg up 'desc("add ignored file")'
  ignored: untracked file differs
  abort: untracked files in working directory differ from files in requested revision
  [255]

test a local add

  $ cd ..
  $ hg init a
  $ newclientrepo b ~/a
  $ cd ..
  $ echo a > b/a
  $ echo a > a/a
  $ hg --cwd a commit -A -m a
  adding a
  $ hg --cwd a book main
  $ cd b
  $ hg add a
  $ hg pull -u -B main
  pulling from $TESTTMP/a
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg st

test updating backwards through a rename

  $ hg mv a b
  $ hg ci -m b
  $ echo b > b
  $ hg up -q 'desc(a)'
  $ hg st
  M a
  $ hg diff --nodates
  diff -r cb9a9f314b8b a
  --- a/a
  +++ b/a
  @@ -1,1 +1,1 @@
  -a
  +b

test for superfluous filemerge of clean files renamed in the past

  $ hg up -qC tip
  $ echo c > c
  $ hg add c
  $ hg up -qt:fail 'desc(a)'

  $ cd ..
