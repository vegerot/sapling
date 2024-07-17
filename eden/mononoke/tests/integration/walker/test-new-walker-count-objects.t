# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration
  $ default_setup_pre_blobimport "blob_files"
  hg repo
  o  C [draft;rev=2;26805aba1e60]
  │
  o  B [draft;rev=1;112478962961]
  │
  o  A [draft;rev=0;426bada5c675]
  $
  $ blobimport repo-hg/.hg repo --derived-data-type=blame --derived-data-type=changeset_info --derived-data-type=deleted_manifest --derived-data-type=fastlog --derived-data-type=fsnodes --derived-data-type=skeleton_manifests --derived-data-type=unodes

check blobstore numbers, walk will do some more steps for mappings
  $ BLOBPREFIX="$TESTTMP/blobstore/blobs/blob-repo0000"
  $ BONSAICOUNT=$(ls $BLOBPREFIX.changeset.* $BLOBPREFIX.content.* $BLOBPREFIX.content_metadata2.* | wc -l)
  $ echo "$BONSAICOUNT"
  9
  $ HGCOUNT=$(ls $BLOBPREFIX.* | grep -E '.(filenode_lookup|hgchangeset|hgfilenode|hgmanifest).' | wc -l)
  $ echo "$HGCOUNT"
  12
  $ BLOBCOUNT=$(ls $BLOBPREFIX.* | grep -v .alias. | wc -l)
  $ echo "$BLOBCOUNT"
  64

count-objects, all types, shallow edges
  $ mononoke_walker -l loaded scrub -q -b master_bookmark -I shallow -i all 2>&1 | strip_glog
  Seen,Loaded: 51,51, repo: repo

count-objects, all types, deep edges
  $ mononoke_walker -l loaded scrub -q -b master_bookmark -I deep -i all 2>&1 | strip_glog
  Seen,Loaded: 86,86, repo: repo

count-objects, all types, all edges, difference in final count vs deep edges is PhaseMapping and one extra BonsaiHgMapping from the bookmark
  $ mononoke_walker -l loaded scrub -q -b master_bookmark -I all -i all 2>&1 | strip_glog
  Seen,Loaded: 89,89, repo: repo

count-objects, bonsai core data.  total nodes is BONSAICOUNT plus one for the root bookmark step.
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I bonsai 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToBonsaiParent, ChangesetToFileContent], repo: repo
  Walking node types [Bookmark, Changeset, FileContent], repo: repo
  Seen,Loaded: 7,7, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:3,* FileContent:3,* (glob)

count-objects, shallow, bonsai only.  No parents, expect just one of each node type. Also exclude FsnodeToFileContent to keep the test intact
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -X hg -x BonsaiHgMapping -X FsnodeToFileContent -i default -i derived_fsnodes 2>&1 | strip_glog
  Walking edge types [AliasContentMappingToFileContent, BookmarkToChangeset, ChangesetToFileContent, ChangesetToFsnodeMapping, FileContentMetadataV2ToGitSha1Alias, FileContentMetadataV2ToSeededBlake3Alias, FileContentMetadataV2ToSha1Alias, FileContentMetadataV2ToSha256Alias, FileContentToFileContentMetadataV2, FsnodeMappingToRootFsnode, FsnodeToChildFsnode], repo: repo
  Walking node types [AliasContentMapping, Bookmark, Changeset, FileContent, FileContentMetadataV2, Fsnode, FsnodeMapping], repo: repo
  Seen,Loaded: 10,10, repo: repo
  * Type:Walked,Checks,Children AliasContentMapping:4,* Bookmark:1,* Changeset:1,* FileContent:1,* FileContentMetadataV2:1,* Fsnode:1,* FsnodeMapping:1,* (glob)

count-objects, hg only. total nodes is HGCOUNT plus 1 for the root bookmark step, plus 1 for mapping from bookmark to hg. plus 3 for filenode (same blob as envelope)
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I hg 2>&1 | strip_glog
  Walking edge types [BonsaiHgMappingToHgChangesetViaBonsai, BookmarkToBonsaiHgMapping, HgChangesetToHgManifest, HgChangesetToHgParent, HgChangesetViaBonsaiToHgChangeset, HgFileEnvelopeToFileContent, HgFileNodeToHgCopyfromFileNode, HgFileNodeToHgParentFileNode, HgFileNodeToLinkedHgChangeset, HgManifestToChildHgManifest, HgManifestToHgFileEnvelope, HgManifestToHgFileNode], repo: repo
  Walking node types [BonsaiHgMapping, Bookmark, FileContent, HgChangeset, HgChangesetViaBonsai, HgFileEnvelope, HgFileNode, HgManifest], repo: repo
  Seen,Loaded: 20,20, repo: repo
  * Type:Walked,Checks,Children BonsaiHgMapping:1,* Bookmark:1,* FileContent:3,* HgChangeset:3,* HgChangesetViaBonsai:3,* HgFileEnvelope:3,* HgFileNode:3,* HgManifest:3,* (glob)

count-objects, default shallow walk across bonsai and hg data, but exclude HgFileEnvelope so that we can test that we visit FileContent from fsnodes
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -x HgFileEnvelope -i default -i derived_fsnodes 2>&1 | strip_glog
  Walking edge types [AliasContentMappingToFileContent, BonsaiHgMappingToHgChangesetViaBonsai, BookmarkToChangeset, ChangesetToBonsaiHgMapping, ChangesetToFileContent, ChangesetToFsnodeMapping, FileContentMetadataV2ToGitSha1Alias, FileContentMetadataV2ToSeededBlake3Alias, FileContentMetadataV2ToSha1Alias, FileContentMetadataV2ToSha256Alias, FileContentToFileContentMetadataV2, FsnodeMappingToRootFsnode, FsnodeToChildFsnode, FsnodeToFileContent, HgChangesetToHgManifest, HgChangesetViaBonsaiToHgChangeset, HgManifestToChildHgManifest, HgManifestToHgFileNode], repo: repo
  Walking node types [AliasContentMapping, BonsaiHgMapping, Bookmark, Changeset, FileContent, FileContentMetadataV2, Fsnode, FsnodeMapping, HgChangeset, HgChangesetViaBonsai, HgFileNode, HgManifest], repo: repo
  Seen,Loaded: 29,29, repo: repo
  * Type:Walked,Checks,Children AliasContentMapping:12,* BonsaiHgMapping:1,* Bookmark:1,* Changeset:1,* FileContent:3,* FileContentMetadataV2:3,* Fsnode:1,* FsnodeMapping:1,* HgChangeset:1,* HgChangesetViaBonsai:1,* HgFileNode:3,* HgManifest:1,* (glob)

count-objects, default shallow walk across bonsai and hg data, including mutable
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -I marker 2>&1 | strip_glog
  Walking edge types [AliasContentMappingToFileContent, BonsaiHgMappingToHgChangesetViaBonsai, BookmarkToChangeset, ChangesetToBonsaiHgMapping, ChangesetToFileContent, ChangesetToPhaseMapping, FileContentMetadataV2ToGitSha1Alias, FileContentMetadataV2ToSeededBlake3Alias, FileContentMetadataV2ToSha1Alias, FileContentMetadataV2ToSha256Alias, FileContentToFileContentMetadataV2, HgChangesetToHgManifest, HgChangesetViaBonsaiToHgChangeset, HgFileEnvelopeToFileContent, HgManifestToChildHgManifest, HgManifestToHgFileEnvelope, HgManifestToHgFileNode], repo: repo
  Walking node types [AliasContentMapping, BonsaiHgMapping, Bookmark, Changeset, FileContent, FileContentMetadataV2, HgChangeset, HgChangesetViaBonsai, HgFileEnvelope, HgFileNode, HgManifest, PhaseMapping], repo: repo
  Seen,Loaded: 31,31, repo: repo
  * Type:Walked,Checks,Children AliasContentMapping:12,* BonsaiHgMapping:1,* Bookmark:1,* Changeset:1,* FileContent:3,* FileContentMetadataV2:3,* HgChangeset:1,* HgChangesetViaBonsai:1,* HgFileEnvelope:3,* HgFileNode:3,* HgManifest:1,* PhaseMapping:1,* (glob)

count-objects, default shallow walk across bonsai and hg data, including mutable for all public heads
  $ mononoke_walker -L sizing scrub -q --walk-root PublishedBookmarks -I shallow -I marker 2>&1 | strip_glog
  Walking edge types [AliasContentMappingToFileContent, BonsaiHgMappingToHgChangesetViaBonsai, ChangesetToBonsaiHgMapping, ChangesetToFileContent, ChangesetToPhaseMapping, FileContentMetadataV2ToGitSha1Alias, FileContentMetadataV2ToSeededBlake3Alias, FileContentMetadataV2ToSha1Alias, FileContentMetadataV2ToSha256Alias, FileContentToFileContentMetadataV2, HgChangesetToHgManifest, HgChangesetViaBonsaiToHgChangeset, HgFileEnvelopeToFileContent, HgManifestToChildHgManifest, HgManifestToHgFileEnvelope, HgManifestToHgFileNode, PublishedBookmarksToBonsaiHgMapping, PublishedBookmarksToChangeset], repo: repo
  Walking node types [AliasContentMapping, BonsaiHgMapping, Changeset, FileContent, FileContentMetadataV2, HgChangeset, HgChangesetViaBonsai, HgFileEnvelope, HgFileNode, HgManifest, PhaseMapping, PublishedBookmarks], repo: repo
  Suppressing edge OutgoingEdge { label: ChangesetToBonsaiHgMapping, target: BonsaiHgMapping(ChangesetKey { inner: ChangesetId(Blake2(c3384961b16276f2db77df9d7c874bbe981cf0525bd6f84a502f919044f2dabd)), filenode_known_derived: false }), path: None }, repo: repo
  Seen,Loaded: 31,31, repo: repo
  * Type:Walked,Checks,Children AliasContentMapping:12,* BonsaiHgMapping:1,* Changeset:1,* FileContent:3,* FileContentMetadataV2:3,* HgChangeset:1,* HgChangesetViaBonsai:1,* HgFileEnvelope:3,* HgFileNode:3,* HgManifest:1,* PhaseMapping:1,* PublishedBookmarks:1,* (glob)

count-objects, shallow walk across bonsai and changeset_info
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_changeset_info 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetInfoMappingToChangesetInfo, ChangesetToChangesetInfoMapping], repo: repo
  Walking node types [Bookmark, Changeset, ChangesetInfo, ChangesetInfoMapping], repo: repo
  Seen,Loaded: 4,4, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:1,* ChangesetInfo:1,* ChangesetInfoMapping:1,* (glob)

count-objects, deep walk across bonsai and changeset_info
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_changeset_info 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetInfoMappingToChangesetInfo, ChangesetInfoToChangesetInfoParent, ChangesetToBonsaiParent, ChangesetToChangesetInfoMapping], repo: repo
  Walking node types [Bookmark, Changeset, ChangesetInfo, ChangesetInfoMapping], repo: repo
  Seen,Loaded: 10,10, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:3,* ChangesetInfo:3,* ChangesetInfoMapping:3,* (glob)

count-objects, shallow walk across bonsai and unodes
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_unodes -i FileContent -X ChangesetToFileContent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToUnodeMapping, UnodeFileToFileContent, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Bookmark, Changeset, FileContent, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 10,10, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:1,* FileContent:3,* UnodeFile:3,* UnodeManifest:1,* UnodeMapping:1,* (glob)

count-objects, deep walk across bonsai and unodes
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_unodes -X ChangesetToBonsaiParent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToUnodeMapping, UnodeFileToLinkedChangeset, UnodeFileToUnodeFileParent, UnodeManifestToLinkedChangeset, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeManifestToUnodeManifestParent, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Bookmark, Changeset, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 13,13, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:3,* UnodeFile:3,* UnodeManifest:3,* UnodeMapping:3,* (glob)

count-objects, shallow walk across blame
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_unodes -i derived_blame -X ChangesetToFileContent -X UnodeFileToFileContent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToUnodeMapping, UnodeFileToBlame, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Blame, Bookmark, Changeset, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 10,10, repo: repo
  * Type:Walked,Checks,Children Blame:3,* Bookmark:1,* Changeset:1,* UnodeFile:3,* UnodeManifest:1,* UnodeMapping:1,* (glob)

count-objects, deep walk across blame
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_unodes -i derived_blame -X ChangesetToBonsaiParent -X UnodeFileToLinkedChangeset -X UnodeManifestToLinkedChangeset 2>&1 | strip_glog
  Walking edge types [BlameToChangeset, BookmarkToChangeset, ChangesetToUnodeMapping, UnodeFileToBlame, UnodeFileToUnodeFileParent, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeManifestToUnodeManifestParent, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Blame, Bookmark, Changeset, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 16,16, repo: repo
  * Type:Walked,Checks,Children Blame:3,* Bookmark:1,* Changeset:3,* UnodeFile:3,* UnodeManifest:3,* UnodeMapping:3,* (glob)

count-objects, shallow walk across deleted manifest
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_deleted_manifest -X ChangesetToFileContent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToDeletedManifestV2Mapping, DeletedManifestV2MappingToRootDeletedManifestV2, DeletedManifestV2ToDeletedManifestV2Child], repo: repo
  Walking node types [Bookmark, Changeset, DeletedManifestV2, DeletedManifestV2Mapping], repo: repo
  Seen,Loaded: 4,4, repo: repo
  * Type:Walked,Checks,Children * DeletedManifestV2:1,* DeletedManifestV2Mapping:1,* (glob)

count-objects, deep walk across deleted manifest
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_deleted_manifest 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToBonsaiParent, ChangesetToDeletedManifestV2Mapping, DeletedManifestV2MappingToRootDeletedManifestV2, DeletedManifestV2ToDeletedManifestV2Child, DeletedManifestV2ToLinkedChangeset], repo: repo
  Walking node types [Bookmark, Changeset, DeletedManifestV2, DeletedManifestV2Mapping], repo: repo
  Seen,Loaded: 8,8, repo: repo
  * Type:Walked,Checks,Children * DeletedManifestV2:1,* DeletedManifestV2Mapping:3,* (glob)

count-objects, shallow walk across skeleton manifest
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_skeleton_manifests -X ChangesetToFileContent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToSkeletonManifestMapping, SkeletonManifestMappingToRootSkeletonManifest, SkeletonManifestToSkeletonManifestChild], repo: repo
  Walking node types [Bookmark, Changeset, SkeletonManifest, SkeletonManifestMapping], repo: repo
  Seen,Loaded: 4,4, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:1,* SkeletonManifest:1,* SkeletonManifestMapping:1,* (glob)

count-objects, deep walk across skeleton manifest
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_skeleton_manifests 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToBonsaiParent, ChangesetToSkeletonManifestMapping, SkeletonManifestMappingToRootSkeletonManifest, SkeletonManifestToSkeletonManifestChild], repo: repo
  Walking node types [Bookmark, Changeset, SkeletonManifest, SkeletonManifestMapping], repo: repo
  Seen,Loaded: 10,10, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:3,* SkeletonManifest:3,* SkeletonManifestMapping:3,* (glob)

count-objects, shallow walk across fastlog
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -i bonsai -i derived_unodes -i derived_fastlog -X ChangesetToFileContent -X UnodeFileToFileContent 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToUnodeMapping, FastlogBatchToPreviousBatch, FastlogDirToPreviousBatch, FastlogFileToPreviousBatch, UnodeFileToFastlogFile, UnodeManifestToFastlogDir, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Bookmark, Changeset, FastlogBatch, FastlogDir, FastlogFile, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 11,11, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:1,* FastlogDir:1,* FastlogFile:3,* UnodeFile:3,* UnodeManifest:1,* UnodeMapping:1,* (glob)

count-objects, deep walk across fastlog
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -i bonsai -i derived_unodes -i derived_fastlog -X ChangesetToBonsaiParent -X UnodeFileToLinkedChangeset -X UnodeManifestToLinkedChangeset 2>&1 | strip_glog
  Walking edge types [BookmarkToChangeset, ChangesetToUnodeMapping, FastlogBatchToChangeset, FastlogBatchToPreviousBatch, FastlogDirToChangeset, FastlogDirToPreviousBatch, FastlogFileToChangeset, FastlogFileToPreviousBatch, UnodeFileToFastlogFile, UnodeFileToUnodeFileParent, UnodeManifestToFastlogDir, UnodeManifestToUnodeFileChild, UnodeManifestToUnodeManifestChild, UnodeManifestToUnodeManifestParent, UnodeMappingToRootUnodeManifest], repo: repo
  Walking node types [Bookmark, Changeset, FastlogBatch, FastlogDir, FastlogFile, UnodeFile, UnodeManifest, UnodeMapping], repo: repo
  Seen,Loaded: 19,19, repo: repo
  * Type:Walked,Checks,Children Bookmark:1,* Changeset:3,* FastlogDir:3,* FastlogFile:3,* UnodeFile:3,* UnodeManifest:3,* UnodeMapping:3,* (glob)

count-objects, shallow walk across hg
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I shallow -I BookmarkToBonsaiHgMapping -i Bookmark -i hg 2>&1 | strip_glog
  Walking edge types [BonsaiHgMappingToHgChangesetViaBonsai, BookmarkToBonsaiHgMapping, HgChangesetToHgManifest, HgChangesetToHgManifestFileNode, HgChangesetViaBonsaiToHgChangeset, HgManifestToChildHgManifest, HgManifestToHgFileEnvelope, HgManifestToHgFileNode, HgManifestToHgManifestFileNode], repo: repo
  Walking node types [BonsaiHgMapping, Bookmark, HgChangeset, HgChangesetViaBonsai, HgFileEnvelope, HgFileNode, HgManifest, HgManifestFileNode], repo: repo
  Seen,Loaded: 12,12, repo: repo
  * Type:Walked,Checks,Children BonsaiHgMapping:1,* Bookmark:1,* HgChangeset:1,* HgChangesetViaBonsai:1,* HgFileEnvelope:3,* HgFileNode:3,* HgManifest:1,* (glob)

count-objects, deep walk across hg
  $ mononoke_walker -L sizing scrub -q -b master_bookmark -I deep -I BookmarkToBonsaiHgMapping -i Bookmark -i hg 2>&1 | strip_glog
  Walking edge types [BonsaiHgMappingToHgChangesetViaBonsai, BookmarkToBonsaiHgMapping, HgChangesetToHgManifest, HgChangesetToHgManifestFileNode, HgChangesetToHgParent, HgChangesetViaBonsaiToHgChangeset, HgFileNodeToHgCopyfromFileNode, HgFileNodeToHgParentFileNode, HgFileNodeToLinkedHgBonsaiMapping, HgFileNodeToLinkedHgChangeset, HgManifestFileNodeToHgCopyfromFileNode, HgManifestFileNodeToHgParentFileNode, HgManifestFileNodeToLinkedHgBonsaiMapping, HgManifestFileNodeToLinkedHgChangeset, HgManifestToChildHgManifest, HgManifestToHgFileEnvelope, HgManifestToHgFileNode], repo: repo
  Walking node types [BonsaiHgMapping, Bookmark, HgBonsaiMapping, HgChangeset, HgChangesetViaBonsai, HgFileEnvelope, HgFileNode, HgManifest, HgManifestFileNode], repo: repo
  Seen,Loaded: 23,23, repo: repo
  * Type:Walked,Checks,Children BonsaiHgMapping:1,* Bookmark:1,* HgBonsaiMapping:3,* HgChangeset:3,* HgChangesetViaBonsai:3,* HgFileEnvelope:3,* HgFileNode:3,* HgManifest:3,* (glob)
