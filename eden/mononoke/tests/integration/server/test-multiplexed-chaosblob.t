# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ MULTIPLEXED=2 default_setup_blobimport "blob_files"
  hg repo
  o  C [draft;rev=2;26805aba1e60]
  │
  o  B [draft;rev=1;112478962961]
  │
  o  A [draft;rev=0;426bada5c675]
  $
  blobimporting

Base case, check the stores have expected counts
  $ ls blobstore/0/blobs/ | wc -l
  33
  $ ls blobstore/1/blobs/ | wc -l
  33
  $ ls blobstore/2/blobs/ | wc -l
  33


Erase the sqllites and blobstore_sync_queue
  $ rm -rf "$TESTTMP/blobstore/"*/blobs/*

blobimport them into Mononoke storage again, but with failures on one side
  $ blobimport repo-hg/.hg repo --blobstore-write-chaos-rate=1

Check the stores have expected counts
  $ ls blobstore/0/blobs/ | wc -l
  0
  $ ls blobstore/1/blobs/ | wc -l
  33
  $ ls blobstore/2/blobs/ | wc -l
  33

Check that healer queue has items
  $ read_blobstore_wal_queue_size
  33
