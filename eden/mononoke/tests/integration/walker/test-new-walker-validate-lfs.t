# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ LFS_THRESHOLD=1 default_setup_blobimport "blob_files"
  hg repo
  o  C [draft;rev=2;26805aba1e60]
  │
  o  B [draft;rev=1;112478962961]
  │
  o  A [draft;rev=0;426bada5c675]
  $
  blobimporting

validate with LFS enabled, shallow
  $ mononoke_walker --scuba-dataset file://scuba-validate-shallow.json -L graph validate --include-check-type=FileContentIsLfs -I shallow -I BookmarkToBonsaiHgMapping -i hg -x HgFileNode -i FileContent -i FileContentMetadataV2 -q -b master_bookmark 2>&1 | strip_glog
  Performing check types [FileContentIsLfs], repo: repo
  Seen,Loaded: 15,15, repo: repo
  Nodes,Pass,Fail:15,3,0; EdgesChecked:3; CheckType:Pass,Fail Total:3,0 FileContentIsLfs:3,0, repo: repo

Check scuba data is logged for lfs and that it contains useful hg changeset and path in via_node_key and node_path.  As its shallow walk expect all via_node_key to be the same
  $ wc -l < scuba-validate-shallow.json
  3
  $ jq -r '.int * .normal | [ .check_fail, .check_type, .check_size, .node_key, .node_path, .node_type, .repo, .src_node_type, .via_node_key, .via_node_type, .walk_type, .error_msg ] | @csv' < scuba-validate-shallow.json | sort
  0,"file_content_is_lfs",1,"content.blake2.55662471e2a28db8257939b2f9a2d24e65b46a758bac12914a58f17dcde6905f","B","FileContentMetadataV2","repo","HgFileEnvelope","hgchangeset.sha1.26805aba1e600a82e93661149f2313866a221a7b","HgChangeset","validate",
  0,"file_content_is_lfs",1,"content.blake2.896ad5879a5df0403bfc93fc96507ad9c93b31b11f3d0fa05445da7918241e5d","C","FileContentMetadataV2","repo","HgFileEnvelope","hgchangeset.sha1.26805aba1e600a82e93661149f2313866a221a7b","HgChangeset","validate",
  0,"file_content_is_lfs",1,"content.blake2.eb56488e97bb4cf5eb17f05357b80108a4a71f6c3bab52dfcaec07161d105ec9","A","FileContentMetadataV2","repo","HgFileEnvelope","hgchangeset.sha1.26805aba1e600a82e93661149f2313866a221a7b","HgChangeset","validate",

Make a commit for a file in a subdir path
  $ cd repo-hg
  $ mkdir foo
  $ cd foo
  $ mkcommit bar
  $ cd ../..
  $ blobimport repo-hg/.hg repo

validate with LFS enabled, deep.  Params are setup so that ValidateRoute contains the HgChangeset that originated a Bonsai and then the Bonsai points to the files it touched.
  $ mononoke_walker --scuba-dataset file://scuba-validate-deep.json validate --include-check-type=FileContentIsLfs -I deep -X HgFileNodeToLinkedHgChangeset -X HgFileNodeToHgParentFileNode -X HgFileNodeToHgCopyfromFileNode -X ChangesetToBonsaiParent -X ChangesetToBonsaiHgMapping -X HgChangesetToHgParent -i default -x HgFileEnvelope -x AliasContentMapping -q -p BonsaiHgMapping 2>&1 | strip_glog
  Walking edge types [BonsaiHgMappingToHgChangesetViaBonsai, ChangesetToFileContent, FileContentToFileContentMetadataV2, HgBonsaiMappingToChangeset, HgChangesetToHgManifest, HgChangesetViaBonsaiToHgChangeset, HgFileNodeToLinkedHgBonsaiMapping, HgManifestToChildHgManifest, HgManifestToHgFileNode], repo: repo
  Walking node types [BonsaiHgMapping, Changeset, FileContent, FileContentMetadataV2, HgBonsaiMapping, HgChangeset, HgChangesetViaBonsai, HgFileNode, HgManifest], repo: repo
  Performing check types [FileContentIsLfs], repo: repo
  Repo bounds: (1, 5), repo: repo
  Starting chunk 1 with bounds (1, 5), repo: repo
  Seen,Loaded: * (glob)
  Walked* (glob)
  Nodes,Pass,Fail:37,4,0; EdgesChecked:4; CheckType:Pass,Fail Total:4,0 FileContentIsLfs:4,0, repo: repo
  Deferred: 0, repo: repo
  Completed in 1 chunks of size 100000, repo: repo

Check scuba data is logged for lfs and that it contains useful hg changeset and path in via_node_key and node_path
  $ wc -l < scuba-validate-deep.json
  4
  $ jq -r '.int * .normal | [ .check_fail, .check_type, .node_key, .node_path, .node_type, .repo, .src_node_type, .via_node_key, .via_node_type, .walk_type, .error_msg ] | @csv' < scuba-validate-deep.json | sort
  0,"file_content_is_lfs","content.blake2.55662471e2a28db8257939b2f9a2d24e65b46a758bac12914a58f17dcde6905f","B","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.112478962961147124edd43549aedd1a335e44bf","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.896ad5879a5df0403bfc93fc96507ad9c93b31b11f3d0fa05445da7918241e5d","C","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.26805aba1e600a82e93661149f2313866a221a7b","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.e164fd53a3714f754d5f5763688bea02d99123436e51e9ed9c85ad04fdc52222","foo/bar","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.5792aaeebbba3ab28cd80600dbddd96184b1b986","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.eb56488e97bb4cf5eb17f05357b80108a4a71f6c3bab52dfcaec07161d105ec9","A","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.426bada5c67598ca65036d57d9e4b64b0c1ce7a0","HgBonsaiMapping","validate",


validate with LFS enabled, deep with simpler query.  Should have same output but touch less nodes to get there.
  $ mononoke_walker --scuba-dataset file://scuba-validate-deep2.json validate --include-check-type=FileContentIsLfs -I deep -I BonsaiHgMappingToHgBonsaiMapping -X BonsaiHgMappingToHgChangesetViaBonsai -X ChangesetToBonsaiParent -X ChangesetToBonsaiHgMapping -i bonsai -i FileContent -i FileContentMetadataV2 -i HgBonsaiMapping -i BonsaiHgMapping -q -p BonsaiHgMapping 2>&1 | strip_glog
  Walking edge types [BonsaiHgMappingToHgBonsaiMapping, ChangesetToFileContent, FileContentToFileContentMetadataV2, HgBonsaiMappingToChangeset], repo: repo
  Walking node types [BonsaiHgMapping, Changeset, FileContent, FileContentMetadataV2, HgBonsaiMapping], repo: repo
  Performing check types [FileContentIsLfs], repo: repo
  Repo bounds: (1, 5), repo: repo
  Starting chunk 1 with bounds (1, 5), repo: repo
  Seen,Loaded: * (glob)
  Walked* (glob)
  Nodes,Pass,Fail:20,4,0; EdgesChecked:4; CheckType:Pass,Fail Total:4,0 FileContentIsLfs:4,0, repo: repo
  Deferred: 0, repo: repo
  Completed in 1 chunks of size 100000, repo: repo

Check scuba data is logged for lfs and that it contains useful hg changeset and path in via_node_key and node_path
  $ wc -l < scuba-validate-deep2.json
  4
  $ jq -r '.int * .normal | [ .check_fail, .check_type, .node_key, .node_path, .node_type, .repo, .src_node_type, .via_node_key, .via_node_type, .walk_type, .error_msg ] | @csv' < scuba-validate-deep2.json | sort
  0,"file_content_is_lfs","content.blake2.55662471e2a28db8257939b2f9a2d24e65b46a758bac12914a58f17dcde6905f","B","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.112478962961147124edd43549aedd1a335e44bf","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.896ad5879a5df0403bfc93fc96507ad9c93b31b11f3d0fa05445da7918241e5d","C","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.26805aba1e600a82e93661149f2313866a221a7b","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.e164fd53a3714f754d5f5763688bea02d99123436e51e9ed9c85ad04fdc52222","foo/bar","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.5792aaeebbba3ab28cd80600dbddd96184b1b986","HgBonsaiMapping","validate",
  0,"file_content_is_lfs","content.blake2.eb56488e97bb4cf5eb17f05357b80108a4a71f6c3bab52dfcaec07161d105ec9","A","FileContentMetadataV2","repo","Changeset","hgchangeset.sha1.426bada5c67598ca65036d57d9e4b64b0c1ce7a0","HgBonsaiMapping","validate",
