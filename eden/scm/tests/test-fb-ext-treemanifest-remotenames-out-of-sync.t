#chg-compatible
  $ setconfig experimental.allowfilepeer=True

  $ . "$TESTDIR/library.sh"

  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > pushrebase=
  > remotenames=
  > treemanifest=
  > [treemanifest]
  > sendtrees=True
  > treeonly=True
  > EOF

# Setup repo

  $ hg init repo --config remotefilelog.reponame=repo --config extensions.treemanifest=$TESTDIR/../sapling/ext/treemanifestserver.py
  $ cd repo
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > treemanifest=$TESTDIR/../sapling/ext/treemanifestserver.py
  > [treemanifest]
  > server=True
  > [remotefilelog]
  > reponame=repo
  > server=True
  > EOF
  $ echo x > x
  $ hg commit -qAm x
  $ hg book master
  $ echo x >> x
  $ hg commit -qAm x2

# Setup shadow repo that will be 'out of date'

  $ cd ..
  $ cp -R repo repo_copy

# Setup two independent clones with their own caches

  $ ROOTDIR=$(pwd)
  $ mkdir cache_client_concurrent
  $ mkdir cache_client
  $ hgcloneshallow ssh://user@dummy/repo client_concurrent -q --config remotefilelog.cachepath=$ROOTDIR/cache_client_concurrent
  fetching tree '' a18d21674e76d6aab2edb46810b20fbdbd10fb4b
  1 trees fetched over * (glob)
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  $ hgcloneshallow ssh://user@dummy/repo client -q  --config remotefilelog.cachepath=$ROOTDIR/cache_client
  fetching tree '' a18d21674e76d6aab2edb46810b20fbdbd10fb4b
  1 trees fetched over * (glob)
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)

# Modify first client repo to create 'out of sync' state

  $ cd client_concurrent
  $ setconfig remotefilelog.cachepath=$ROOTDIR/cache_client_concurrent
  $ setconfig treemanifest.pullprefetchrevs=master treemanifest.sendtrees=True treemanifest.treeonly=True
  $ echo x >> y
  $ hg commit -qAm x3
  $ hg push --to master
  pushing rev 0b41a6a811a2 to destination ssh://user@dummy/repo bookmark master
  searching for changes
  updating bookmark master
  remote: pushing 1 changeset:
  remote:     0b41a6a811a2  x3
  $ hg log -r .
  commit:      0b41a6a811a2
  bookmark:    default/master
  hoistedname: master
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     x3
  
  $ cd ..

# Start making modifications on out of date client, default is set to
# ?read_copy so that fetches will come from an 'out of date' server

  $ cd client
  $ setconfig remotefilelog.cachepath=$ROOTDIR/cache_client
  $ setconfig treemanifest.pullprefetchrevs=master treemanifest.sendtrees=True treemanifest.treeonly=True
  $ setconfig paths.default=ssh://user@dummy/repo?read_copy
  $ setconfig paths.default-push=ssh://user@dummy/repo?write
  $ hg path
  default = ssh://user@dummy/repo?read_copy
  default-push = ssh://user@dummy/repo?write
  $ hg log -r .
  commit:      a89d614e2364
  bookmark:    default/master
  hoistedname: master
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     x2
  
  $ echo x >> x
  $ hg commit -qAm x4
  $ hg log -r .
  commit:      e68715a0fc4c
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     x4
  
# Verify that a push succeeds because the read will go to the write server
# instead of the out-of-date read server

  $ hg push --to master
  pushing rev e68715a0fc4c to destination ssh://user@dummy/repo?write bookmark master
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master
  remote: pushing 1 changeset:
  remote:     e68715a0fc4c  x4
  remote: 2 new changesets from the server will be downloaded
  fetching tree '' eda1f7bdb1c764a4e03857a25db3d6cad9d25088
  1 trees fetched over * (glob)
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg log -r .
  commit:      12f14bedbd28
  bookmark:    default/master
  hoistedname: master
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     x4
  
