/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

mod ancestors_difference;
mod children;
mod common_base;
mod descendants;
mod is_ancestor;
mod range_stream;
mod segments;
mod slice_ancestors;
mod update_preloaded;

use ancestors_difference::AncestorsDifferenceArgs;
use anyhow::Result;
use bonsai_git_mapping::BonsaiGitMapping;
use bonsai_globalrev_mapping::BonsaiGlobalrevMapping;
use bonsai_hg_mapping::BonsaiHgMapping;
use bonsai_svnrev_mapping::BonsaiSvnrevMapping;
use children::ChildrenArgs;
use clap::Parser;
use clap::Subcommand;
use commit_graph::CommitGraph;
use commit_graph::CommitGraphWriter;
use common_base::CommonBaseArgs;
use descendants::DescendantsArgs;
use is_ancestor::IsAncestorArgs;
use metaconfig_types::RepoConfig;
use mononoke_app::args::RepoArgs;
use mononoke_app::MononokeApp;
use range_stream::RangeStreamArgs;
use repo_blobstore::RepoBlobstore;
use repo_identity::RepoIdentity;
use segments::SegmentsArgs;
use slice_ancestors::SliceAncestorsArgs;
use update_preloaded::UpdatePreloadedArgs;

/// Query and manage the commit graph
#[derive(Parser)]
pub struct CommandArgs {
    #[clap(flatten)]
    repo: RepoArgs,

    #[clap(subcommand)]
    subcommand: CommitGraphSubcommand,
}

#[derive(Subcommand)]
pub enum CommitGraphSubcommand {
    /// Display ids of all commits that are ancestors of one set of commits (heads),
    /// excluding ancestors of another set of commits (common) in reverse topological order.
    AncestorsDifference(AncestorsDifferenceArgs),
    /// Display ids of all commits that are simultaneously a descendant of one commit (start)
    /// and an ancestor of another (end) in topological order.
    RangeStream(RangeStreamArgs),
    /// Update preloaded commit graph and upload it to blobstore.
    UpdatePreloaded(UpdatePreloadedArgs),
    /// Display ids of all the highest generation commits among the common ancestors of two commits.
    CommonBase(CommonBaseArgs),
    /// Slices ancestors of given commits and displays commits IDs of frontiers for each slice.
    SliceAncestors(SliceAncestorsArgs),
    /// Display ids of all children commits of a given commit.
    Children(ChildrenArgs),
    /// Display ids of the union of descendants of the given commits.
    Descendants(DescendantsArgs),
    /// Display segments representing ancestors of one set of commits (heads), excluding
    /// ancestors of another set of commits (common) in reverse topological order.
    Segments(SegmentsArgs),
    /// Check if a commit is an ancestor of another commit.
    IsAncestor(IsAncestorArgs),
}

#[facet::container]
pub struct Repo {
    #[facet]
    commit_graph: CommitGraph,

    #[facet]
    commit_graph_writer: dyn CommitGraphWriter,

    #[facet]
    config: RepoConfig,

    #[facet]
    id: RepoIdentity,

    #[facet]
    bonsai_hg_mapping: dyn BonsaiHgMapping,

    #[facet]
    bonsai_git_mapping: dyn BonsaiGitMapping,

    #[facet]
    bonsai_globalrev_mapping: dyn BonsaiGlobalrevMapping,

    #[facet]
    bonsai_svnrev_mapping: dyn BonsaiSvnrevMapping,

    #[facet]
    repo_blobstore: RepoBlobstore,
}

pub async fn run(app: MononokeApp, args: CommandArgs) -> Result<()> {
    let ctx = app.new_basic_context();
    let repo: Repo = app.open_repo(&args.repo).await?;

    match args.subcommand {
        CommitGraphSubcommand::AncestorsDifference(args) => {
            ancestors_difference::ancestors_difference(&ctx, &repo, args).await
        }
        CommitGraphSubcommand::RangeStream(args) => {
            range_stream::range_stream(&ctx, &repo, args).await
        }
        CommitGraphSubcommand::UpdatePreloaded(args) => {
            update_preloaded::update_preloaded(&ctx, &app, &repo, args).await
        }
        CommitGraphSubcommand::CommonBase(args) => {
            common_base::common_base(&ctx, &repo, args).await
        }
        CommitGraphSubcommand::SliceAncestors(args) => {
            slice_ancestors::slice_ancestors(&ctx, &repo, args).await
        }
        CommitGraphSubcommand::Children(args) => children::children(&ctx, &repo, args).await,
        CommitGraphSubcommand::Descendants(args) => {
            descendants::descendants(&ctx, &repo, args).await
        }
        CommitGraphSubcommand::Segments(args) => segments::segments(&ctx, &repo, args).await,
        CommitGraphSubcommand::IsAncestor(args) => {
            is_ancestor::is_ancestor(&ctx, &repo, args).await
        }
    }
}
