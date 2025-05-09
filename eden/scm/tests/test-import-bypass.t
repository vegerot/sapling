
#require no-eden



  $ shortlog() {
  >     hg log -G --template '{node|short} {author} {date|hgdate} - {desc|firstline}\n'
  > }
  $ enable amend

Test --bypass with other options

  $ newclientrepo repo-options server
  $ echo a > a
  $ hg ci -Am adda
  adding a
  $ echo a >> a
  $ hg ci -Am changea
  $ hg push -r . -q --to head1 --create
  $ hg export . > ../test.diff
  $ hg up null
  0 files updated, 0 files merged, 1 files removed, 0 files unresolved

Test importing an existing revision
(this also tests that "hg import" disallows combination of '--exact'
and '--edit')

  $ hg import --bypass --exact --edit ../test.diff
  abort: cannot use --exact with --edit
  [255]
  $ hg import --bypass --exact ../test.diff
  applying ../test.diff
  $ shortlog
  o  540395c44225 test 0 0 - changea
  │
  o  07f494440405 test 0 0 - adda
  

Test failure without --exact

  $ hg import --bypass ../test.diff
  applying ../test.diff
  unable to find 'a' for patching
  (use '--prefix' to apply patch relative to the current directory)
  abort: patch failed to apply
  [255]
  $ hg st
  $ shortlog
  o  540395c44225 test 0 0 - changea
  │
  o  07f494440405 test 0 0 - adda
  

Test --user, --date and --message

  $ hg up 'desc(adda)'
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg import --bypass --u test2 -d '1 0' -m patch2 ../test.diff
  applying ../test.diff
  $ cat .hg/last-message.txt
  patch2 (no-eol)
  $ shortlog
  o  2e127d1da504 test2 1 0 - patch2
  │
  │ o  540395c44225 test 0 0 - changea
  ├─╯
  @  07f494440405 test 0 0 - adda
  
  $ hg hide -q tip

Test --strip

  $ hg import --bypass --strip 0 - <<EOF
  > # HG changeset patch
  > # User test
  > # Date 0 0
  > # Branch foo
  > # Node ID 4e322f7ce8e3e4203950eac9ece27bf7e45ffa6c
  > # Parent  07f4944404050f47db2e5c5071e0e84e7a27bba9
  > changea
  > 
  > diff -r 07f494440405 -r 4e322f7ce8e3 a
  > --- a	Thu Jan 01 00:00:00 1970 +0000
  > +++ a	Thu Jan 01 00:00:00 1970 +0000
  > @@ -1,1 +1,2 @@
  >  a
  > +a
  > EOF
  applying patch from stdin

Test --strip with --bypass

  $ mkdir -p dir/dir2
  $ echo bb > dir/dir2/b
  $ echo cc > dir/dir2/c
  $ echo d > dir/d
  $ hg ci -Am 'addabcd'
  adding dir/d
  adding dir/dir2/b
  adding dir/dir2/c
  $ shortlog
  @  8a9cc7e88ada test 0 0 - addabcd
  │
  │ o  540395c44225 test 0 0 - changea
  ├─╯
  o  07f494440405 test 0 0 - adda
  
  $ hg import --bypass --strip 2 --prefix dir/ - <<EOF
  > # HG changeset patch
  > # User test
  > # Date 0 0
  > # Branch foo
  > changeabcd
  > 
  > diff --git a/foo/a b/foo/a
  > new file mode 100644
  > --- /dev/null
  > +++ b/foo/a
  > @@ -0,0 +1 @@
  > +a
  > diff --git a/foo/dir2/b b/foo/dir2/b2
  > rename from foo/dir2/b
  > rename to foo/dir2/b2
  > diff --git a/foo/dir2/c b/foo/dir2/c
  > --- a/foo/dir2/c
  > +++ b/foo/dir2/c
  > @@ -0,0 +1 @@
  > +cc
  > diff --git a/foo/d b/foo/d
  > deleted file mode 100644
  > --- a/foo/d
  > +++ /dev/null
  > @@ -1,1 +0,0 @@
  > -d
  > EOF
  applying patch from stdin

  $ shortlog
  o  dded091def5d test 0 0 - changeabcd
  │
  @  8a9cc7e88ada test 0 0 - addabcd
  │
  │ o  540395c44225 test 0 0 - changea
  ├─╯
  o  07f494440405 test 0 0 - adda
  
  $ hg diff --change 'desc(changeabcd)' --git
  diff --git a/dir/a b/dir/a
  new file mode 100644
  --- /dev/null
  +++ b/dir/a
  @@ -0,0 +1,1 @@
  +a
  diff --git a/dir/d b/dir/d
  deleted file mode 100644
  --- a/dir/d
  +++ /dev/null
  @@ -1,1 +0,0 @@
  -d
  diff --git a/dir/dir2/b b/dir/dir2/b2
  rename from dir/dir2/b
  rename to dir/dir2/b2
  diff --git a/dir/dir2/c b/dir/dir2/c
  --- a/dir/dir2/c
  +++ b/dir/dir2/c
  @@ -1,1 +1,2 @@
   cc
  +cc
  $ hg -q debugstrip .

Test unsupported combinations

  $ hg import --bypass --no-commit ../test.diff
  abort: cannot use --no-commit with --bypass
  [255]
  $ hg import --bypass --similarity 50 ../test.diff
  abort: cannot use --similarity with --bypass
  [255]
  $ hg import --exact --prefix dir/ ../test.diff
  abort: cannot use --exact with --prefix
  [255]

Test commit editor
(this also tests that editor is invoked, if the patch doesn't contain
the commit message, regardless of '--edit')

  $ cat > ../test.diff <<EOF
  > diff -r 07f494440405 -r 4e322f7ce8e3 a
  > --- a/a	Thu Jan 01 00:00:00 1970 +0000
  > +++ b/a	Thu Jan 01 00:00:00 1970 +0000
  > @@ -1,1 +1,2 @@
  > -a
  > +b
  > +c
  > EOF
  $ HGEDITOR=cat hg import --bypass ../test.diff
  applying ../test.diff
  
  
  HG: Enter commit message.  Lines beginning with 'HG:' are removed.
  HG: Leave message empty to abort commit.
  HG: --
  HG: user: test
  HG: changed a
  abort: empty commit message
  [255]

Test patch.eol is handled
(this also tests that editor is not invoked for '--bypass', if the
commit message is explicitly specified, regardless of '--edit')

  $ printf "a\r\n" > a
  $ hg ci -m makeacrlf
  $ HGEDITOR=cat hg import -m 'should fail because of eol' --edit --bypass ../test.diff
  applying ../test.diff
  patching file a
  Hunk #1 FAILED at 0
  abort: patch failed to apply
  [255]
  $ hg --config patch.eol=auto import -d '0 0' -m 'test patch.eol' --bypass ../test.diff
  applying ../test.diff
  $ shortlog
  o  c606edafba99 test 0 0 - test patch.eol
  │
  @  872023de769d test 0 0 - makeacrlf
  │
  │ o  540395c44225 test 0 0 - changea
  ├─╯
  o  07f494440405 test 0 0 - adda
  

Test applying multiple patches

  $ hg up -qC 'desc(adda)'
  $ echo e > e
  $ hg ci -Am adde
  adding e
  $ hg export . > ../patch1.diff
  $ hg up -qC 'desc(changea)'
  $ echo f > f
  $ hg ci -Am addf
  adding f
  $ hg push -r . -q --to head2 --create
  $ hg export . > ../patch2.diff
  $ cd ..
  $ newclientrepo repo-multi1 server head1 head2
  $ hg up 'desc(adda)'
  1 files updated, 0 files merged, 1 files removed, 0 files unresolved
  $ hg import --bypass ../patch1.diff ../patch2.diff
  applying ../patch1.diff
  applying ../patch2.diff
  $ shortlog
  o  bc8ca3f8a7c4 test 0 0 - addf
  │
  o  16581080145e test 0 0 - adde
  │
  │ o  f24ac4984bef test 0 0 - addf
  │ │
  │ o  540395c44225 test 0 0 - changea
  ├─╯
  @  07f494440405 test 0 0 - adda
  

Test applying multiple patches with --exact

  $ newclientrepo repo-multi2 server head1 head2
  $ hg up 540395c44225
  0 files updated, 0 files merged, 1 files removed, 0 files unresolved
  $ hg import --bypass --exact ../patch1.diff ../patch2.diff
  applying ../patch1.diff
  applying ../patch2.diff
  $ shortlog
  o  16581080145e test 0 0 - adde
  │
  │ o  f24ac4984bef test 0 0 - addf
  │ │
  │ @  540395c44225 test 0 0 - changea
  ├─╯
  o  07f494440405 test 0 0 - adda
  

  $ cd ..

Test avoiding editor invocation at applying the patch with --exact
even if commit message is empty

  $ cd repo-options

  $ echo a >> a
  $ hg commit -m ' '
  $ hg tip -T "{node}\n"
  7bb02e5e6d9de292a9e1b1cb2af5911ed53a378f
  $ hg export -o ../empty-log.diff .
  $ hg goto -q -C ".^1"
  $ hg debugstrip -q tip
  $ HGEDITOR=cat hg import --exact --bypass ../empty-log.diff
  applying ../empty-log.diff
  $ hg tip -T "{node}\n"
  7bb02e5e6d9de292a9e1b1cb2af5911ed53a378f

  $ cd ..

#if symlink execbit

Test complicated patch with --exact

  $ newclientrepo repo-exact
  $ echo a > a
  $ echo c > c
  $ echo d > d
  $ echo e > e
  $ echo f > f
  $ chmod +x f
  $ ln -s c linkc
  $ hg ci -Am t
  adding a
  adding c
  adding d
  adding e
  adding f
  adding linkc
  $ hg cp a aa1
  $ echo b >> a
  $ echo b > b
  $ hg add b
  $ hg cp a aa2
  $ echo aa >> aa2
  $ chmod +x e
  $ chmod -x f
  $ ln -s a linka
  $ hg rm d
  $ hg rm linkc
  $ hg mv c cc
  $ hg ci -m patch
  $ hg export --git . > ../test.diff
  $ hg up -C null
  0 files updated, 0 files merged, 7 files removed, 0 files unresolved
  $ hg purge
  $ hg st
  $ hg import --bypass --exact ../test.diff
  applying ../test.diff

The patch should have matched the exported revision and generated no additional
data. If not, diff both heads to debug it.

  $ shortlog
  o  2978fd5c8aa4 test 0 0 - patch
  │
  o  a0e19e636a43 test 0 0 - t
  
#endif

