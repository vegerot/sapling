
#require no-eden


  $ setconfig format.allowbundle1=true format.usegeneraldelta=yes

bundle w/o type option

  $ newclientrepo t1
  $ newclientrepo t2
  $ cd ../t1
  $ echo blablablablabla > file.txt
  $ hg ci -Ama
  adding file.txt
  $ hg log | grep summary
  summary:     a
  $ hg bundle ../b1 test:t2_server
  searching for changes
  1 changesets found

  $ cd ../t2
  $ hg unbundle ../b1
  adding changesets
  adding manifests
  adding file changes
  $ hg up tip
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg log | grep summary
  summary:     a
  $ cd ..

Unknown compression type is rejected

  $ newclientrepo t3
  $ hg -q unbundle ../b1
  $ hg bundle -a -t unknown out.hg
  abort: unknown is not a recognized bundle specification
  (see 'hg help bundlespec' for supported values for --type)
  [255]

  $ hg bundle -a -t unknown-v2 out.hg
  abort: unknown compression is not supported
  (see 'hg help bundlespec' for supported values for --type)
  [255]

  $ cd ..

test bundle types

  $ testbundle() {
  >   echo % test bundle type $1
  >   newclientrepo t$1
  >   cd ../t1
  >   hg bundle -t $1 ../b$1 test:t${1}_server
  >   f -q -B6 -D ../b$1; echo
  >   cd ../t$1
  >   hg debugbundle ../b$1
  >   hg debugbundle --spec ../b$1
  >   echo
  >   cd ..
  > }

  $ for t in "None" "bzip2" "gzip" "none-v2" "v2" "v1" "gzip-v1"; do
  >   testbundle $t
  > done
  % test bundle type None
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  none-v2
  
  % test bundle type bzip2
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {Compression: BZ}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  bzip2-v2
  
  % test bundle type gzip
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {Compression: GZ}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  gzip-v2
  
  % test bundle type none-v2
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  none-v2
  
  % test bundle type v2
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {Compression: BZ}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  bzip2-v2
  
  % test bundle type v1
  searching for changes
  devel-warn: using deprecated bundlev1 format
   at: *changegroup* (makechangegroup) (glob)
  1 changesets found
  HG10BZ
  c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  bzip2-v1
  
  % test bundle type gzip-v1
  searching for changes
  devel-warn: using deprecated bundlev1 format
   at: *changegroup* (makechangegroup) (glob)
  1 changesets found
  HG10GZ
  c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  gzip-v1
  

Compression level can be adjusted for bundle2 bundles

  $ newclientrepo test-complevel

  $ cat > file0 << EOF
  > this is a file
  > with some text
  > and some more text
  > and other content
  > EOF
  $ cat > file1 << EOF
  > this is another file
  > with some other content
  > and repeated, repeated, repeated, repeated content
  > EOF
  $ hg -q commit -A -m initial

  $ hg bundle -a -t gzip-v2 gzip-v2.hg
  1 changesets found
#if common-zlib
  $ f --size gzip-v2.hg
  gzip-v2.hg: size=488
#endif

  $ hg --config experimental.bundlecomplevel=1 bundle -a -t gzip-v2 gzip-v2-level1.hg
  1 changesets found
#if common-zlib
  $ f --size gzip-v2-level1.hg
  gzip-v2-level1.hg: size=506
#endif

  $ cd ..

#if zstd

  $ for t in "zstd" "zstd-v2"; do
  >   testbundle $t
  > done
  % test bundle type zstd
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {Compression: ZS}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  zstd-v2
  
  % test bundle type zstd-v2
  searching for changes
  1 changesets found
  HG20\x00\x00 (esc)
  Stream params: {Compression: ZS}
  changegroup -- {nbchanges: 1, version: 02}
      c35a0f9217e65d1fdb90c936ffa7dbe679f83ddf
  b2x:treegroup2 -- {cache: False, category: manifests, version: 1}
      1 data items, 1 history items
      0b62bb0b3f9ee2ce6305e6e44861804a05547caf 
  zstd-v2
  

zstd-v1 always fails

  $ hg -R tzstd bundle -a -t zstd-v1 zstd-v1
  abort: compression engine zstd is not supported on v1 bundles
  (see 'hg help bundlespec' for supported values for --type)
  [255]

#else

zstd is a valid engine but isn't available

  $ hg -R t1 bundle -a -t zstd irrelevant.hg
  abort: compression engine zstd could not be loaded
  [255]

#endif

test garbage file

  $ echo garbage > bgarbage
  $ newclientrepo tgarbage
  $ hg unbundle ../bgarbage
  abort: ../bgarbage: not a Sapling bundle
  [255]
  $ cd ..

test invalid bundle type

  $ cd t1
  $ hg bundle -a -t garbage ../bgarbage
  abort: garbage is not a recognized bundle specification
  (see 'hg help bundlespec' for supported values for --type)
  [255]
  $ cd ..
