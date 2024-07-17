#modern-config-incompatible

#require execbit no-eden
  $ setconfig experimental.nativecheckout=true
  $ setconfig commands.update.check=none

  $ newserver server

  $ rm -rf a
  $ newremoterepo a

  $ echo foo > foo
  $ hg ci -qAm0
  $ echo toremove > toremove
  $ echo todelete > todelete
  $ chmod +x foo toremove todelete
  $ hg ci -qAm1

Test that local removed/deleted, remote removed works with flags
  $ hg rm toremove
  $ rm todelete
  $ hg co -q 'desc(0)'

  $ echo dirty > foo
  $ hg up -c 'desc(1)'
  abort: uncommitted changes
  [255]
  $ hg up -q 'desc(1)'
  $ cat foo
  dirty
  $ hg st -A
  M foo
  C todelete
  C toremove

Validate update of standalone execute bit change:

  $ hg up -C 'desc(0)'
  1 files updated, 0 files merged, 2 files removed, 0 files unresolved
  $ chmod -x foo
  $ hg ci -m removeexec
  nothing changed
  [1]
  $ hg up -C 'desc(0)'
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg up 'desc(1)'
  3 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg st

  $ cd ..
