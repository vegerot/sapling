#modern-config-incompatible

#require no-eden

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
  $ echo a > a
  $ hg commit -qAm a

  $ cd ..

  $ hgcloneshallow ssh://user@dummy/master shallow -q
  2 files fetched over 1 fetches - (2 misses, 0.00% hit ratio) over *s (glob) (?)
  $ cd shallow

Test blame

  $ hg blame -c x
  b292c1e3311f: x
  66ee28d0328c: y
  16db62c5946f: z
  2 files fetched over 1 fetches - (2 misses, 0.00% hit ratio) over 0.00s (?)
