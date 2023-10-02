# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ CACHEDIR=$PWD/cachepath
  $ . "${TEST_FIXTURES}/library.sh"

Setup repo config (we use blob_files to share across Mononoke and API Server):
  $ LFS_THRESHOLD="1000" setup_common_config "blob_files"
  $ cd $TESTTMP

Setup hg repo, create a commit there. No LFS blobs yet.
  $ hginit_treemanifest repo-hg-nolfs
  $ cd repo-hg-nolfs
  $ setup_hg_server

Commit small file
  $ echo s > smallfile
  $ hg commit -Aqm "add small file"
  $ hg bookmark master_bookmark -r tip
  $ cd ..

Blobimport the hg repo to Mononoke
  $ blobimport repo-hg-nolfs/.hg repo

Start Mononoke with LFS enabled.
  $ start_and_wait_for_mononoke_server
Start Mononoke API server, to serve LFS blobs
  $ lfs_uri="$(lfs_server)/repo"

Create a new client repository. Enable LFS there.
  $ hgclone_treemanifest ssh://user@dummy/repo-hg-nolfs repo-hg-lfs --noupdate --config extensions.remotenames=
  $ cd repo-hg-lfs
  $ setup_hg_client
  $ setup_hg_lfs "$lfs_uri" 1000B "$TESTTMP/lfs-cache1"

  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > remotenames =
  > EOF

Update in the client repo
  $ hgmn pull -q
  $ hgmn update -r master_bookmark -q

Perform LFS push
  $ LONG="$(yes A 2>/dev/null | head -c 2000)"
  $ echo "$LONG" > lfs-largefile
  $ echo "${LONG}for-rename" > lfs-largefile-for-rename
  $ hg commit -Aqm "add lfs-large files"
  $ hgmn push -r . --to master_bookmark -v
  pushing rev 99765c8d839c to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  lfs: need to transfer 2 objects (3.92 KB)
  lfs: uploading e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d (1.95 KB)
  lfs: processed: e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d
  lfs: uploading d19bca751e178f8cce59e1b872e0fd5857951c2577a2318aefad3253c317d982 (1.96 KB)
  lfs: processed: d19bca751e178f8cce59e1b872e0fd5857951c2577a2318aefad3253c317d982
  validated revset for rebase
  1 changesets found
  uncompressed size of bundle content:
       231 (changelog)
       282  lfs-largefile
       293  lfs-largefile-for-rename
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark

# Rename a file
  $ hg mv lfs-largefile-for-rename lfs-largefile-renamed
  $ hg commit -Aqm "rename"
  $ hgmn push -r . --to master_bookmark -v
  pushing rev c651f052c52d to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  validated revset for rebase
  1 changesets found
  uncompressed size of bundle content:
       226 (changelog)
       379  lfs-largefile-renamed
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark

Verify that if we fail to upload LFS blobs first, the push fails
  $ cat >> .hg/hgrc << EOF
  > [extensions]
  > lfs=
  > [lfs]
  > url=file://$TESTTMP/unused-dummystore
  > EOF

  $ echo "${LONG}ANOTHER-LFS" > f
  $ hg commit -m f -A f
  $ hgmn push -r . --to master_bookmark -v
  pushing rev e4337405c947 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  validated revset for rebase
  1 changesets found
  uncompressed size of bundle content:
       176 (changelog)
       270  f
  remote: Command failed
  remote:   Error:
  remote:     While resolving Changegroup
  remote: 
  remote:   Root cause:
  remote:     Blob is missing: alias.sha256.4200cad32a33c257258c559e80d19eedb89df109377863c6c16cf8416918b974
  remote: 
  remote:   Caused by:
  remote:     While uploading File Blobs
  remote:   Caused by:
  remote:     While decoding delta cache for file id ff714056cdbb88eef0578934980d740a05be8384, path f
  remote:   Caused by:
  remote:     Blob is missing: alias.sha256.4200cad32a33c257258c559e80d19eedb89df109377863c6c16cf8416918b974
  remote: 
  remote:   Debug context:
  remote:     Error {
  remote:         context: "While resolving Changegroup",
  remote:         source: Error {
  remote:             context: "While uploading File Blobs",
  remote:             source: Error {
  remote:                 context: "While decoding delta cache for file id ff714056cdbb88eef0578934980d740a05be8384, path f",
  remote:                 source: Missing(
  remote:                     "alias.sha256.4200cad32a33c257258c559e80d19eedb89df109377863c6c16cf8416918b974",
  remote:                 ),
  remote:             },
  remote:         },
  remote:     }
  abort: unexpected EOL, expected netstring digit
  [255]

  $ cd ..

Create a new client repository, using getpack (with its own cachepath)
  $ hgclone_treemanifest ssh://user@dummy/repo-hg-nolfs repo-hg-lfs3 --noupdate --config extensions.remotenames=
  $ cd repo-hg-lfs3
  $ setup_hg_client
  $ setup_hg_lfs "$lfs_uri" 1000B "$TESTTMP/lfs-cache3"

  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > remotenames =
  > [remotefilelog]
  > fetchpacks = True
  > getpackversion = 2
  > cachepath=$TESTTMP/cachepath-alt
  > EOF

  $ hgmn pull -v
  pulling from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  all local heads known remotely
  adding changesets
  adding manifests
  adding file changes
 
  $ hgmn update -r master_bookmark -v
  resolving manifests
  lfs: need to transfer 2 objects (3.92 KB)
  lfs: downloading d19bca751e178f8cce59e1b872e0fd5857951c2577a2318aefad3253c317d982 (1.96 KB)
  lfs: processed: d19bca751e178f8cce59e1b872e0fd5857951c2577a2318aefad3253c317d982
  lfs: downloading e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d (1.95 KB)
  lfs: processed: e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d
  3 files updated, 0 files merged, 0 files removed, 0 files unresolved

  $ sha256sum lfs-largefile
  e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d  lfs-largefile

  $ sha256sum lfs-largefile-renamed
  d19bca751e178f8cce59e1b872e0fd5857951c2577a2318aefad3253c317d982  lfs-largefile-renamed

  $ hgmn st --change . -C
  A lfs-largefile-renamed
    lfs-largefile-for-rename
  R lfs-largefile-for-rename

Change "sha256:oid" to an another valid oid to check sha1 consisnency
  $ echo "${LONG}inconsistent" > inconsistent_file
  $ hg commit -Aqm "hash check"

Corrupt file contents via an extension:
  $ cat > $TESTTMP/corrupt.py <<EOF
  > def _revision(orig, rfl, node, raw=False):
  >     return orig(rfl, node, raw).replace(b"sha256:f79cf994214182953d15cd20b2a92731052ddc9a02f4c60518dc78d7a005cca9", b"sha256:e2fff2ce58d585b4b0572e0a323f9e7e5f98cc641489e12c03c401d05d0e350d")
  > from edenscm import extensions
  > from edenscm.ext import remotefilelog
  > def uisetup(ui):
  >     extensions.wrapfunction(remotefilelog.remotefilelog.remotefilelog, "revision", _revision)
  > EOF

  $ hgmn push -r . --to master_bookmark -v --config extensions.corrupt=$TESTTMP/corrupt.py
  pushing rev 77f499cb0645 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  validated revset for rebase
  1 changesets found
  uncompressed size of bundle content:
       201 (changelog)
       286  inconsistent_file
  remote: Command failed
  remote:   Error:
  remote:     Error while uploading data for changesets, hashes: [HgChangesetId(HgNodeHash(Sha1(77f499cb064550703c65d943b8ce1b982a1293cd)))]
  remote: 
  remote:   Root cause:
  remote:     Inconsistent node hash for entry: path file 'inconsistent_file', provided: ef5953d600ca68bacb539eab8dffb415441213bb, computed: 232ec9b974a9df3d48c2b740396691fb8939976c
  remote: 
  remote:   Caused by:
  remote:     While creating Changeset Some(HgNodeHash(Sha1(77f499cb064550703c65d943b8ce1b982a1293cd))), uuid: * (glob)
  remote:   Caused by:
  remote:     While creating and verifying Changeset for blobstore
  remote:   Caused by:
  remote:     While processing entries
  remote:   Caused by:
  remote:     While uploading child entries
  remote:   Caused by:
  remote:     While walking dependencies of Root Manifest with id HgManifestId(HgNodeHash(Sha1(a1da9053000e0fb9217762d82ba5db793cfb26ce)))
  remote:   Caused by:
  remote:     Inconsistent node hash for entry: path file 'inconsistent_file', provided: ef5953d600ca68bacb539eab8dffb415441213bb, computed: 232ec9b974a9df3d48c2b740396691fb8939976c
  remote: 
  remote:   Debug context:
  remote:     Error {
  remote:         context: "Error while uploading data for changesets, hashes: [HgChangesetId(HgNodeHash(Sha1(77f499cb064550703c65d943b8ce1b982a1293cd)))]",
  remote:         source: SharedError {
  remote:             error: Error {
  remote:                 context: "While creating Changeset Some(HgNodeHash(Sha1(77f499cb064550703c65d943b8ce1b982a1293cd))), uuid: *", (glob)
  remote:                 source: Error {
  remote:                     context: "While creating and verifying Changeset for blobstore",
  remote:                     source: Error {
  remote:                         context: "While processing entries",
  remote:                         source: Error {
  remote:                             context: "While uploading child entries",
  remote:                             source: Error {
  remote:                                 context: "While walking dependencies of Root Manifest with id HgManifestId(HgNodeHash(Sha1(a1da9053000e0fb9217762d82ba5db793cfb26ce)))",
  remote:                                 source: SharedError {
  remote:                                     error: InconsistentEntryHashForPath(
  remote:                                         FilePath(
  remote:                                             NonRootMPath("inconsistent_file"),
  remote:                                         ),
  remote:                                         HgNodeHash(
  remote:                                             Sha1(ef5953d600ca68bacb539eab8dffb415441213bb),
  remote:                                         ),
  remote:                                         HgNodeHash(
  remote:                                             Sha1(232ec9b974a9df3d48c2b740396691fb8939976c),
  remote:                                         ),
  remote:                                     ),
  remote:                                 },
  remote:                             },
  remote:                         },
  remote:                     },
  remote:                 },
  remote:             },
  remote:         },
  remote:     }
  abort: unexpected EOL, expected netstring digit
  [255]

# trailing whitespace
