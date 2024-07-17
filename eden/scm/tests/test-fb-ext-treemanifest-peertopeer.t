#chg-compatible
#debugruntest-incompatible
  $ setconfig experimental.allowfilepeer=True

  $ CACHEDIR=`pwd`/hgcache

  $ . "$TESTDIR/library.sh"

Create server
  $ hginit master
  $ cd master
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > treemanifest=$TESTDIR/../sapling/ext/treemanifestserver.py
  > [treemanifest]
  > server=True
  > [remotefilelog]
  > server=True
  > EOF

  $ mkdir subdir
  $ echo x > subdir/x
  $ hg commit -qAm 'add subdir/x'
  $ cd ..

Create client2 - it will have only the first commit, so client1 will be pushing
two server and one local commits later.
  $ hgcloneshallow ssh://user@dummy/master client2 -q
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  $ cat >> client2/.hg/hgrc <<EOF
  > [remotefilelog]
  > reponame=master
  > cachepath=$CACHEDIR
  > EOF

Create create two more server commits
  $ cd master
  $ echo x >> subdir/x
  $ hg commit -m 'modify subdir/x'
  $ echo x >> subdir/x
  $ hg commit -m 'modify subdir/x again'
  $ cd ..

Create client1 - it will have both server commits
  $ hgcloneshallow ssh://user@dummy/master client1 -q
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  $ cd client1
  $ cat >> .hg/hgrc <<EOF
  > [remotefilelog]
  > reponame=master
  > cachepath=$CACHEDIR
  > EOF

  $ echo a > a
  $ mkdir dir
  $ echo b > dir/b
  $ hg commit -Aqm 'initial commit'

  $ hg debugdumpindexedlog .hg/store/manifests/indexedlogdatastore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2
  $ hg debugdumpindexedlog .hg/store/manifests/indexedloghistorystore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2
  $ clearcache

Pushing p2p with sendtrees=True puts the received packs in the local pack store
# Prefetch client2 so we dont see any downloads related to what the target
# already has.
  $ hg -R ../client2 prefetch -r 'all()'
  1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  $ cp ../client2/.hg/hgrc ../client2/.hg/hgrc.bak
  $ cat >> ../client2/.hg/hgrc <<EOF
  > [remotefilelog]
  > cachepath=$CACHEDIR/cache2
  > EOF

# Push and expect downloads of both public trees (in arbitrary order)
  $ hg push -q ssh://user@dummy/client2
  remote: 1 files fetched over 1 fetches - (1 misses, 0.00% hit ratio) over * (glob) (?)
  2 files fetched over 1 fetches - (2 misses, 0.00% hit ratio) over * (glob) (?)
  $ hg log
  commit:      dba4eeec7e63
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     initial commit
  
  commit:      c24ce1f3eb31
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     modify subdir/x again
  
  commit:      098a163f13ea
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     modify subdir/x
  
  commit:      d618f764f9a1
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     add subdir/x
  
  $ mv ../client2/.hg/hgrc.bak ../client2/.hg/hgrc
  $ hg debugdumpindexedlog ../client2/.hg/store/manifests/indexedlogdatastore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2
  $ hg debugdumpindexedlog ../client2/.hg/store/manifests/indexedloghistorystore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2
Pulling between peers should send local trees but not remote trees
# Strip back one server commit and one draft commit, so we can pull them again
  $ cd ../client2
  $ hg debugstrip -r 'max(desc(modify))' --no-backup
# Delete the old local tree data from the draft commit, so we can verify it is
# downloaded again during pull.
  $ rm -rf .hg/store/manifests
  $ hg debugdumpindexedlog .hg/store/manifests/indexedlogdatastore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  0
  $ hg debugdumpindexedlog .hg/store/manifests/indexedloghistorystore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  0
# Change this client to use a different cache from the other client, since the
# other client may populate data that we need to test if this client is
# downloading.
  $ cp .hg/hgrc .hg/hgrc.bak
  $ mkdir $TESTTMP/hgcache2
  $ cat >> .hg/hgrc <<EOF
  > [remotefilelog]
  > cachepath=$TESTTMP/hgcache2
  > EOF
# Force the draft commit to be public so we can ensure it's trees are delivered
# despite it being public.
  $ rm -rf $TESTTMP/hgcache2/*
  $ hg pull -q --config treemanifest.sendtrees=True ../client1 --config remotefilelog.fallbackpath=ssh://user@dummy/master
# Check that the local commits for the draft commit were downloaded to the local store.
  $ hg debugdumpindexedlog .hg/store/manifests/indexedlogdatastore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2
  $ hg debugdumpindexedlog .hg/store/manifests/indexedloghistorystore 2>/dev/stdout | grep Entry | wc -l | sed -e 's/ //g'
  2

# Verify the real-public commit wasn't received during the pull and therefore
# has to be downloaded on demand.
  $ rm -rf $TESTTMP/hgcache2/*
  $ ls_l $TESTTMP/hgcache2/
  $ hg manifest -r 'tip^'
  subdir/x
# Verify the fake-public commit was received during the pull and does not
# require additional ondemand downloads.
  $ hg manifest -r tip
  a
  dir/b
  subdir/x
  $ mv .hg/hgrc.bak .hg/hgrc
