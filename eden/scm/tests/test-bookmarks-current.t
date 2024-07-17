
#require no-eden



  $ hg init repo
  $ cd repo

no bookmarks

  $ hg bookmarks
  no bookmarks set

set bookmark X

  $ hg bookmark X

list bookmarks

  $ hg bookmark
   * X                         000000000000

list bookmarks with color

  $ hg --config extensions.color= --config color.mode=ansi \
  >     bookmark --color=always
  \x1b[32m * \x1b[39m\x1b[32mX\x1b[39m\x1b[32m                         000000000000\x1b[39m (esc)

update to bookmark X

  $ hg bookmarks
   * X                         000000000000
  $ hg goto X
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved

list bookmarks

  $ hg bookmarks
   * X                         000000000000

rename

  $ hg bookmark -m X Z

list bookmarks

  $ hg bookmarks
   * Z                         000000000000

new bookmarks X and Y, first one made active

  $ hg bookmark Y X

list bookmarks

  $ hg bookmark
     X                         000000000000
   * Y                         000000000000
     Z                         000000000000

  $ hg bookmark -d X

commit

  $ echo 'b' > b
  $ hg add b
  $ hg commit -m'test'

list bookmarks

  $ hg bookmark
   * Y                         719295282060
     Z                         000000000000

Verify that switching to Z updates the active bookmark:
  $ hg goto Z
  0 files updated, 0 files merged, 1 files removed, 0 files unresolved
  (changing active bookmark from Y to Z)
  $ hg bookmark
     Y                         719295282060
   * Z                         000000000000

Switch back to Y for the remaining tests in this file:
  $ hg goto Y
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (changing active bookmark from Z to Y)

delete bookmarks

  $ hg bookmark -d Y
  $ hg bookmark -d Z

list bookmarks

  $ hg bookmark
  no bookmarks set

update to tip

  $ hg goto tip
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved

set bookmark Y using -r . but make sure that the active
bookmark is not activated

  $ hg bookmark -r . Y

list bookmarks, Y should not be active

  $ hg bookmark
     Y                         719295282060

now, activate Y

  $ hg up -q Y

set bookmark Z using -i

  $ hg bookmark -r . -i Z
  $ hg bookmarks
   * Y                         719295282060
     Z                         719295282060

deactivate active bookmark using -i

  $ hg bookmark -i Y
  $ hg bookmarks
     Y                         719295282060
     Z                         719295282060

  $ hg up -q Y
  $ hg bookmark -i
  $ hg bookmarks
     Y                         719295282060
     Z                         719295282060
  $ hg bookmark -i
  no active bookmark
  $ hg up -q Y
  $ hg bookmarks
   * Y                         719295282060
     Z                         719295282060

deactivate active bookmark while renaming

  $ hg bookmark -i -m Y X
  $ hg bookmarks
     X                         719295282060
     Z                         719295282060

  $ echo a > a
  $ hg ci -Am1
  adding a
  $ echo b >> a
  $ hg ci -Am2
  $ hg goto -q X

test deleting .hg/bookmarks.current when explicitly updating
to a revision

  $ echo a >> b
  $ hg ci -m.
  $ hg up -q X
  $ test -f .hg/bookmarks.current

try to update to it again to make sure we don't
set and then unset it

  $ hg up -q X
  $ test -f .hg/bookmarks.current

  $ hg up -q 'desc(1)'
  $ test -f .hg/bookmarks.current
  [1]

when a bookmark is active, hg up -r . is
analogous to hg book -i <active bookmark>

  $ hg up -q X
  $ hg up -q .
  $ test -f .hg/bookmarks.current
  [1]

issue 4552 -- simulate a pull moving the active bookmark

  $ hg up -q X
  $ printf "Z" > .hg/bookmarks.current
  $ hg log -T '{activebookmark}\n' -r Z
  Z
  $ hg log -T '{bookmarks % "{active}\n"}' -r Z
  Z

