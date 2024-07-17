#modern-config-incompatible

#require no-eden

  $ setconfig experimental.allowfilepeer=True

  $ . "$TESTDIR/library.sh"
  $ . "$TESTDIR/infinitepush/library.sh"
  $ setupcommon

  $ enable crdump remotenames
  $ setconfig crdump.commitcloud=true

Setup server
  $ hg init repo
  $ cd repo
  $ setupserver
  $ cd ../

  $ hg clone ssh://user@dummy/repo client -q
  $ cd client
  $ echo a >> a
  $ hg commit -Aqm "added a" --config infinitepushbackup.autobackup=False

commit_cloud should be false when commitcloud is broken
  $ setconfig treemanifest.http=0
  $ hg debugcrdump -r . --config paths.default=xxxxx | grep commit_cloud
              "commit_cloud": false,

debugcrdump should upload the commit and commit_cloud should be true when
commitcloud is working
  $ hg debugcrdump -r . 2>/dev/null | grep commit_cloud
              "commit_cloud": true,

debugcrdump should not attempt to access the network if the commit was
previously backed up (as shown by the lack of error when given a faulty path)
  $ hg debugcrdump -r . --config ui.ssh=true | grep commit_cloud
              "commit_cloud": true,
