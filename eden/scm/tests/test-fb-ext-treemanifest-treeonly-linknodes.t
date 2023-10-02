#chg-compatible
  $ setconfig experimental.allowfilepeer=True

  $ . "$TESTDIR/library.sh"

Set up the server

  $ hginit master
  $ cd master
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase=
  > remotenames=
  > treemanifest=$TESTDIR/../sapling/ext/treemanifestserver.py
  > [treemanifest]
  > server=true
  > treeonly=true
  > [remotefilelog]
  > server=true
  > shallowtrees=true
  > EOF

  $ echo 1 > x
  $ hg commit -Aqm x1

Create client
  $ cd ..
  $ hgcloneshallow ssh://user@dummy/master client -q --config extensions.treemanifest= --config treemanifest.treeonly=true
  fetching tree '' 2e4a95dcb6b42bbf0034f84d293bd9c71b19de64
  1 trees fetched over * (glob)
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  $ cd client
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > amend=
  > pushrebase=
  > remotenames=
  > treemanifest=
  > [treemanifest]
  > treeonly=true
  > sendtrees=true
  > useruststore=true
  > EOF

Create a commit, and then amend the message twice.  All three should share a manifest.
  $ echo 2 > x
  $ hg commit -Aqm x2
  $ hg amend -m x2a
  $ hg amend -m x2b
  $ hg log -G -r 'all()' --hidden -T '{node} {manifest} {desc}'
  @  426667c0eafcfb0836d7a5a55f66b2b8f20c9842 4921ba8b088dda769331d6cf5c70f349b7c5c6c8 x2b
  │
  │ x  e0ce6fd597a73d4b7d1fda2cbe6337636f94d3dd 4921ba8b088dda769331d6cf5c70f349b7c5c6c8 x2a
  ├─╯
  │ x  5ee5c65bfee26d54c1fb59cf411fd5a81a328b83 4921ba8b088dda769331d6cf5c70f349b7c5c6c8 x2
  ├─╯
  o  203f57bcaf7c8ad8dd3bb2ba85343f072905c086 2e4a95dcb6b42bbf0034f84d293bd9c71b19de64 x1
  

Push commit 1 to the server
  $ hg push -r 5ee5c65bfee26d54c1fb59cf411fd5a81a328b83 --allow-anon
  pushing to ssh://user@dummy/master
  searching for changes
  abort: push includes obsolete changeset: 5ee5c65bfee2!
  [255]

Works ok with pushrebase.
  $ hg unhide 'desc(x2a)'
  $ hg push -r 'desc(x2a)' --to test --create
  pushing rev e0ce6fd597a7 to destination ssh://user@dummy/master bookmark test
  searching for changes
  exporting bookmark test
  remote: pushing 1 changeset:
  remote:     e0ce6fd597a7  x2a
