
#require no-eden


Test uncommit - set up the config

  $ eagerepo
  $ configure mutation-norecord
  $ enable amend

Build up a repo

  $ newrepo
  $ hg bookmark foo

Help for uncommit

  $ hg help uncommit
  hg uncommit [OPTION]... [FILE]...
  
  aliases: unc
  
  uncommit part or all of the current commit
  
      Reverse the effects of an 'hg commit' operation. When run with no
      arguments, hides the current commit and checks out the parent commit, but
      does not revert the state of the working copy. Changes that were contained
      in the uncommitted commit become pending changes in the working copy.
  
      'hg uncommit' cannot be run on commits that have children. In other words,
      you cannot uncommit a commit in the middle of a stack. Similarly, by
      default, you cannot run 'hg uncommit' if there are pending changes in the
      working copy.
  
      You can selectively uncommit files from the current commit by optionally
      specifying a list of files to remove. The specified files are removed from
      the list of changed files in the current commit, but are not modified on
      disk, so they appear as pending changes in the working copy.
  
      Note:
         Running 'hg uncommit' is similar to running 'hg undo --keep'
         immediately after 'hg commit'. However, unlike 'hg undo', which can
         only undo a commit if it was the last operation you performed, 'hg
         uncommit' can uncommit any draft commit in the graph that does not have
         children.
  
  Options ([+] can be repeated):
  
      --keep                allow an empty commit after uncommiting
   -I --include PATTERN [+] include files matching the given patterns
   -X --exclude PATTERN [+] exclude files matching the given patterns
  
  (some details hidden, use --verbose to show complete help)

Uncommit with no commits should fail

  $ hg uncommit
  abort: cannot uncommit null changeset
  (no changeset checked out)
  [255]

Create some commits

  $ touch files
  $ hg add files
  $ for i in a ab abc abcd abcde; do echo $i > files; echo $i > file-$i; hg add file-$i; hg commit -m "added file-$i"; done
  $ ls
  file-a
  file-ab
  file-abc
  file-abcd
  file-abcde
  files

  $ hg log -G -T '{node} {desc}' --hidden
  @  6c4fd43ed714e7fcd8adbaa7b16c953c2e985b60 added file-abcde
  │
  o  6db330d65db434145c0b59d291853e9a84719b24 added file-abcd
  │
  o  abf2df566fc193b3ac34d946e63c1583e4d4732b added file-abc
  │
  o  69a232e754b08d568c4899475faf2eb44b857802 added file-ab
  │
  o  3004d2d9b50883c1538fc754a3aeb55f1b4084f6 added file-a
  
Simple uncommit off the top, also moves bookmark

  $ hg bookmark
   * foo                       6c4fd43ed714
  $ hg uncommit
  $ hg status
  M files
  A file-abcde
  $ hg bookmark
   * foo                       6db330d65db4

  $ hg log -G -T '{node} {desc}' --hidden
  o  6c4fd43ed714e7fcd8adbaa7b16c953c2e985b60 added file-abcde
  │
  @  6db330d65db434145c0b59d291853e9a84719b24 added file-abcd
  │
  o  abf2df566fc193b3ac34d946e63c1583e4d4732b added file-abc
  │
  o  69a232e754b08d568c4899475faf2eb44b857802 added file-ab
  │
  o  3004d2d9b50883c1538fc754a3aeb55f1b4084f6 added file-a
  

Recommit

  $ hg commit -m 'new change abcde'
  $ hg status
  $ hg heads -T '{node} {desc}'
  0c07a3ccda771b25f1cb1edbd02e683723344ef1 new change abcde (no-eol)

Uncommit of non-existent and unchanged files has no effect
  $ hg uncommit nothinghere
  nothing to uncommit
  [1]
  $ hg status
  $ hg uncommit file-abc
  nothing to uncommit
  [1]
  $ hg status

Uncommit empty commit
  $ echo temp > temp && hg add temp && hg commit -m empty
  $ hg rm temp && hg amend
  $ hg diff -r .^
  $ hg uncommit

Try partial uncommit, also moves bookmark

  $ hg bookmark
   * foo                       0c07a3ccda77
  $ hg uncommit files
  $ hg status
  M files
  $ hg bookmark
   * foo                       3727deee06f7
  $ hg heads -T '{node} {desc}'
  3727deee06f72f5ffa8db792ee299cf39e3e190b new change abcde (no-eol)
  $ hg log -r . -p -T '{node} {desc}'
  3727deee06f72f5ffa8db792ee299cf39e3e190b new change abcdediff -r 6db330d65db4 -r 3727deee06f7 file-abcde
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file-abcde	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +abcde
  
  $ hg log -G -T '{node} {desc}' --hidden
  @  3727deee06f72f5ffa8db792ee299cf39e3e190b new change abcde
  │
  │ o  5d2fdaa86d070e06669eb141937268780b14861d empty
  │ │
  │ │ x  aa5000ec01a38397e1e2ad1eddc643c151459a9b empty
  │ ├─╯
  │ o  0c07a3ccda771b25f1cb1edbd02e683723344ef1 new change abcde
  ├─╯
  │ o  6c4fd43ed714e7fcd8adbaa7b16c953c2e985b60 added file-abcde
  ├─╯
  o  6db330d65db434145c0b59d291853e9a84719b24 added file-abcd
  │
  o  abf2df566fc193b3ac34d946e63c1583e4d4732b added file-abc
  │
  o  69a232e754b08d568c4899475faf2eb44b857802 added file-ab
  │
  o  3004d2d9b50883c1538fc754a3aeb55f1b4084f6 added file-a
  
  $ hg commit -m 'update files for abcde'

Uncommit with dirty state

  $ echo "foo" >> files
  $ cat files
  abcde
  foo
  $ hg status
  M files
  $ hg uncommit --config experimental.uncommitondirtywdir=False
  abort: uncommitted changes
  [255]
  $ hg uncommit files
  $ cat files
  abcde
  foo
  $ hg commit -m "files abcde + foo"

Testing with 'experimental.uncommitondirtywdir' on and off

  $ echo "bar" >> files
  $ hg uncommit  --config experimental.uncommitondirtywdir=False
  abort: uncommitted changes
  [255]
  $ hg uncommit
  $ hg commit -m "files abcde + foo"

Uncommit in the middle of a stack, does not move bookmark

  $ hg checkout '.^^^'
  1 files updated, 0 files merged, 2 files removed, 0 files unresolved
  (leaving bookmark foo)
  $ hg log -r . -p -T '{node} {desc}'
  abf2df566fc193b3ac34d946e63c1583e4d4732b added file-abcdiff -r 69a232e754b0 -r abf2df566fc1 file-abc
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file-abc	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +abc
  diff -r 69a232e754b0 -r abf2df566fc1 files
  --- a/files	Thu Jan 01 00:00:00 1970 +0000
  +++ b/files	Thu Jan 01 00:00:00 1970 +0000
  @@ -1,1 +1,1 @@
  -ab
  +abc
  
  $ hg bookmark
     foo                       48e5bd7cd583
  $ hg uncommit
  $ hg status
  M files
  A file-abc
  $ hg heads -T '{node} {desc}'
  48e5bd7cd583eb24164ef8b89185819c84c96ed7 files abcde + foo (no-eol)
  $ hg bookmark
     foo                       48e5bd7cd583
  $ hg commit -m 'new abc'

Partial uncommit in the middle, does not move bookmark

  $ hg checkout '.^'
  1 files updated, 0 files merged, 1 files removed, 0 files unresolved
  $ hg log -r . -p -T '{node} {desc}'
  69a232e754b08d568c4899475faf2eb44b857802 added file-abdiff -r 3004d2d9b508 -r 69a232e754b0 file-ab
  --- /dev/null	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file-ab	Thu Jan 01 00:00:00 1970 +0000
  @@ -0,0 +1,1 @@
  +ab
  diff -r 3004d2d9b508 -r 69a232e754b0 files
  --- a/files	Thu Jan 01 00:00:00 1970 +0000
  +++ b/files	Thu Jan 01 00:00:00 1970 +0000
  @@ -1,1 +1,1 @@
  -a
  +ab
  
  $ hg bookmark
     foo                       48e5bd7cd583
  $ hg uncommit file-ab
  $ hg status
  A file-ab

  $ hg heads -T '{node} {desc}\n'
  8eb87968f2edb7f27f27fe676316e179de65fff6 added file-ab
  5dc89ca4486f8a88716c5797fa9f498d13d7c2e1 new abc
  48e5bd7cd583eb24164ef8b89185819c84c96ed7 files abcde + foo

  $ hg bookmark
     foo                       48e5bd7cd583
  $ hg commit -m 'update ab'
  $ hg status
  $ hg heads -T '{node} {desc}\n'
  f21039c59242b085491bb58f591afc4ed1c04c09 update ab
  5dc89ca4486f8a88716c5797fa9f498d13d7c2e1 new abc
  48e5bd7cd583eb24164ef8b89185819c84c96ed7 files abcde + foo

  $ hg log -G -T '{node} {desc}' --hidden
  @  f21039c59242b085491bb58f591afc4ed1c04c09 update ab
  │
  o  8eb87968f2edb7f27f27fe676316e179de65fff6 added file-ab
  │
  │ o  5dc89ca4486f8a88716c5797fa9f498d13d7c2e1 new abc
  │ │
  │ │ o  48e5bd7cd583eb24164ef8b89185819c84c96ed7 files abcde + foo
  │ │ │
  │ │ │ o  83815831694b1271e9f207cb1b79b2b19275edcb files abcde + foo
  │ │ ├─╯
  │ │ │ o  0977fa602c2fd7d8427ed4e7ee15ea13b84c9173 update files for abcde
  │ │ ├─╯
  │ │ o  3727deee06f72f5ffa8db792ee299cf39e3e190b new change abcde
  │ │ │
  │ │ │ o  5d2fdaa86d070e06669eb141937268780b14861d empty
  │ │ │ │
  │ │ │ │ x  aa5000ec01a38397e1e2ad1eddc643c151459a9b empty
  │ │ │ ├─╯
  │ │ │ o  0c07a3ccda771b25f1cb1edbd02e683723344ef1 new change abcde
  │ │ ├─╯
  │ │ │ o  6c4fd43ed714e7fcd8adbaa7b16c953c2e985b60 added file-abcde
  │ │ ├─╯
  │ │ o  6db330d65db434145c0b59d291853e9a84719b24 added file-abcd
  │ │ │
  │ │ o  abf2df566fc193b3ac34d946e63c1583e4d4732b added file-abc
  │ ├─╯
  │ o  69a232e754b08d568c4899475faf2eb44b857802 added file-ab
  ├─╯
  o  3004d2d9b50883c1538fc754a3aeb55f1b4084f6 added file-a
  
Uncommit with draft parent

  $ hg uncommit
  $ hg phase -r .
  8eb87968f2edb7f27f27fe676316e179de65fff6: draft
  $ hg commit -m 'update ab again'

Uncommit with public parent

  $ hg debugmakepublic "::.^"
  $ hg uncommit
  $ hg phase -r .
  8eb87968f2edb7f27f27fe676316e179de65fff6: public

Partial uncommit with public parent

  $ echo xyz > xyz
  $ hg add xyz
  $ hg commit -m "update ab and add xyz"
  $ hg uncommit xyz
  $ hg status
  A xyz
  $ hg phase -r .
  eba3a9aaec002872b3f74ec1f71cbecc0ad86ac8: draft
  $ hg phase -r ".^"
  8eb87968f2edb7f27f27fe676316e179de65fff6: public

Uncommit leaving an empty changeset

  $ newrepo
  $ hg debugdrawdag <<'EOS'
  > Q
  > |
  > P
  > EOS
  $ hg up Q -q
  $ hg uncommit --keep
  $ hg log -G -T '{desc} FILES: {files}'
  @  Q FILES:
  │
  o  P FILES: P
  
  $ hg status
  A Q

Testing uncommit while merge

  $ newrepo

Create some history

  $ touch a
  $ hg add a
  $ for i in 1 2 3; do echo $i > a; hg commit -m "a $i"; done
  $ hg checkout ea4e33293d4d274a2ba73150733c2612231f398c
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ touch b
  $ hg add b
  $ for i in 1 2 3; do echo $i > b; hg commit -m "b $i"; done
  $ hg log -G -T '{node} {desc}' --hidden
  @  2cd56cdde163ded2fbb16ba2f918c96046ab0bf2 b 3
  │
  o  c3a0d5bb3b15834ffd2ef9ef603e93ec65cf2037 b 2
  │
  o  49bb009ca26078726b8870f1edb29fae8f7618f5 b 1
  │
  │ o  990982b7384266e691f1bc08ca36177adcd1c8a9 a 3
  │ │
  │ o  24d38e3cf160c7b6f5ffe82179332229886a6d34 a 2
  ├─╯
  o  ea4e33293d4d274a2ba73150733c2612231f398c a 1
  

Add and expect uncommit to fail on both merge working dir and merge changeset

  $ hg merge 'max(desc(a))'
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (branch merge, don't forget to commit)

  $ hg uncommit --config experimental.uncommitondirtywdir=False
  abort: outstanding uncommitted merge
  [255]

  $ hg uncommit
  abort: cannot uncommit while merging
  [255]

  $ hg status
  M a
  $ hg commit -m 'merge a and b'

  $ hg uncommit
  abort: cannot uncommit merge changeset
  [255]

  $ hg status
  $ hg log -G -T '{node} {desc}' --hidden
  @    c03b9c37bc67bf504d4912061cfb527b47a63c6e merge a and b
  ├─╮
  │ o  2cd56cdde163ded2fbb16ba2f918c96046ab0bf2 b 3
  │ │
  │ o  c3a0d5bb3b15834ffd2ef9ef603e93ec65cf2037 b 2
  │ │
  │ o  49bb009ca26078726b8870f1edb29fae8f7618f5 b 1
  │ │
  o │  990982b7384266e691f1bc08ca36177adcd1c8a9 a 3
  │ │
  o │  24d38e3cf160c7b6f5ffe82179332229886a6d34 a 2
  ├─╯
  o  ea4e33293d4d274a2ba73150733c2612231f398c a 1
Recover added / deleted files

  $ newrepo
  $ drawdag <<'EOS'
  > B
  > |
  > A
  > EOS
  $ hg up -q $B
  $ hg rm B
  $ touch C
  $ hg add C
  $ hg commit -m C -q
  $ hg uncommit
  $ hg status
  A C
  R B
  $ ls * | sort
  A
  C

Don't mess up with copies when "dest" of copy was added in the commit we are undoing,
and we have a pending removal of the copied file.
  $ newrepo
  $ touch foo
  $ hg commit -Aqm foo
  $ hg cp foo bar
  $ hg commit -Aqm bar
  $ hg rm bar
  $ hg uncommit
  $ hg st
  $ find .
  foo
