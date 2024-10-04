#require no-eden

  $ configure modern
  $ setconfig diff.git=1

  $ reinit () {
  >   newrepo
  > }

  $ newrepo

Test what said in drawdag.py docstring

  $ hg debugdrawdag <<'EOS'
  > c d
  > |/
  > b
  > |
  > a
  > EOS

  $ hg log -G -T '{desc} ({bookmarks})'
  o  d (d)
  │
  │ o  c (c)
  ├─╯
  o  b (b)
  │
  o  a (a)
  
  $ hg debugdrawdag <<'EOS'
  >  foo    bar       bar  foo
  >   |     /          |    |
  >  ancestor(c,d)     a   baz
  > EOS

  $ hg log -G -T '{desc}'
  o    foo
  ├─╮
  │ │ o  bar
  ╭───┤
  │ o │  baz
  │   │
  │ o │  d
  ├─╯ │
  │ o │  c
  ├─╯ │
  o   │  b
  ├───╯
  o  a
  
  $ reinit

  $ hg debugdrawdag <<'EOS'
  > foo
  > |\
  > | | d
  > | |/
  > | | c
  > | |/
  > | | bar
  > | |/|
  > | b |
  > | |/
  > | a
  > |
  > baz
  > EOS

  $ hg log -G -T '{desc}'
  o    foo
  ├─╮
  │ │ o  d
  │ ├─╯
  │ │ o  c
  │ ├─╯
  │ │ o  bar
  │ ╭─┤
  │ o │  b
  │ ├─╯
  o │  baz
    │
    o  a
  
  $ hg manifest -r a
  a
  $ hg manifest -r b
  a
  b
  $ hg manifest -r bar
  a
  b
  $ hg manifest -r foo
  a
  b
  baz

Edges existed in repo are no-ops

  $ reinit
  $ hg debugdrawdag <<'EOS'
  > B C C
  > | | |
  > A A B
  > EOS

  $ hg log -G -T '{desc}'
  o    C
  ├─╮
  │ o  B
  ├─╯
  o  A
  

  $ hg debugdrawdag --traceback <<'EOS'
  > C D C
  > | | |
  > B B A
  > EOS

  $ hg log -G -T '{desc}'
  o  D
  │
  │ o  C
  ╭─┤
  o │  B
  ├─╯
  o  A
  

Node with more than 2 parents are disallowed

  $ hg debugdrawdag <<'EOS'
  >   A
  >  /|\
  > D B C
  > EOS
  abort: A: too many parents: B C D
  [255]

Cycles are disallowed

  $ hg debugdrawdag <<'EOS'
  > A
  > |
  > A
  > EOS
  abort: the graph has cycles
  [255]

  $ hg debugdrawdag <<'EOS'
  > A
  > |
  > B
  > |
  > A
  > EOS
  abort: the graph has cycles
  [255]

Create mutations or hide commits via comments

  $ reinit

  $ hg debugdrawdag <<'EOS'
  >       G L
  >       | |
  > I D C F K    # split: B -> E, F, G
  >  \ \| | |    # replace: C -> D -> H
  >   H B E J M  # prune: F, I
  >    \|/  |/   # fold: J, K, L -> M
  >     A   A    # revive: D, K
  > EOS

  $ hg log -r 'sort(all(), topo)' -G --hidden -T '{desc} {node}'
  o  G 711f53bbef0bebd12eb6f0511d5e2e998b984846
  │
  o  F 64a8289d249234b9886244d379f15e6b650b28e3
  │
  o  E 7fb047a69f220c21711122dfd94305a9efb60cba
  │
  │ x  L 12ac214c2132ccaa5b97fa70b25570496f86853c
  │ │
  │ x  K 623037570ba0971f93c31b1b90fa8a1b82307329
  │ │
  │ x  J a0a5005cec670cc22e984711855473e8ba07230a
  ├─╯
  │ x  D be0ef73c17ade3fc89dc41701eb9fc3a91b58282
  │ │
  │ │ x  C 26805aba1e600a82e93661149f2313866a221a7b
  │ ├─╯
  │ x  B 112478962961147124edd43549aedd1a335e44bf
  ├─╯
  │ o  I 58e6b987bf7045fcd9c54f496396ca1d1fc81047
  │ │
  │ o  H 575c4b5ec114d64b681d33f8792853568bfb2b2c
  ├─╯
  │ o  M 699bc4b6fa2207ae482508d19836281c02008d1e
  ├─╯
  o  A 426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  $ hg debugmutation -r 'all()'
   *  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  
   *  112478962961147124edd43549aedd1a335e44bf
  
   *  7fb047a69f220c21711122dfd94305a9efb60cba
  
   *  be0ef73c17ade3fc89dc41701eb9fc3a91b58282 replace by test at 1970-01-01T00:00:00 from:
      26805aba1e600a82e93661149f2313866a221a7b
  
   *  64a8289d249234b9886244d379f15e6b650b28e3
  
   *  711f53bbef0bebd12eb6f0511d5e2e998b984846 split by test at 1970-01-01T00:00:00 (split into this and: 7fb047a69f220c21711122dfd94305a9efb60cba, 64a8289d249234b9886244d379f15e6b650b28e3) from:
      112478962961147124edd43549aedd1a335e44bf
  
   *  575c4b5ec114d64b681d33f8792853568bfb2b2c replace by test at 1970-01-01T00:00:00 from:
      be0ef73c17ade3fc89dc41701eb9fc3a91b58282 replace by test at 1970-01-01T00:00:00 from:
      26805aba1e600a82e93661149f2313866a221a7b
  
   *  699bc4b6fa2207ae482508d19836281c02008d1e fold by test at 1970-01-01T00:00:00 from:
      |-  a0a5005cec670cc22e984711855473e8ba07230a
      |-  623037570ba0971f93c31b1b90fa8a1b82307329
      '-  12ac214c2132ccaa5b97fa70b25570496f86853c
  
   *  58e6b987bf7045fcd9c54f496396ca1d1fc81047
  
Change file contents via comments

  $ reinit
  $ hg debugdrawdag <<'EOS'
  > C       # A/dir1/a = 1\n2
  > |\      # B/dir2/b = 34
  > A B     # C/dir1/c = 5
  >         # C/dir2/c = 6
  >         # C/A = a
  >         # C/B = b
  > EOS

  $ hg log -G -T '{desc} {files}'
  o    C A B dir1/c dir2/c
  ├─╮
  │ o  B B dir2/b
  │
  o  A A dir1/a
  
  $ for f in `hg files -r C`; do
  >   echo FILE "$f"
  >   hg cat -r C "$f"
  >   echo
  > done
  FILE A
  a
  FILE B
  b
  FILE dir1/a
  1
  2
  FILE dir1/c
  5
  FILE dir2/b
  34
  FILE dir2/c
  6

Special comments: "(removed)", "(copied from X)", "(renamed from X)"

  $ newrepo
  $ drawdag <<'EOS'
  > C   # C/X1 = (removed)
  > |   # C/C = (removed)
  > |
  > B   # B/B = (removed)
  > |   # B/X1 = X\n1\n (renamed from X)
  > |   # B/Y1 = Y\n1\n (copied from Y)
  > |
  > |   # A/A = (removed)
  > A   # A/X = X\n
  >     # A/Y = Y\n
  > EOS

  $ hg log -p -G -r 'all()' -T '{desc}\n'
  o  C
  │  diff --git a/X1 b/X1
  │  deleted file mode 100644
  │  --- a/X1
  │  +++ /dev/null
  │  @@ -1,2 +0,0 @@
  │  -X
  │  -1
  │
  o  B
  │  diff --git a/X b/X1
  │  rename from X
  │  rename to X1
  │  --- a/X
  │  +++ b/X1
  │  @@ -1,1 +1,2 @@
  │   X
  │  +1
  │  diff --git a/Y b/Y1
  │  copy from Y
  │  copy to Y1
  │  --- a/Y
  │  +++ b/Y1
  │  @@ -1,1 +1,2 @@
  │   Y
  │  +1
  │
  o  A
     diff --git a/X b/X
     new file mode 100644
     --- /dev/null
     +++ b/X
     @@ -0,0 +1,1 @@
     +X
     diff --git a/Y b/Y
     new file mode 100644
     --- /dev/null
     +++ b/Y
     @@ -0,0 +1,1 @@
     +Y

Special comments: "X has date 1 0"

  $ newrepo
  $ drawdag <<'EOS'
  > B  # B has date 2 0
  > |
  > A
  > EOS
  $ hg log -r 'all()' -T '{desc} {date}\n'
  A 0.00
  B 2.00

--no-bookmarks and --print:

  $ newrepo
  $ echo 'A' | hg debugdrawdag --no-bookmarks --print
  426bada5c675 A
  $ hg up 'desc(A)' -q
  $ hg debugdrawdag --no-bookmarks --print << 'EOS'
  > B
  > |
  > .
  > EOS
  426bada5c675 .
  112478962961 B
  $ hg bookmarks
  no bookmarks set

Bookmarks:

  $ newrepo
  $ hg debugdrawdag --no-bookmarks << 'EOS'
  > B   # bookmark BOOK_B = B
  > |   # bookmark BOOK-A = A
  > A
  > EOS
  $ hg bookmarks
     BOOK-A                    426bada5c675
     BOOK_B                    112478962961

Horizontal graph with ranges:

  $ newrepo
  $ hg debugdrawdag << 'EOS'
  > A--S--D..G--Z
  > EOS
  $ hg log -Gr: -T '{desc}'
  o  Z
  │
  o  G
  │
  o  F
  │
  o  E
  │
  o  D
  │
  o  S
  │
  o  A
  
Customized script for complex commit properties, intended for creating examples
for documentation:

  $ newrepo
  $ hg debugdrawdag << 'EOS'
  > P1..P9
  > P3-A1-A2
  > P5-B1..B3
  > P7-C1-C2-C3
  >        \
  >         C4
  > python:
  > now('2000-7-28 12:00 UTC')  # set the "now" time
  > commit(user='mary')         # set default author
  > commit('A1', '[sl] windows: update Python', date='2000-7-15 14:25', files={"0": b85(b"f\n")})
  > commit('A2', 'debug', date='2000-7-15 14:35', files={"1": b85(b"f\n"), "2": "f\n", "3": b85(b"f\n") + ' (copied from 0)'})
  > commit('B1', '[eden] Thread EdenConfig down to Windows fsck', date='3d ago')
  > commit('B2', '[eden] Remove n^2 path comparisons from Windows fsck', date='3d ago')
  > commit('B3', '[edenfs] Recover Overlay from disk/scm for Windows fsck', date='3d ago')
  > commit('C1', '[eden] Use PathMap for WindowsFsck', date='2d ago')
  > commit('C2', '[eden] Close Windows file handle during Windows Fsck', date='2d ago')
  > commit('C3', 'temp', date='2d ago')
  > commit('C4', '[eden] Support long paths in Windows FSCK', date='12m ago')
  > commit('P9', remotename='remote/master')
  > commit('P7', remotename='remote/stable')
  > commit('P6', pred='A1', op='land')
  > goto('B3')
  > EOS

  $ enable smartlog
  $ cat >> "$HGRCPATH" <<EOF
  > [templatealias]
  > mutation_descs = "{join(mutations % '({operation} to {join(successors % \'{node|short}\', \', \')})', ' ')}"
  > [devel]
  > default-date=2000-7-28 12:00 UTC
  > EOF
  $ hg sl -T '{node|shortest} {ifeq(phase,"public","{remotenames}","{mutation_descs} {author} {date|simpledate}\n{desc|firstline}\n ")}'
  o  1313 remote/master
  ╷
  ╷ o  03d3  mary Today at 11:48
  ╷ │  [eden] Support long paths in Windows FSCK
  ╷ │
  ╷ │ o  839a  mary Wednesday at 12:00
  ╷ ├─╯  temp
  ╷ │
  ╷ o  8c37  mary Wednesday at 12:00
  ╷ │  [eden] Close Windows file handle during Windows Fsck
  ╷ │
  ╷ o  e105  mary Wednesday at 12:00
  ╭─╯  [eden] Use PathMap for WindowsFsck
  │
  o  e406 remote/stable
  ╷
  ╷ @  67e2  mary Tuesday at 12:00
  ╷ │  [edenfs] Recover Overlay from disk/scm for Windows fsck
  ╷ │
  ╷ o  ee32  mary Tuesday at 12:00
  ╷ │  [eden] Remove n^2 path comparisons from Windows fsck
  ╷ │
  ╷ o  4284  mary Tuesday at 12:00
  ╭─╯  [eden] Thread EdenConfig down to Windows fsck
  │
  o  eb5e
  ╷
  ╷ o  d097  mary Jul 15 at 14:35
  ╷ │  debug
  ╷ │
  ╷ x  8ada (land to 29ac1765dd19) mary Jul 15 at 14:25
  ╭─╯  [sl] windows: update Python
  │
  o  19cd
  │
  ~

  $ hg log -T '{desc}\n' -p -r 'desc(debug)'
  debug
  diff --git a/1 b/1
  new file mode 100644
  --- /dev/null
  +++ b/1
  @@ -0,0 +1,1 @@
  +f
  diff --git a/2 b/2
  new file mode 100644
  --- /dev/null
  +++ b/2
  @@ -0,0 +1,1 @@
  +f
  diff --git a/0 b/3
  copy from 0
  copy to 3


Give errors for invalid commit names for files:
  $ newrepo
  $ drawdag <<EOS
  > A  # B/oops = oops
  > EOS
  abort: unused files: ['B/oops']
  [255]


Support file names with dashes:
  $ newrepo
  $ drawdag <<EOS
  > A  # A/hi-there = foo
  > EOS
  $ hg st --change $A
  A A
  A hi-there

Support binary, base85 data and works for merge:
  $ newrepo
  $ drawdag << 'EOS'
  > C    # B/B has 20 bytes.
  > |\   # B/B=base85:e#pZ!R^O0GSDo$oDHiT8&d*Cs
  > A B  # A/A=random:30
  > EOS
  $ hg up -q $C
  $ wc -c A
  30
  $ wc -c B
  20


Can combine comment files with commit():
  $ newrepo
  $ drawdag << 'EOS'
  > A  # A/comment = foo\n
  > python:
  > commit("A", "message", files={"python": "bar\n"})
  > EOS
  $ hg show $A
  commit:      b501648cc37d
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  files:       comment python
  description:
  message
  
  
  diff --git a/comment b/comment
  new file mode 100644
  --- /dev/null
  +++ b/comment
  @@ -0,0 +1,1 @@
  +foo
  diff --git a/python b/python
  new file mode 100644
  --- /dev/null
  +++ b/python
  @@ -0,0 +1,1 @@
  +bar
