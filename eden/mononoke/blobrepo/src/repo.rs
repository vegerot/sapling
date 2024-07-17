/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;

use anyhow::Error;
use bonsai_git_mapping::BonsaiGitMapping;
use bonsai_globalrev_mapping::BonsaiGlobalrevMapping;
use bonsai_hg_mapping::BonsaiHgMapping;
use bonsai_svnrev_mapping::BonsaiSvnrevMapping;
use bonsai_tag_mapping::BonsaiTagMapping;
use bookmarks::BookmarkUpdateLog;
use bookmarks::Bookmarks;
use changeset_fetcher::ChangesetFetcher;
use changeset_fetcher::SimpleChangesetFetcher;
use changesets::Changesets;
use changesets::ChangesetsRef;
use commit_cloud::CommitCloud;
use commit_graph::CommitGraph;
use context::CoreContext;
use ephemeral_blobstore::Bubble;
use filenodes::Filenodes;
use filestore::FilestoreConfig;
use git_symbolic_refs::GitSymbolicRefs;
use mercurial_mutation::HgMutationStore;
use metaconfig_types::RepoConfig;
use mononoke_types::BonsaiChangeset;
use mutable_counters::MutableCounters;
use phases::Phases;
use pushrebase_mutation_mapping::PushrebaseMutationMapping;
use repo_blobstore::RepoBlobstore;
use repo_blobstore::RepoBlobstoreRef;
use repo_bookmark_attrs::RepoBookmarkAttrs;
use repo_derived_data::RepoDerivedData;
use repo_identity::RepoIdentity;
use repo_identity::RepoIdentityRef;
use repo_lock::RepoLock;
use repo_permission_checker::RepoPermissionChecker;

// NOTE: this structure and its fields are public to enable `DangerousOverride` functionality
#[facet::container]
#[derive(Clone)]
pub struct BlobRepoInner {
    #[facet]
    pub repo_identity: RepoIdentity,

    #[init(repo_identity.name().to_string())]
    pub reponame: String,

    #[facet]
    pub config: RepoConfig,

    #[facet]
    pub repo_blobstore: RepoBlobstore,

    #[facet]
    pub changesets: dyn Changesets,

    #[facet]
    pub changeset_fetcher: dyn ChangesetFetcher,

    #[facet]
    pub commit_graph: CommitGraph,

    #[facet]
    pub bonsai_hg_mapping: dyn BonsaiHgMapping,

    #[facet]
    pub bonsai_git_mapping: dyn BonsaiGitMapping,

    #[facet]
    pub bonsai_tag_mapping: dyn BonsaiTagMapping,

    #[facet]
    pub git_symbolic_refs: dyn GitSymbolicRefs,

    #[facet]
    pub bonsai_globalrev_mapping: dyn BonsaiGlobalrevMapping,

    #[facet]
    pub bonsai_svnrev_mapping: dyn BonsaiSvnrevMapping,

    #[facet]
    pub pushrebase_mutation_mapping: dyn PushrebaseMutationMapping,

    #[facet]
    pub bookmarks: dyn Bookmarks,

    #[facet]
    pub bookmark_update_log: dyn BookmarkUpdateLog,

    #[facet]
    pub phases: dyn Phases,

    #[facet]
    pub filestore_config: FilestoreConfig,

    #[facet]
    pub filenodes: dyn Filenodes,

    #[facet]
    pub hg_mutation_store: dyn HgMutationStore,

    #[facet]
    pub repo_derived_data: RepoDerivedData,

    #[facet]
    pub mutable_counters: dyn MutableCounters,

    #[facet]
    pub permission_checker: dyn RepoPermissionChecker,

    #[facet]
    pub repo_lock: dyn RepoLock,

    #[facet]
    pub repo_bookmark_attrs: RepoBookmarkAttrs,

    #[facet]
    pub commit_cloud: CommitCloud,
}

#[facet::container]
#[derive(Clone)]
pub struct BlobRepo {
    #[delegate(
        RepoIdentity,
        RepoBlobstore,
        dyn Changesets,
        dyn ChangesetFetcher,
        CommitGraph,
        dyn BonsaiHgMapping,
        dyn BonsaiGitMapping,
        dyn BonsaiTagMapping,
        dyn BonsaiGlobalrevMapping,
        dyn BonsaiSvnrevMapping,
        dyn PushrebaseMutationMapping,
        dyn Bookmarks,
        dyn BookmarkUpdateLog,
        dyn Phases,
        FilestoreConfig,
        dyn Filenodes,
        dyn GitSymbolicRefs,
        dyn HgMutationStore,
        RepoDerivedData,
        RepoConfig,
        dyn MutableCounters,
        dyn RepoPermissionChecker,
        dyn RepoLock,
        RepoBookmarkAttrs,
        CommitCloud
    )]
    inner: Arc<BlobRepoInner>,
}

impl BlobRepo {
    /// To be used by `DangerousOverride` only
    pub fn inner(&self) -> &Arc<BlobRepoInner> {
        &self.inner
    }

    /// To be used by `DagerouseOverride` only
    pub fn from_inner_dangerous(inner: BlobRepoInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_bubble(&self, bubble: Bubble) -> Self {
        let blobstore = bubble.wrap_repo_blobstore(self.repo_blobstore().clone());
        let changesets = Arc::new(bubble.repo_changesets(self));
        let commit_graph = Arc::new(bubble.repo_commit_graph(self));
        let changeset_fetcher =
            SimpleChangesetFetcher::new(changesets.clone(), self.repo_identity().id());
        let new_manager = self
            .inner
            .repo_derived_data
            .manager()
            .clone()
            .for_bubble(bubble);
        let repo_derived_data = self.inner.repo_derived_data.with_manager(new_manager);
        let mut inner = (*self.inner).clone();
        inner.repo_derived_data = Arc::new(repo_derived_data);
        inner.changesets = changesets;
        inner.changeset_fetcher = Arc::new(changeset_fetcher);
        inner.repo_blobstore = Arc::new(blobstore);
        inner.commit_graph = commit_graph;
        Self {
            inner: Arc::new(inner),
        }
    }
}

/// Compatibility trait for conversion between a facet-style repo (that
/// happens to contain a BlobRepo) and the blob repo (for calling things that
/// still require blobrepo).
pub trait AsBlobRepo {
    fn as_blob_repo(&self) -> &BlobRepo;
}

impl AsBlobRepo for BlobRepo {
    fn as_blob_repo(&self) -> &BlobRepo {
        self
    }
}

/// This function uploads bonsai changesets object to blobstore in parallel, and then does
/// sequential writes to changesets table. Parents of the changesets should already by saved
/// in the repository.
pub async fn save_bonsai_changesets(
    bonsai_changesets: Vec<BonsaiChangeset>,
    ctx: CoreContext,
    container: &(impl ChangesetsRef + RepoBlobstoreRef + RepoIdentityRef),
) -> Result<(), Error> {
    changesets_creation::save_changesets(&ctx, container, bonsai_changesets).await
}
