#chg-compatible
#debugruntest-incompatible
  $ setconfig experimental.allowfilepeer=True

  $ . "$TESTDIR/library.sh"

  $ hginit master
  $ cd master
  $ cat >> .hg/hgrc <<EOF
  > [remotefilelog]
  > server=True
  > EOF
  $ echo x > x
  $ hg commit -qAm x
  $ echo y >> x
  $ hg commit -qAm y
  $ echo z >> x
  $ hg commit -qAm z

  $ cd ..

  $ hgcloneshallow ssh://user@dummy/master shallow -q
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over *s (glob) (?)
  $ cd shallow

Unbundling a shallow bundle

  $ hg debugstrip -r 66ee28d0328c
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over *s (glob) (?)

Unbundling a full bundle

  $ hg -R ../master bundle -r 66ee28d0328c:: --base "66ee28d0328c^" ../fullbundle.hg
  2 changesets found
  $ hg unbundle ../fullbundle.hg
  adding changesets
  adding manifests
  adding file changes

Pulling from a shallow bundle

  $ hg debugstrip -r 66ee28d0328c
  $ hg pull -r 66ee28d0328c .hg/strip-backup/66ee28d0328c-3d7aafd1-backup.hg
  pulling from .hg/strip-backup/66ee28d0328c-3d7aafd1-backup.hg
  searching for changes
  adding changesets
  adding manifests
  adding file changes

Pulling from a full bundle

  $ hg debugstrip -r 66ee28d0328c
  $ hg pull -r 66ee28d0328c ../fullbundle.hg
  pulling from ../fullbundle.hg
  searching for changes
  abort: cannot pull from full bundles
  (use `hg unbundle` instead)
  [255]
