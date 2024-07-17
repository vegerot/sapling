# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration with some compressable files.  3 way multiplex with first two stores packed
  $ MULTIPLEXED=2 PACK_BLOB=1 setup_common_config "blob_files"
  $ cd $TESTTMP
  $ hginit_treemanifest repo-hg-nolfs
  $ cd repo-hg-nolfs
  $ setup_hg_server
  $ cp "${TEST_FIXTURES}/raw_text.txt" f1
  $ hg commit -Aqm "f1"
  $ cp f1 f2
  $ echo "More text" >> f2
  $ hg commit -Aqm "f2"
  $ cp f1 f3
  $ echo "Yet more text" >> f3
  $ hg commit -Aqm "f3"
  $ hg bookmark master_bookmark -r tip
  $ cd ..
  $ blobimport repo-hg-nolfs/.hg repo

Set up the key file for packing
  $ mkdir -p $TESTTMP/pack_key_files_0/
  $ (cd blobstore/0/blobs; ls) | sed -e 's/^blob-//' -e 's/.pack$//' >> $TESTTMP/pack_key_files_0/reporepo.store0.part1.keys.txt
  $ mkdir -p $TESTTMP/pack_key_files_1/
  $ (cd blobstore/0/blobs; ls) | sed -e 's/^blob-//' -e 's/.pack$//' >> $TESTTMP/pack_key_files_1/reporepo.store1.part1.keys.txt

Pack the blobs in the two packed stores differently
  $ packer --zstd-level=3 --keys-dir $TESTTMP/pack_key_files_0/ --tuning-info-scuba-table "file://${TESTTMP}/tuning_scuba.json"
  $ packer --zstd-level=19 --keys-dir $TESTTMP/pack_key_files_1/ --tuning-info-scuba-table "file://${TESTTMP}/tuning_scuba.json"

Run a scrub, need a scrub action to put ScrubBlobstore in the stack, which is necessary to make sure all the inner stores of the multiplex are read
  $ mononoke_walker -l loaded --blobstore-scrub-action=ReportOnly scrub -q -I deep -i bonsai -i FileContent -b master_bookmark -a all --pack-log-scuba-file pack-info-packed.json 2>&1 | strip_glog
  Seen,Loaded: 7,7, repo: repo

Check logged pack info now the store is packed. Expecting to see two packed stores and one unpacked
  $ jq -r '.int * .normal | [ .blobstore_id, .chunk_num, .blobstore_key, .node_type, .node_fingerprint, .similarity_key, .mtime, .uncompressed_size, .unique_compressed_size, .pack_key, .relevant_uncompressed_size, .relevant_compressed_size ] | @csv' < pack-info-packed.json | sort | uniq
  0,1,"repo0000.changeset.blake2.22eaf128d2cd64e1e47f9f0f091f835d893415588cb41c66d8448d892bcc0756","Changeset",-2205411614990931422,,0,108,117,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107748,45* (glob)
  0,1,"repo0000.changeset.blake2.67472b417c6772992e6c4ef87258527b01a6256ef707a3f9c5fe6bc9679499f8","Changeset",-7389730255194601625,,0,73,82,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107713,45* (glob)
  0,1,"repo0000.changeset.blake2.99283342831420aaf2c75c890cf3eb98bb26bf07e94d771cf8239b033ca45714","Changeset",-6187923334023141223,,0,108,117,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107748,45* (glob)
  0,1,"repo0000.content.blake2.4caa3d2f7430890df6f5deb3b652fcc88769e3323c0b7676e9771d172a521bbd","FileContent",975364069869333068,6905401043796602115,0,107626,2*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",21*,4* (glob)
  0,1,"repo0000.content.blake2.7f4c8284eea7351488400d6fdf82e1c262a81e20d4abd8ee469841d19b60c94a","FileContent",1456254697391410303,-6891338160001598946,0,107640,4*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",10*,4* (glob)
  0,1,"repo0000.content.blake2.ca629f1bf107b9986c1dcb16aa8aa45bc31ac0a56871c322a6cd16025b0afd09","FileContent",-7441908177121090870,-6743401566611195657,0,107636,2*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",21*,4* (glob)
  1,1,"repo0000.changeset.blake2.22eaf128d2cd64e1e47f9f0f091f835d893415588cb41c66d8448d892bcc0756","Changeset",-2205411614990931422,,0,108,117,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107748,41* (glob)
  1,1,"repo0000.changeset.blake2.67472b417c6772992e6c4ef87258527b01a6256ef707a3f9c5fe6bc9679499f8","Changeset",-7389730255194601625,,0,73,82,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107713,41* (glob)
  1,1,"repo0000.changeset.blake2.99283342831420aaf2c75c890cf3eb98bb26bf07e94d771cf8239b033ca45714","Changeset",-6187923334023141223,,0,108,117,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",107748,41* (glob)
  1,1,"repo0000.content.blake2.4caa3d2f7430890df6f5deb3b652fcc88769e3323c0b7676e9771d172a521bbd","FileContent",975364069869333068,6905401043796602115,0,107626,2*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",21*,4* (glob)
  1,1,"repo0000.content.blake2.7f4c8284eea7351488400d6fdf82e1c262a81e20d4abd8ee469841d19b60c94a","FileContent",1456254697391410303,-6891338160001598946,0,107640,4*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",10*,4* (glob)
  1,1,"repo0000.content.blake2.ca629f1bf107b9986c1dcb16aa8aa45bc31ac0a56871c322a6cd16025b0afd09","FileContent",-7441908177121090870,-6743401566611195657,0,107636,2*,"multiblob-e231aba0d585d987f88f89b06002a0351355b53c67dda35bee64840a60f98bab.pack",21*,4* (glob)
  2,1,"repo0000.changeset.blake2.22eaf128d2cd64e1e47f9f0f091f835d893415588cb41c66d8448d892bcc0756","Changeset",-2205411614990931422,,0,108,,,,
  2,1,"repo0000.changeset.blake2.67472b417c6772992e6c4ef87258527b01a6256ef707a3f9c5fe6bc9679499f8","Changeset",-7389730255194601625,,0,73,,,,
  2,1,"repo0000.changeset.blake2.99283342831420aaf2c75c890cf3eb98bb26bf07e94d771cf8239b033ca45714","Changeset",-6187923334023141223,,0,108,,,,
  2,1,"repo0000.content.blake2.4caa3d2f7430890df6f5deb3b652fcc88769e3323c0b7676e9771d172a521bbd","FileContent",975364069869333068,6905401043796602115,0,107626,,,,
  2,1,"repo0000.content.blake2.7f4c8284eea7351488400d6fdf82e1c262a81e20d4abd8ee469841d19b60c94a","FileContent",1456254697391410303,-6891338160001598946,0,107640,,,,
  2,1,"repo0000.content.blake2.ca629f1bf107b9986c1dcb16aa8aa45bc31ac0a56871c322a6cd16025b0afd09","FileContent",-7441908177121090870,-6743401566611195657,0,107636,,,,
