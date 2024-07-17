#debugruntest-incompatible
(debugruntest fails under buck for some reason)
#chg-compatible

  $ configure mutation-norecord
  $ enable amend rebase shelve

Test amend copytrace
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ hg add a
  $ hg ci -m "create a"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ hg up -q ".^"
  $ hg mv a b
  $ hg amend
  hint[amend-restack]: descendants of 9f815da0cfb3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg rebase --restack
  rebasing ad25e018afa9 "mod a"
  merging b and a to b
  $ ls
  b
  x
  $ cat b
  a
  $ hg goto 'max(desc(mod))'
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ cat b
  b
  $ cd ..
  $ rm -rf repo

Test amend copytrace with multiple stacked commits
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ echo b > b
  $ echo c > c
  $ hg add a b c
  $ hg ci -m "create a b c"
  $ echo a1 > a
  $ hg ci -qm "mod a"
  $ echo b2 > b
  $ hg ci -qm "mod b"
  $ echo c3 > c
  $ hg ci -qm "mod c"
  $ hg bookmark test-top
  $ hg up -q '.~3'
  $ hg mv a a1
  $ hg mv b b2
  $ hg amend
  hint[amend-restack]: descendants of ec8c441da632 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg mv c c3
  $ hg amend
  $ hg rebase --restack
  rebasing 797127d4e250 "mod a"
  merging a1 and a to a1
  rebasing e2aabbfe749a "mod b"
  merging b2 and b to b2
  rebasing 4f8d18558559 "mod c" (test-top)
  merging c3 and c to c3
  $ hg up test-top
  3 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (activating bookmark test-top)
  $ cat a1 b2 c3
  a1
  b2
  c3
  $ cd ..
  $ rm -rf repo

Test amend copytrace with multiple renames of the same file
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ hg add a
  $ hg ci -m "create a"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ hg up -q ".^"
  $ hg mv a b
  $ hg amend
  hint[amend-restack]: descendants of 9f815da0cfb3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg mv b c
  $ hg amend
  $ hg rebase --restack
  rebasing ad25e018afa9 "mod a"
  merging c and a to c
  $ hg goto 'max(desc(mod))'
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ cat c
  b
  $ cd ..
  $ rm -rf repo

Test amend copytrace with copies
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ echo i > i
  $ hg add a i
  $ hg ci -m "create a i"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ echo j > i
  $ hg ci -qm "mod i"
  $ hg bookmark test-top
  $ hg up -q ".~2"
  $ hg cp a b
  $ hg amend
  hint[amend-restack]: descendants of 0157114ee1b3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg cp i j
  $ hg amend
  $ hg cp b c
  $ hg amend
  $ hg rebase --restack
  rebasing 6938f0d82b23 "mod a"
  merging b and a to b
  merging c and a to c
  rebasing df8dfcb1d237 "mod i" (test-top)
  merging j and i to j
  $ hg up test-top
  5 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (activating bookmark test-top)
  $ cat a b c i j
  b
  b
  b
  j
  j
  $ cd ..
  $ rm -rf repo

Test rebase after amend deletion of copy
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ hg add a
  $ hg ci -m "create a"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ hg up -q ".^"
  $ hg cp a b
  $ hg amend
  hint[amend-restack]: descendants of 9f815da0cfb3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg rm b
  $ hg amend
  $ hg rebase --restack
  rebasing ad25e018afa9 "mod a"
  $ cd ..
  $ rm -rf repo

Test failure to rebase deletion after rename
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ hg add a
  $ hg ci -m "create a"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ hg rm a
  $ hg ci -m "delete a"
  $ hg up -q ".~2"
  $ hg mv a b
  $ hg amend
  hint[amend-restack]: descendants of 9f815da0cfb3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg rebase --restack
  rebasing ad25e018afa9 "mod a"
  merging b and a to b
  rebasing ba0395f0e180 "delete a"
  local [dest] changed b which other [source] deleted (as a)
  use (c)hanged version, (d)elete, or leave (u)nresolved? u
  unresolved conflicts (see hg resolve, then hg rebase --continue)
  [1]
  $ hg rebase --abort
  rebase aborted
  $ cd ..
  $ rm -rf repo

Test amend copytrace can be disabled
  $ cat >> $HGRCPATH << EOF
  > [copytrace]
  > enableamendcopytrace=false
  > EOF
  $ hg init repo
  $ cd repo
  $ echo x > x
  $ hg add x
  $ hg ci -m initial
  $ echo a > a
  $ hg add a
  $ hg ci -m "create a"
  $ echo b > a
  $ hg ci -qm "mod a"
  $ hg up -q ".^"
  $ hg mv a b
  $ hg amend
  hint[amend-restack]: descendants of 9f815da0cfb3 are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg rebase --restack
  rebasing ad25e018afa9 "mod a"
  other [source] changed a which local [dest] is missing
  hint: the missing file was probably added by commit 9f815da0cfb3 in the branch being rebased
  use (c)hanged version, leave (d)eleted, or leave (u)nresolved, or input (r)enamed path? u
  unresolved conflicts (see hg resolve, then hg rebase --continue)
  [1]
  $ cd ..
  $ rm -rf repo
