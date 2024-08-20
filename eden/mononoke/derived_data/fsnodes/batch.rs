/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::Context;
use anyhow::Error;
use blobstore::Loadable;
use borrowed::borrowed;
use context::CoreContext;
use derived_data::batch::split_batch_in_linear_stacks;
use derived_data::batch::FileConflicts;
use derived_data::batch::StackItem;
use derived_data_manager::BonsaiDerivable;
use derived_data_manager::DerivationContext;
use futures::stream::FuturesOrdered;
use futures::stream::TryStreamExt;
use mononoke_types::ChangesetId;
use mononoke_types::FsnodeId;
use stats::prelude::*;

use crate::derive::derive_fsnodes_stack;
use crate::RootFsnodeId;

define_stats! {
    prefix = "mononoke.derived_data.fsnodes";
    new_parallel: timeseries(Rate, Sum),
}

/// Derive a batch of fsnodes, potentially doing it faster than deriving fsnodes sequentially.
/// The primary purpose of this is to be used while backfilling fsnodes for a large repository.
///
/// The best results are achieved if a batch is a linear stack (i.e. no merges) of commits where batch[i-1] is a parent
/// of batch[i]. However if it's not the case then using derive_fsnode_in_batch shouldn't be much slower than a sequential
/// derivation of the same commits.
///
/// `derive_fsnode_in_batch` proceed in a few stages:
/// 1) Split `batch` in a a few linear stacks (there are certain rules about how it can be done, see `split_batch_in_linear_stacks` for more details)
/// 2) Stacks are processed one after another (i.e. we get benefits from parallel execution only if two commits are in the same stack)
/// 3) For each commit stack derive fsnode commits in parallel. This is done by calling `derive_fsnode()`
///    with parents of the first commit in the stack, and all bonsai file changes since first commit in the stack. See example below:
///
///   Stack:
///     Commit 1 - Added "file1" with content "A", has parent commit 0
///     Commit 2 - Added "file2" with content "B", has parent commit 1
///     Commit 3 - Modified "file1" with content "C", has parent commit 2
///
///   We make three derive_fsnode() calls in parallel with these parameters:
///      derive_fsnode([commit0], {"file1" => "A"})
///      derive_fsnode([commit0], {"file1" => "A", "file2" => "B"})
///      derive_fsnode([commit0], {"file1" => "C", "file2" => "B"})
///
/// So effectively we combine the changes from all commits in the stack. Note that it's not possible to do it for unodes
/// because unodes depend on the order of the changes.
///
/// Fsnode derivation can be cpu-bounded, and the speed up is achieved by spawning derivation on different
/// tokio tasks - this allows us to use more cpu.
pub async fn derive_fsnode_in_batch(
    ctx: &CoreContext,
    derivation_ctx: &DerivationContext,
    batch: Vec<ChangesetId>,
) -> Result<HashMap<ChangesetId, RootFsnodeId>, Error> {
    let linear_stacks = split_batch_in_linear_stacks(
        ctx,
        derivation_ctx.blobstore(),
        batch,
        FileConflicts::ChangeDelete.into(),
    )
    .await?;
    let mut res: HashMap<ChangesetId, RootFsnodeId> = HashMap::new();
    for linear_stack in linear_stacks {
        // Fetch the parent fsnodes, either from a previous iteration of this
        // loop (which will have stored the mapping in `res`), or from the
        // main mapping, where they should already be derived.
        let parent_fsnodes = linear_stack
            .parents
            .into_iter()
            .map(|p| {
                borrowed!(res);
                async move {
                    anyhow::Result::<_>::Ok(
                        match res.get(&p) {
                            Some(fsnode_id) => fsnode_id.clone(),
                            None => derivation_ctx.fetch_dependency(ctx, p).await?,
                        }
                        .into_fsnode_id(),
                    )
                }
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect::<Vec<_>>()
            .await?;

        STATS::new_parallel.add_value(1);
        new_batch_derivation(
            ctx,
            derivation_ctx,
            parent_fsnodes,
            linear_stack.stack_items,
            &mut res,
        )
        .await?;
    }

    Ok(res)
}

pub async fn new_batch_derivation(
    ctx: &CoreContext,
    derivation_ctx: &DerivationContext,
    parent_fsnode_manifests: Vec<FsnodeId>,
    file_changes: Vec<StackItem>,
    already_derived: &mut HashMap<ChangesetId, RootFsnodeId>,
) -> Result<(), Error> {
    if parent_fsnode_manifests.len() > 1 {
        // we can't derive stack for a merge commit,
        // so let's derive it without batching
        for item in file_changes {
            let bonsai = item.cs_id.load(ctx, derivation_ctx.blobstore()).await?;
            let parents = derivation_ctx
                .fetch_unknown_parents(ctx, Some(already_derived), &bonsai)
                .await?;
            let derived = RootFsnodeId::derive_single(ctx, derivation_ctx, bonsai, parents).await?;
            already_derived.insert(item.cs_id, derived);
        }
    } else {
        let first = file_changes.first().map(|item| item.cs_id);
        let last = file_changes.last().map(|item| item.cs_id);

        let file_changes = file_changes
            .into_iter()
            .map(|item| (item.cs_id, item.per_commit_file_changes))
            .collect::<Vec<_>>();

        let derived = derive_fsnodes_stack(
            ctx,
            derivation_ctx,
            file_changes,
            parent_fsnode_manifests.first().copied(),
        )
        .await
        .with_context(|| format!("failed deriving stack of {:?} to {:?}", first, last,))?;

        already_derived.extend(
            derived
                .into_iter()
                .map(|(csid, mf_id)| (csid, RootFsnodeId(mf_id))),
        );
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use bonsai_hg_mapping::BonsaiHgMapping;
    use bookmarks::Bookmarks;
    use commit_graph::CommitGraph;
    use commit_graph::CommitGraphRef;
    use commit_graph::CommitGraphWriter;
    use fbinit::FacebookInit;
    use filestore::FilestoreConfig;
    use fixtures::Linear;
    use fixtures::TestRepoFixture;
    use repo_blobstore::RepoBlobstore;
    use repo_derived_data::RepoDerivedData;
    use repo_derived_data::RepoDerivedDataRef;
    use repo_identity::RepoIdentity;
    use test_repo_factory::TestRepoFactory;
    use tests_utils::bookmark;
    use tests_utils::drawdag::create_from_dag;
    use tests_utils::resolve_cs_id;

    use super::*;

    #[facet::container]
    #[derive(Clone)]
    struct TestRepo(
        dyn BonsaiHgMapping,
        dyn Bookmarks,
        CommitGraph,
        dyn CommitGraphWriter,
        RepoDerivedData,
        RepoBlobstore,
        FilestoreConfig,
        RepoIdentity,
    );

    #[fbinit::test]
    async fn batch_derive(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);
        let new_batch = {
            let repo: TestRepo = Linear::get_repo(fb).await;
            let master_cs_id = resolve_cs_id(&ctx, &repo, "master").await?;

            let mut cs_ids = repo
                .commit_graph()
                .ancestors_difference(&ctx, vec![master_cs_id], vec![])
                .await?;
            cs_ids.reverse();

            let manager = repo.repo_derived_data().manager();

            manager
                .derive_exactly_batch::<RootFsnodeId>(&ctx, cs_ids, None)
                .await?;

            manager
                .fetch_derived::<RootFsnodeId>(&ctx, master_cs_id, None)
                .await?
                .unwrap()
                .into_fsnode_id()
        };

        let sequential = {
            let repo: TestRepo = Linear::get_repo(fb).await;
            let master_cs_id = resolve_cs_id(&ctx, &repo, "master").await?;
            repo.repo_derived_data()
                .manager()
                .derive::<RootFsnodeId>(&ctx, master_cs_id, None)
                .await?
                .into_fsnode_id()
        };

        assert_eq!(new_batch, sequential);
        Ok(())
    }

    #[fbinit::test]
    async fn batch_derive_with_merge(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);
        let new_batch = {
            let repo = repo_with_merge(&ctx).await?;
            let master_cs_id = resolve_cs_id(&ctx, &repo, "master").await?;

            let mut cs_ids = repo
                .commit_graph()
                .ancestors_difference(&ctx, vec![master_cs_id], vec![])
                .await?;
            cs_ids.reverse();

            let manager = repo.repo_derived_data().manager();
            manager
                .derive_exactly_batch::<RootFsnodeId>(&ctx, cs_ids, None)
                .await?;

            manager
                .fetch_derived::<RootFsnodeId>(&ctx, master_cs_id, None)
                .await?
                .unwrap()
                .into_fsnode_id()
        };

        let sequential = {
            let repo = repo_with_merge(&ctx).await?;
            let master_cs_id = resolve_cs_id(&ctx, &repo, "master").await?;
            repo.repo_derived_data()
                .manager()
                .derive::<RootFsnodeId>(&ctx, master_cs_id, None)
                .await?
                .into_fsnode_id()
        };

        assert_eq!(new_batch, sequential);
        Ok(())
    }

    async fn repo_with_merge(ctx: &CoreContext) -> Result<TestRepo, Error> {
        let repo: TestRepo = TestRepoFactory::new(ctx.fb)?.build().await?;

        let commit_map = create_from_dag(
            ctx,
            &repo,
            r##"
            A-M
             /
            B
            "##,
        )
        .await?;

        let m = commit_map.get("M").unwrap();
        bookmark(ctx, &repo, "master").set_to(*m).await?;

        Ok(repo)
    }
}
