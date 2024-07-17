/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::time::Duration;

use anyhow::anyhow;
use anyhow::Error;
use blobstore::Loadable;
use bookmarks::BookmarkKey;
use bookmarks::BookmarksRef;
use context::CoreContext;
use derived_data::BonsaiDerived;
use futures::future;
use futures::future::try_join;
use futures::TryStreamExt;
use itertools::Itertools;
use manifest::Diff;
use manifest::ManifestOps;
use maplit::hashset;
use megarepolib::common::create_and_save_bonsai;
use megarepolib::common::ChangesetArgsFactory;
use megarepolib::common::StackPosition;
use mercurial_derivation::DeriveHgChangeset;
use metaconfig_types::PushrebaseFlags;
use mononoke_types::ChangesetId;
use mononoke_types::FileChange;
use mononoke_types::NonRootMPath;
use pushrebase::do_pushrebase_bonsai;
use regex::Regex;
use repo_blobstore::RepoBlobstoreRef;
use slog::error;
use slog::info;
use tokio::time::sleep;
use unodes::RootUnodeManifestId;

use crate::Repo;

pub async fn create_deletion_head_commits<'a>(
    ctx: &'a CoreContext,
    repo: &'a Repo,
    head_bookmark: BookmarkKey,
    commit_to_merge: ChangesetId,
    path_regex: Regex,
    deletion_chunk_size: usize,
    cs_args_factory: Box<dyn ChangesetArgsFactory>,
    pushrebase_flags: &'a PushrebaseFlags,
    wait_secs: u64,
) -> Result<(), Error> {
    let files =
        find_files_that_need_to_be_deleted(ctx, repo, &head_bookmark, commit_to_merge, path_regex)
            .await?;

    info!(ctx.logger(), "total files to delete is {}", files.len());
    for (num, chunk) in files
        .into_iter()
        .chunks(deletion_chunk_size)
        .into_iter()
        .enumerate()
    {
        let files = chunk
            .into_iter()
            .map(|path| (path, FileChange::Deletion))
            .collect();
        let maybe_head_bookmark_val = repo.bookmarks().get(ctx.clone(), &head_bookmark).await?;
        let head_bookmark_val =
            maybe_head_bookmark_val.ok_or_else(|| anyhow!("{} not found", head_bookmark))?;

        let bcs_id = create_and_save_bonsai(
            ctx,
            repo,
            vec![head_bookmark_val],
            files,
            cs_args_factory(StackPosition(num)),
        )
        .await?;
        info!(
            ctx.logger(),
            "created bonsai #{}. Deriving hg changeset for it to verify its correctness", num
        );
        let hg_cs_id = repo.derive_hg_changeset(ctx, bcs_id).await?;

        info!(ctx.logger(), "derived {}, pushrebasing...", hg_cs_id);

        let bcs = bcs_id.load(ctx, repo.repo_blobstore()).await?;
        let pushrebase_res = do_pushrebase_bonsai(
            ctx,
            repo,
            pushrebase_flags,
            &head_bookmark,
            &hashset![bcs],
            &[],
        )
        .await?;
        info!(ctx.logger(), "Pushrebased to {}", pushrebase_res.head);
        if wait_secs > 0 {
            info!(ctx.logger(), "waiting for {} seconds", wait_secs);
            sleep(Duration::from_secs(wait_secs)).await;
        }
    }

    Ok(())
}

pub async fn validate(
    ctx: &CoreContext,
    repo: &Repo,
    head_commit: ChangesetId,
    to_merge_commit: ChangesetId,
    path_regex: Regex,
) -> Result<(), Error> {
    let head_root_unode = RootUnodeManifestId::derive(ctx, repo, head_commit);
    let to_merge_commit_root_unode = RootUnodeManifestId::derive(ctx, repo, to_merge_commit);

    let (head_root_unode, to_merge_commit_root_unode) =
        try_join(head_root_unode, to_merge_commit_root_unode).await?;

    let head_leaves = head_root_unode
        .manifest_unode_id()
        .list_leaf_entries(ctx.clone(), repo.repo_blobstore().clone())
        .try_collect::<Vec<_>>();
    let to_merge_commit_leaves = to_merge_commit_root_unode
        .manifest_unode_id()
        .list_leaf_entries(ctx.clone(), repo.repo_blobstore().clone())
        .try_collect::<Vec<_>>();

    let (head_leaves, mut to_merge_commit_leaves) =
        try_join(head_leaves, to_merge_commit_leaves).await?;

    info!(
        ctx.logger(),
        "total unodes in head commit: {}",
        head_leaves.len()
    );
    info!(
        ctx.logger(),
        "total unodes in to merge commit: {}",
        to_merge_commit_leaves.len()
    );
    let mut head_leaves = head_leaves
        .into_iter()
        .filter(|(path, _)| path.matches_regex(&path_regex))
        .collect::<Vec<_>>();
    info!(
        ctx.logger(),
        "unodes in to head commit after filtering: {}",
        head_leaves.len()
    );

    head_leaves.sort();
    to_merge_commit_leaves.sort();

    if head_leaves == to_merge_commit_leaves {
        info!(ctx.logger(), "all is well");
    } else {
        error!(ctx.logger(), "validation failed!");
        for (path, unode) in head_leaves {
            println!("{}\t{}\t{}", head_commit, path, unode);
        }

        for (path, unode) in to_merge_commit_leaves {
            println!("{}\t{}\t{}", to_merge_commit, path, unode);
        }
    }
    Ok(())
}

// Returns paths of the files that:
// 1) Match `path_regex`
// 2) Either do not exist in `commit_to_merge` or have different content/filetype.
async fn find_files_that_need_to_be_deleted(
    ctx: &CoreContext,
    repo: &Repo,
    head_bookmark: &BookmarkKey,
    commit_to_merge: ChangesetId,
    path_regex: Regex,
) -> Result<Vec<NonRootMPath>, Error> {
    let maybe_head_bookmark_val = repo.bookmarks().get(ctx.clone(), head_bookmark).await?;

    let head_bookmark_val =
        maybe_head_bookmark_val.ok_or_else(|| anyhow!("{} not found", head_bookmark))?;

    let head_root_unode = RootUnodeManifestId::derive(ctx, repo, head_bookmark_val);
    let commit_to_merge_root_unode = RootUnodeManifestId::derive(ctx, repo, commit_to_merge);

    let (head_root_unode, commit_to_merge_root_unode) =
        try_join(head_root_unode, commit_to_merge_root_unode).await?;

    let mut paths = head_root_unode
        .manifest_unode_id()
        .diff(
            ctx.clone(),
            repo.repo_blobstore().clone(),
            *commit_to_merge_root_unode.manifest_unode_id(),
        )
        .try_filter_map(|diff| async move {
            use Diff::*;
            let maybe_path =
                match diff {
                    Added(_maybe_path, _entry) => None,
                    Removed(maybe_path, entry) => entry
                        .into_leaf()
                        .and(Option::<NonRootMPath>::from(maybe_path)),
                    Changed(maybe_path, _old_entry, new_entry) => new_entry
                        .into_leaf()
                        .and(Option::<NonRootMPath>::from(maybe_path)),
                };

            Ok(maybe_path)
        })
        .try_filter(|path| future::ready(path.matches_regex(&path_regex)))
        .try_collect::<Vec<_>>()
        .await?;

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod test {
    use commit_graph::CommitGraphRef;
    use fbinit::FacebookInit;
    use futures::StreamExt;
    use megarepolib::common::ChangesetArgs;
    use mononoke_types::DateTime;
    use tests_utils::bookmark;
    use tests_utils::resolve_cs_id;
    use tests_utils::CreateCommitContext;

    use super::*;

    const PATH_REGEX: &str = "^(unchanged/.*|changed/.*|toremove/.*)";

    #[fbinit::test]
    async fn test_find_files_that_needs_to_be_deleted(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);
        let repo = prepare_repo(&ctx).await?;

        let commit_to_merge = resolve_cs_id(&ctx, &repo, "commit_to_merge").await?;
        let book = BookmarkKey::new("book")?;
        let mut paths = find_files_that_need_to_be_deleted(
            &ctx,
            &repo,
            &book,
            commit_to_merge,
            Regex::new(PATH_REGEX)?,
        )
        .await?;

        paths.sort();
        assert_eq!(
            paths,
            vec![
                NonRootMPath::new("changed/a")?,
                NonRootMPath::new("changed/b")?,
                NonRootMPath::new("toremove/file1")?,
                NonRootMPath::new("toremove/file2")?,
            ]
        );

        Ok(())
    }

    #[fbinit::test]
    async fn test_find_changed_files_with_revert(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);

        let repo: Repo = test_repo_factory::build_empty(fb).await?;

        let root_commit = CreateCommitContext::new_root(&ctx, &repo)
            .add_file("file", "a")
            .commit()
            .await?;

        // Change file content and then revert it back to existing value
        let head_commit = CreateCommitContext::new(&ctx, &repo, vec![root_commit])
            .add_file("file", "b")
            .commit()
            .await?;
        let head_commit = CreateCommitContext::new(&ctx, &repo, vec![head_commit])
            .add_file("file", "a")
            .commit()
            .await?;
        bookmark(&ctx, &repo, "book").set_to(head_commit).await?;

        let commit_to_merge = CreateCommitContext::new(&ctx, &repo, vec![root_commit])
            .commit()
            .await?;

        let book = BookmarkKey::new("book")?;
        let mut paths = find_files_that_need_to_be_deleted(
            &ctx,
            &repo,
            &book,
            commit_to_merge,
            Regex::new(".*")?,
        )
        .await?;

        paths.sort();
        assert_eq!(paths, vec![NonRootMPath::new("file")?,]);

        Ok(())
    }

    #[fbinit::test]
    async fn test_create_deletion_head_commits(fb: FacebookInit) -> Result<(), Error> {
        let ctx = CoreContext::test_mock(fb);
        let repo = prepare_repo(&ctx).await?;
        let book = BookmarkKey::new("book")?;

        let commit_to_merge = resolve_cs_id(&ctx, &repo, "commit_to_merge").await?;
        let args_factory = Box::new(|stack_pos: StackPosition| ChangesetArgs {
            author: "author".to_string(),
            message: format!("{}", stack_pos.0),
            datetime: DateTime::now(),
            bookmark: None,
            mark_public: false,
        });

        let pushrebase_flags = PushrebaseFlags {
            rewritedates: true,
            forbid_p2_root_rebases: true,
            casefolding_check: true,
            recursion_limit: None,
            ..Default::default()
        };

        let commit_before_push = resolve_cs_id(&ctx, &repo, book.clone()).await?;
        create_deletion_head_commits(
            &ctx,
            &repo,
            book.clone(),
            commit_to_merge,
            Regex::new(PATH_REGEX)?,
            1,
            args_factory,
            &pushrebase_flags,
            0,
        )
        .await?;
        let commit_after_push = resolve_cs_id(&ctx, &repo, book.clone()).await?;

        let range_len = repo
            .commit_graph()
            .range_stream(&ctx, commit_before_push, commit_after_push)
            .await?
            .count()
            .await;

        // 4 new commits + commit_before_push
        assert_eq!(range_len, 4 + 1);

        let paths = find_files_that_need_to_be_deleted(
            &ctx,
            &repo,
            &book,
            commit_to_merge,
            Regex::new(PATH_REGEX)?,
        )
        .await?;

        assert!(paths.is_empty());
        Ok(())
    }

    async fn prepare_repo(ctx: &CoreContext) -> Result<Repo, Error> {
        let repo: Repo = test_repo_factory::build_empty(ctx.fb).await?;

        let root_commit = CreateCommitContext::new_root(ctx, &repo)
            .add_file("unchanged/a", "a")
            .commit()
            .await?;

        let head_commit = CreateCommitContext::new(ctx, &repo, vec![root_commit])
            .add_file("unrelated_file", "a")
            .add_file("changed/a", "oldcontent")
            .add_file("changed/b", "oldcontent")
            .add_file("toremove/file1", "content")
            .add_file("toremove/file2", "content")
            .commit()
            .await?;

        let commit_to_merge = CreateCommitContext::new(ctx, &repo, vec![root_commit])
            .add_file("changed/a", "newcontent")
            .add_file("changed/b", "newcontent")
            .commit()
            .await?;

        bookmark(ctx, &repo, "book").set_to(head_commit).await?;
        bookmark(ctx, &repo, "commit_to_merge")
            .set_to(commit_to_merge)
            .await?;

        Ok(repo)
    }
}
