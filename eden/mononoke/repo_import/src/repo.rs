/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use blobrepo::AsBlobRepo;
use blobrepo::BlobRepo;
use bonsai_git_mapping::BonsaiGitMapping;
use bonsai_globalrev_mapping::BonsaiGlobalrevMapping;
use bonsai_hg_mapping::BonsaiHgMapping;
use bookmarks::BookmarkUpdateLog;
use bookmarks::Bookmarks;
use changeset_fetcher::ChangesetFetcher;
use changesets::Changesets;
use commit_graph::CommitGraph;
use filenodes::Filenodes;
use filestore::FilestoreConfig;
use metaconfig_types::RepoConfig;
use mononoke_types::RepositoryId;
use mutable_counters::MutableCounters;
use phases::Phases;
use pushrebase_mutation_mapping::PushrebaseMutationMapping;
use pushredirect::PushRedirectionConfig;
use repo_blobstore::RepoBlobstore;
use repo_bookmark_attrs::RepoBookmarkAttrs;
use repo_cross_repo::RepoCrossRepo;
use repo_derived_data::RepoDerivedData;
use repo_identity::RepoIdentity;
use repo_identity::RepoIdentityRef;
use synced_commit_mapping::SyncedCommitMapping;

#[facet::container]
#[derive(Clone)]
pub struct Repo {
    #[delegate(
        FilestoreConfig,
        RepoBlobstore,
        RepoBookmarkAttrs,
        RepoDerivedData,
        RepoIdentity,
        dyn BonsaiGitMapping,
        dyn BonsaiGlobalrevMapping,
        dyn BonsaiHgMapping,
        dyn Bookmarks,
        dyn BookmarkUpdateLog,
        dyn ChangesetFetcher,
        dyn Changesets,
        dyn Phases,
        dyn PushrebaseMutationMapping,
        dyn MutableCounters,
        CommitGraph,
        dyn Filenodes,
    )]
    blob_repo: BlobRepo,

    #[facet]
    synced_commit_mapping: dyn SyncedCommitMapping,

    #[facet]
    repo_cross_repo: RepoCrossRepo,

    #[facet]
    config: RepoConfig,

    #[facet]
    push_redirection_config: dyn PushRedirectionConfig,
}

impl Repo {
    pub fn repo_id(&self) -> RepositoryId {
        self.repo_identity().id()
    }

    pub fn name(&self) -> &str {
        self.repo_identity().name()
    }
}

impl AsBlobRepo for Repo {
    fn as_blob_repo(&self) -> &BlobRepo {
        &self.blob_repo
    }
}
