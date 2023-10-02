/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::Error;
use async_trait::async_trait;
use blobrepo::BlobRepo;
use bookmarks::BookmarkKey;
use bookmarks::BookmarkUpdateReason;
use borrowed::borrowed;
use bytes::Bytes;
use context::CoreContext;
use fbinit::FacebookInit;
use filestore::StoreRequest;
use futures::future::try_join_all;
use futures::stream;
use maplit::btreemap;
use mercurial_derivation::MappedHgChangesetId;
use mercurial_types::HgChangesetId;
use mercurial_types::NonRootMPath;
use mononoke_api_types::InnerRepo;
use mononoke_types::BonsaiChangeset;
use mononoke_types::BonsaiChangesetMut;
use mononoke_types::ChangesetId;
use mononoke_types::DateTime;
use mononoke_types::FileChange;
use mononoke_types::FileType;
use mononoke_types::RepositoryId;
use sorted_vector_map::SortedVectorMap;
use test_repo_factory::TestRepoFactory;
use test_repo_factory::TestRepoFactoryBuilder;
use tests_utils::BasicTestRepo;
use tests_utils::Repo;

pub async fn store_files(
    ctx: &CoreContext,
    files: BTreeMap<&str, Option<&str>>,
    repo: &impl Repo,
) -> SortedVectorMap<NonRootMPath, FileChange> {
    let mut res = BTreeMap::new();

    for (path, content) in files {
        let path = NonRootMPath::new(path).unwrap();
        match content {
            Some(content) => {
                let content = Bytes::copy_from_slice(content.as_bytes());
                let size = content.len() as u64;
                let metadata = filestore::store(
                    repo.repo_blobstore(),
                    *repo.filestore_config(),
                    ctx,
                    &StoreRequest::new(size),
                    stream::once(async { Ok(content) }),
                )
                .await
                .unwrap();
                let file_change =
                    FileChange::tracked(metadata.content_id, FileType::Regular, size, None);
                res.insert(path, file_change);
            }
            None => {
                res.insert(path, FileChange::Deletion);
            }
        }
    }
    res.into()
}

async fn create_bonsai_changeset_from_test_data(
    fb: FacebookInit,
    repo: &impl Repo,
    files: BTreeMap<&str, Option<&str>>,
    commit_metadata: BTreeMap<&str, &str>,
) {
    let ctx = CoreContext::test_mock(fb);
    let file_changes = store_files(&ctx, files, repo).await;
    let date: Vec<_> = commit_metadata
        .get("author_date")
        .unwrap()
        .split(' ')
        .map(|s| s.parse::<i64>().unwrap())
        .collect();

    let parents = commit_metadata
        .get("parents")
        .unwrap()
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(|s| HgChangesetId::from_str(s).unwrap())
        .map(|p| {
            borrowed!(ctx, repo);
            async move {
                repo.bonsai_hg_mapping()
                    .get_bonsai_from_hg(ctx, p)
                    .await
                    .map(|maybe_cs| maybe_cs.unwrap())
            }
        });

    let bonsai_parents = try_join_all(parents).await.unwrap();

    #[allow(clippy::get_first)]
    let bcs = BonsaiChangesetMut {
        parents: bonsai_parents,
        author: commit_metadata.get("author").unwrap().to_string(),
        author_date: DateTime::from_timestamp(*date.get(0).unwrap(), *date.get(1).unwrap() as i32)
            .unwrap(),
        committer: None,
        committer_date: None,
        message: commit_metadata.get("message").unwrap().to_string(),
        hg_extra: Default::default(),
        git_extra_headers: None,
        git_tree_hash: None,
        file_changes,
        is_snapshot: false,
        git_annotated_tag: None,
    }
    .freeze()
    .unwrap();

    let bcs_id = bcs.get_changeset_id();

    changesets_creation::save_changesets(&ctx, repo, vec![bcs])
        .await
        .unwrap();

    let hg_cs = repo
        .repo_derived_data()
        .derive::<MappedHgChangesetId>(&ctx, bcs_id)
        .await
        .map(|id| id.hg_changeset_id())
        .unwrap();

    assert_eq!(
        hg_cs,
        HgChangesetId::from_str(commit_metadata.get("expected_hg_changeset").unwrap()).unwrap()
    );
}

pub async fn set_bookmark(
    fb: FacebookInit,
    repo: &impl Repo,
    hg_cs_id: &str,
    bookmark: BookmarkKey,
) {
    let ctx = CoreContext::test_mock(fb);
    let hg_cs_id = HgChangesetId::from_str(hg_cs_id).unwrap();
    let bcs_id = repo
        .bonsai_hg_mapping()
        .get_bonsai_from_hg(&ctx, hg_cs_id)
        .await
        .unwrap();
    let mut txn = repo.bookmarks().create_transaction(ctx.clone());
    txn.force_set(&bookmark, bcs_id.unwrap(), BookmarkUpdateReason::TestMove)
        .unwrap();
    txn.commit().await.unwrap();
}

#[async_trait]
pub trait TestRepoFixture {
    const REPO_NAME: &'static str;

    async fn initrepo(fb: FacebookInit, repo: &impl Repo);

    async fn get_test_repo(fb: FacebookInit) -> BasicTestRepo {
        let repo: BasicTestRepo = TestRepoFactory::new(fb)
            .unwrap()
            .with_id(RepositoryId::new(0))
            .with_name(Self::REPO_NAME.to_string())
            .build()
            .await
            .unwrap();
        Self::initrepo(fb, &repo).await;
        repo
    }

    async fn get_custom_test_repo<
        R: Repo + for<'builder> facet::AsyncBuildable<'builder, TestRepoFactoryBuilder<'builder>>,
    >(
        fb: FacebookInit,
    ) -> R {
        Self::get_custom_test_repo_with_id(fb, RepositoryId::new(0)).await
    }

    async fn get_custom_test_repo_with_id<
        R: Repo + for<'builder> facet::AsyncBuildable<'builder, TestRepoFactoryBuilder<'builder>>,
    >(
        fb: FacebookInit,
        id: RepositoryId,
    ) -> R {
        let repo: R = TestRepoFactory::new(fb)
            .unwrap()
            .with_id(id)
            .with_name(Self::REPO_NAME.to_string())
            .build()
            .await
            .unwrap();
        Self::initrepo(fb, &repo).await;
        repo
    }

    // This method should be considered as deprecated. For new tests, please use `get_test_repo`
    // instead.
    async fn getrepo(fb: FacebookInit) -> BlobRepo {
        Self::get_inner_repo(fb).await.blob_repo
    }

    async fn get_inner_repo(fb: FacebookInit) -> InnerRepo {
        Self::get_inner_repo_with_id(fb, RepositoryId::new(0)).await
    }

    async fn getrepo_with_id(fb: FacebookInit, id: RepositoryId) -> BlobRepo {
        Self::get_inner_repo_with_id(fb, id).await.blob_repo
    }

    async fn get_inner_repo_with_id(fb: FacebookInit, id: RepositoryId) -> InnerRepo {
        let repo: InnerRepo = TestRepoFactory::new(fb)
            .unwrap()
            .with_id(id)
            .with_name(Self::REPO_NAME.to_string())
            .build()
            .await
            .unwrap();
        Self::initrepo(fb, &repo.blob_repo).await;
        repo
    }
}

pub struct Linear;

#[async_trait]
impl TestRepoFixture for Linear {
    const REPO_NAME: &'static str = "linear";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "1" => Some("1\n"),
            "files" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041758 25200",
            "message"=> "added 1",
            "expected_hg_changeset"=> "2d7d4ba9ce0a6ffd222de7785b249ead9c51c536",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
            "files" => Some("1\n2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "2d7d4ba9ce0a6ffd222de7785b249ead9c51c536",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041758 25200",
            "message"=> "added 2",
            "expected_hg_changeset"=> "3e0e761030db6e479a7fb58b12881883f9f8c63f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
            "files" => Some("1\n2\n3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3e0e761030db6e479a7fb58b12881883f9f8c63f",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041758 25200",
            "message"=> "added 3",
            "expected_hg_changeset"=> "607314ef579bd2407752361ba1b0c1729d08b281",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
            "files" => Some("1\n2\n3\n4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "607314ef579bd2407752361ba1b0c1729d08b281",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041759 25200",
            "message"=> "added 4",
            "expected_hg_changeset"=> "d0a361e9022d226ae52f689667bd7d212a19cfe0",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
            "files" => Some("1\n2\n3\n4\n5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d0a361e9022d226ae52f689667bd7d212a19cfe0",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041759 25200",
            "message"=> "added 5",
            "expected_hg_changeset"=> "cb15ca4a43a59acff5388cea9648c162afde8372",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "6" => Some("6\n"),
            "files" => Some("1\n2\n3\n4\n5\n6\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "cb15ca4a43a59acff5388cea9648c162afde8372",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041760 25200",
            "message"=> "added 6",
            "expected_hg_changeset"=> "eed3a8c0ec67b6a6fe2eb3543334df3f0b4f202b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "7" => Some("7\n"),
            "files" => Some("1\n2\n3\n4\n5\n6\n7\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "eed3a8c0ec67b6a6fe2eb3543334df3f0b4f202b",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041760 25200",
            "message"=> "added 7",
            "expected_hg_changeset"=> "0ed509bf086fadcb8a8a5384dc3b550729b0fc17",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "8" => Some("8\n"),
            "files" => Some("1\n2\n3\n4\n5\n6\n7\n8\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "0ed509bf086fadcb8a8a5384dc3b550729b0fc17",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041761 25200",
            "message"=> "added 8",
            "expected_hg_changeset"=> "a9473beb2eb03ddb1cccc3fbaeb8a4820f9cd157",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "9" => Some("9\n"),
            "files" => Some("1\n2\n3\n4\n5\n6\n7\n8\n9\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "a9473beb2eb03ddb1cccc3fbaeb8a4820f9cd157",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041761 25200",
            "message"=> "added 9",
            "expected_hg_changeset"=> "3c15267ebf11807f3d772eb891272b911ec68759",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "10" => Some("10\n"),
            "files" => Some("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3c15267ebf11807f3d772eb891272b911ec68759",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041761 25200",
            "message"=> "added 10",
            "expected_hg_changeset"=> "a5ffa77602a066db7d5cfb9fb5823a0895717c5a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "10" => Some("modified10\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "a5ffa77602a066db7d5cfb9fb5823a0895717c5a",
            "author"=> "Jeremy Fitzhardinge <jsgf@fb.com>",
            "author_date"=> "1504041761 25200",
            "message"=> "modified 10",
            "expected_hg_changeset"=> "79a13814c5ce7330173ec04d279bf95ab3f652fb",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "79a13814c5ce7330173ec04d279bf95ab3f652fb",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct BranchEven;

#[async_trait]
impl TestRepoFixture for BranchEven {
    const REPO_NAME: &'static str = "branch_even";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "base" => Some("base\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430496 25200",
            "message"=> "base",
            "expected_hg_changeset"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430535 25200",
            "message"=> "Branch 1",
            "expected_hg_changeset"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430543 25200",
            "message"=> "Branch 2",
            "expected_hg_changeset"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430612 25200",
            "message"=> "Doubled",
            "expected_hg_changeset"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430628 25200",
            "message"=> "Add one",
            "expected_hg_changeset"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435041 25200",
            "message"=> "I think 4 is a nice number",
            "expected_hg_changeset"=> "16839021e338500b3cf7c9b871c8a07351697d68",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435062 25200",
            "message"=> "Replace the base",
            "expected_hg_changeset"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct BranchUneven;

#[async_trait]
impl TestRepoFixture for BranchUneven {
    const REPO_NAME: &'static str = "branch_uneven";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "base" => Some("base\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430496 25200",
            "message"=> "base",
            "expected_hg_changeset"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430535 25200",
            "message"=> "Branch 1",
            "expected_hg_changeset"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430543 25200",
            "message"=> "Branch 2",
            "expected_hg_changeset"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430612 25200",
            "message"=> "Doubled",
            "expected_hg_changeset"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430628 25200",
            "message"=> "Add one",
            "expected_hg_changeset"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435041 25200",
            "message"=> "I think 4 is a nice number",
            "expected_hg_changeset"=> "16839021e338500b3cf7c9b871c8a07351697d68",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435062 25200",
            "message"=> "Replace the base",
            "expected_hg_changeset"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "795b8133cf375f6d68d27c6c23db24cd5d0cd00f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "795b8133cf375f6d68d27c6c23db24cd5d0cd00f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "bc7b4d0f858c19e2474b03e442b8495fd7aeef33",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "bc7b4d0f858c19e2474b03e442b8495fd7aeef33",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "fc2cef43395ff3a7b28159007f63d6529d2f41ca",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "fc2cef43395ff3a7b28159007f63d6529d2f41ca",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "5d43888a3c972fe68c224f93d41b30e9f888df7c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "5d43888a3c972fe68c224f93d41b30e9f888df7c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "264f01429683b3dd8042cb3979e8bf37007118bc",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "264f01429683b3dd8042cb3979e8bf37007118bc",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct BranchWide;

#[async_trait]
impl TestRepoFixture for BranchWide {
    const REPO_NAME: &'static str = "branch_wide";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506511707 25200",
            "message"=> "One",
            "expected_hg_changeset"=> "ecba698fee57eeeef88ac3dcc3b623ede4af47bd",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2.1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "ecba698fee57eeeef88ac3dcc3b623ede4af47bd",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506511730 25200",
            "message"=> "Two.one",
            "expected_hg_changeset"=> "9e8521affb7f9d10e9551a99c526e69909042b20",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2.2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "ecba698fee57eeeef88ac3dcc3b623ede4af47bd",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506511742 25200",
            "message"=> "Two.two",
            "expected_hg_changeset"=> "4685e9e62e4885d477ead6964a7600c750e39b03",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3.1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "9e8521affb7f9d10e9551a99c526e69909042b20",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506512909 25200",
            "message"=> "Three.one",
            "expected_hg_changeset"=> "b6a8169454af58b4b72b3665f9aa0d25529755ff",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3.2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "9e8521affb7f9d10e9551a99c526e69909042b20",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506512921 25200",
            "message"=> "Three.two",
            "expected_hg_changeset"=> "c27ef5b7f15e9930e5b93b1f32cc2108a2aabe12",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3.3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "4685e9e62e4885d477ead6964a7600c750e39b03",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506512936 25200",
            "message"=> "Three.three",
            "expected_hg_changeset"=> "04decbb0d1a65789728250ddea2fe8d00248e01c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3.4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "4685e9e62e4885d477ead6964a7600c750e39b03",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506512945 25200",
            "message"=> "Three.four",
            "expected_hg_changeset"=> "49f53ab171171b3180e125b918bd1cf0af7e5449",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "49f53ab171171b3180e125b918bd1cf0af7e5449",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct MergeEven;

#[async_trait]
impl TestRepoFixture for MergeEven {
    const REPO_NAME: &'static str = "merge_even";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par

        // Common commit
        let files = btreemap! {
            "base" => Some("base\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430496 25200",
            "message"=> "base",
            "expected_hg_changeset"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // First branch
        let files = btreemap! {
            "branch" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430543 25200",
            "message"=> "Branch 2",
            "expected_hg_changeset"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430612 25200",
            "message"=> "Doubled",
            "expected_hg_changeset"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
        };

        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        let files = btreemap! {
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435062 25200",
            "message"=> "Replace the base",
            "expected_hg_changeset"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Second branch
        let files = btreemap! {
            "branch" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430535 25200",
            "message"=> "Branch 1",
            "expected_hg_changeset"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430628 25200",
            "message"=> "Add one",
            "expected_hg_changeset"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435041 25200",
            "message"=> "I think 4 is a nice number",
            "expected_hg_changeset"=> "16839021e338500b3cf7c9b871c8a07351697d68",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Merge
        let files = btreemap! {
            "branch" => Some("4\n"),
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed 16839021e338500b3cf7c9b871c8a07351697d68",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435224 25200",
            "message"=> "Merge",
            "expected_hg_changeset"=> "1f6bc010883e397abeca773192f3370558ee1320",
            "changed_files"=> "branch",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        set_bookmark(
            fb,
            repo,
            "1f6bc010883e397abeca773192f3370558ee1320",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct ManyFilesDirs;

#[async_trait]
impl TestRepoFixture for ManyFilesDirs {
    const REPO_NAME: &'static str = "many_files_dirs";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Stanislau Hlebik <stash@fb.com>",
            "author_date"=> "1516807909 28800",
            "message"=> "1",
            "expected_hg_changeset"=> "5a28e25f924a5d209b82ce0713d8d83e68982bc8",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
            "dir1/file_1_in_dir1" => Some("content1\n"),
            "dir1/file_2_in_dir1" => Some("content3\n"),
            "dir1/subdir1/file_1" => Some("content4\n"),
            "dir2/file_1_in_dir2" => Some("content2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "5a28e25f924a5d209b82ce0713d8d83e68982bc8",
            "author"=> "Stanislau Hlebik <stash@fb.com>",
            "author_date"=> "1516808095 28800",
            "message"=> "2",
            "expected_hg_changeset"=> "2f866e7e549760934e31bf0420a873f65100ad63",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "dir1/subdir1/subsubdir1/file_1" => Some("content5\n"),
            "dir1/subdir1/subsubdir2/file_1" => Some("content6\n"),
            "dir1/subdir1/subsubdir2/file_2" => Some("content7\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "2f866e7e549760934e31bf0420a873f65100ad63",
            "author"=> "Stanislau Hlebik <stash@fb.com>",
            "author_date"=> "1516808173 28800",
            "message"=> "3",
            "expected_hg_changeset"=> "d261bc7900818dea7c86935b3fb17a33b2e3a6b4",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "dir1" => Some("dir1content\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d261bc7900818dea7c86935b3fb17a33b2e3a6b4",
            "author"=> "Stanislau Hlebik <stash@fb.com>",
            "author_date"=> "1516963897 28800",
            "message"=> "replace dir1 with a file",
            "expected_hg_changeset"=> "051946ed218061e925fb120dac02634f9ad40ae2",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "051946ed218061e925fb120dac02634f9ad40ae2",
            BookmarkKey::new("master").unwrap(),
        )
        .await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Stanislau Hlebik <stash@fb.com>",
            "author_date"=> "1516807909 28800",
            "message"=> "1",
            "expected_hg_changeset"=> "5a28e25f924a5d209b82ce0713d8d83e68982bc8",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;
        set_bookmark(
            fb,
            repo,
            "5a28e25f924a5d209b82ce0713d8d83e68982bc8",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct MergeUneven;

#[async_trait]
impl TestRepoFixture for MergeUneven {
    const REPO_NAME: &'static str = "merge_uneven";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par

        // Common commit
        let files = btreemap! {
            "base" => Some("base\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430496 25200",
            "message"=> "base",
            "expected_hg_changeset"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // First branch
        let files = btreemap! {
            "branch" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430543 25200",
            "message"=> "Branch 2",
            "expected_hg_changeset"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "d7542c9db7f4c77dab4b315edd328edf1514952f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430612 25200",
            "message"=> "Doubled",
            "expected_hg_changeset"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "b65231269f651cfe784fd1d97ef02a049a37b8a0",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435062 25200",
            "message"=> "Replace the base",
            "expected_hg_changeset"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "4f7f3fd428bec1a48f9314414b063c706d9c1aed",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "795b8133cf375f6d68d27c6c23db24cd5d0cd00f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "795b8133cf375f6d68d27c6c23db24cd5d0cd00f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "bc7b4d0f858c19e2474b03e442b8495fd7aeef33",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "bc7b4d0f858c19e2474b03e442b8495fd7aeef33",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "fc2cef43395ff3a7b28159007f63d6529d2f41ca",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "fc2cef43395ff3a7b28159007f63d6529d2f41ca",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "5d43888a3c972fe68c224f93d41b30e9f888df7c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "5d43888a3c972fe68c224f93d41b30e9f888df7c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "264f01429683b3dd8042cb3979e8bf37007118bc",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Second branch

        let files = btreemap! {
            "branch" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "15c40d0abc36d47fb51c8eaec51ac7aad31f669c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430535 25200",
            "message"=> "Branch 1",
            "expected_hg_changeset"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3cda5c78aa35f0f5b09780d971197b51cad4613a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430628 25200",
            "message"=> "Add one",
            "expected_hg_changeset"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1d8a907f7b4bf50c6a09c16361e2205047ecc5e5",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435041 25200",
            "message"=> "I think 4 is a nice number",
            "expected_hg_changeset"=> "16839021e338500b3cf7c9b871c8a07351697d68",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Merge
        let files = btreemap! {
            "1" => Some("1\n"),
            "2" => Some("2\n"),
            "3" => Some("3\n"),
            "4" => Some("4\n"),
            "5" => Some("5\n"),
            "branch" => Some("4\n"),
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "264f01429683b3dd8042cb3979e8bf37007118bc 16839021e338500b3cf7c9b871c8a07351697d68",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435672 25200",
            "message"=> "Merge two branches",
            "expected_hg_changeset"=> "d35b1875cdd1ed2c687e86f1604b9d7e989450cb",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        set_bookmark(
            fb,
            repo,
            "d35b1875cdd1ed2c687e86f1604b9d7e989450cb",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct MergeMultipleFiles;

#[async_trait]
impl TestRepoFixture for MergeMultipleFiles {
    const REPO_NAME: &'static str = "merge_multiple_files";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // Common commit
        let files = btreemap! {
            "base" => Some("common\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430496 25200",
            "message"=> "base",
            "expected_hg_changeset"=> "860e94a2c490c3ea07ad8e6482c1b53708705565",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // First branch
        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "860e94a2c490c3ea07ad8e6482c1b53708705565",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506430612 25200",
            "message"=> "Doubled",
            "expected_hg_changeset"=> "f7281f23f4ff6b323a86faffe1527bc3931caad8",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f7281f23f4ff6b323a86faffe1527bc3931caad8",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435062 25200",
            "message"=> "Replace the base",
            "expected_hg_changeset"=> "0ecdd411e73b404bf45ea94f86477c4beb202646",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "0ecdd411e73b404bf45ea94f86477c4beb202646",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435631 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "2f340e879ba100e97fe43fafb1357e01b4e046c0",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("other\n"),
        };
        let commit_metadata: BTreeMap<&str, &str> = btreemap! {
            "parents"=> "2f340e879ba100e97fe43fafb1357e01b4e046c0",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435632 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "c0c7af787afb8dffa4eab1eb45019ab4ac9e8688",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "c0c7af787afb8dffa4eab1eb45019ab4ac9e8688",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435633 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "5e09a5d3676c8b51db7fee4aa6ce393871860569",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Second branch
        let files = btreemap! {
            "branch" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "860e94a2c490c3ea07ad8e6482c1b53708705565",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435041 25200",
            "message"=> "I think 4 is a nice number",
            "expected_hg_changeset"=> "f2765c353d10cc1666a7cb6d2eed1d3b1ca04edb",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "base" => Some("other common\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f2765c353d10cc1666a7cb6d2eed1d3b1ca04edb",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435051 25200",
            "message"=> "Other common",
            "expected_hg_changeset"=> "3e672a42c4af4459354c82d4c21a0e7566c1e431",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("some other\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3e672a42c4af4459354c82d4c21a0e7566c1e431",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506435061 25200",
            "message"=> "some other",
            "expected_hg_changeset"=> "a291c0b59375c5321da2a77e215647b405c8cb79",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        // Merge
        let files = btreemap! {
            "1" => Some("1\n"),
            "2" => Some("2\n"),
            "3" => Some("3\n"),
            "4" => Some("4\n"),
            "5" => Some("5\n"),
            "branch" => Some("4\n"),
            "base" => Some("branch1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "5e09a5d3676c8b51db7fee4aa6ce393871860569 a291c0b59375c5321da2a77e215647b405c8cb79",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "150643562 25200",
            "message"=> "Merge two branches",
            "expected_hg_changeset"=> "c7bfbeed73ed19b01f5309716164d5b37725a61d",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        set_bookmark(
            fb,
            repo,
            "c7bfbeed73ed19b01f5309716164d5b37725a61d",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct UnsharedMergeEven;

#[async_trait]
impl TestRepoFixture for UnsharedMergeEven {
    const REPO_NAME: &'static str = "unshared_merge_even";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "side" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506441702 25200",
            "message"=> "One",
            "expected_hg_changeset"=> "9d374b7e8180f933e3043ad1ffab0a9f95e2bac6",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "side" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506441720 25200",
            "message"=> "Two",
            "expected_hg_changeset"=> "1700524113b1a3b1806560341009684b4378660b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1700524113b1a3b1806560341009684b4378660b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442623 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "36ff88dd69c9966c9fad9d6d0457c52153039dde",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "36ff88dd69c9966c9fad9d6d0457c52153039dde",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "f61fdc0ddafd63503dcd8eed8994ec685bfc8941",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f61fdc0ddafd63503dcd8eed8994ec685bfc8941",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "0b94a2881dda90f0d64db5fae3ee5695a38e7c8f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "0b94a2881dda90f0d64db5fae3ee5695a38e7c8f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "2fa8b4ee6803a18db4649a3843a723ef1dfe852b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "2fa8b4ee6803a18db4649a3843a723ef1dfe852b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "03b0589d9788870817d03ce7b87516648ed5b33a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "9d374b7e8180f933e3043ad1ffab0a9f95e2bac6",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "3775a86c64cceeaf68ffe3f012fc90774c42002b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3775a86c64cceeaf68ffe3f012fc90774c42002b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "eee492dcdeaae18f91822c4359dd516992e0dbcd",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "eee492dcdeaae18f91822c4359dd516992e0dbcd",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "163adc0d0f5d2eb0695ca123addcb92bab202096",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "163adc0d0f5d2eb0695ca123addcb92bab202096",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "f01e186c165a2fbe931fd1bf4454235398c591c9",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f01e186c165a2fbe931fd1bf4454235398c591c9",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442629 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "33fb49d8a47b29290f5163e30b294339c89505a2",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "side" => Some("merge\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "33fb49d8a47b29290f5163e30b294339c89505a2 03b0589d9788870817d03ce7b87516648ed5b33a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442663 25200",
            "message"=> "Merge",
            "expected_hg_changeset"=> "d592490c4386cdb3373dd93af04d563de199b2fb",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {};
        let commit_metadata = btreemap! {
            "parents"=> "d592490c4386cdb3373dd93af04d563de199b2fb",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443414 25200",
            "message"=> "And work",
            "expected_hg_changeset"=> "7fe9947f101acb4acf7d945e69f0d6ce76a81113",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        set_bookmark(
            fb,
            repo,
            "7fe9947f101acb4acf7d945e69f0d6ce76a81113",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub struct UnsharedMergeUneven;

#[async_trait]
impl TestRepoFixture for UnsharedMergeUneven {
    const REPO_NAME: &'static str = "unshared_merge_uneven";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        // The code below was partially autogenerated using generate_new_fixtures.par
        let files = btreemap! {
            "side" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506441702 25200",
            "message"=> "One",
            "expected_hg_changeset"=> "9d374b7e8180f933e3043ad1ffab0a9f95e2bac6",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "side" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506441720 25200",
            "message"=> "Two",
            "expected_hg_changeset"=> "1700524113b1a3b1806560341009684b4378660b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "1700524113b1a3b1806560341009684b4378660b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442623 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "36ff88dd69c9966c9fad9d6d0457c52153039dde",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "36ff88dd69c9966c9fad9d6d0457c52153039dde",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "f61fdc0ddafd63503dcd8eed8994ec685bfc8941",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f61fdc0ddafd63503dcd8eed8994ec685bfc8941",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "0b94a2881dda90f0d64db5fae3ee5695a38e7c8f",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "0b94a2881dda90f0d64db5fae3ee5695a38e7c8f",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "2fa8b4ee6803a18db4649a3843a723ef1dfe852b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "2fa8b4ee6803a18db4649a3843a723ef1dfe852b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442624 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "03b0589d9788870817d03ce7b87516648ed5b33a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "1" => Some("1\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "9d374b7e8180f933e3043ad1ffab0a9f95e2bac6",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 1",
            "expected_hg_changeset"=> "3775a86c64cceeaf68ffe3f012fc90774c42002b",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "2" => Some("2\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "3775a86c64cceeaf68ffe3f012fc90774c42002b",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 2",
            "expected_hg_changeset"=> "eee492dcdeaae18f91822c4359dd516992e0dbcd",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "3" => Some("3\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "eee492dcdeaae18f91822c4359dd516992e0dbcd",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 3",
            "expected_hg_changeset"=> "163adc0d0f5d2eb0695ca123addcb92bab202096",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "4" => Some("4\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "163adc0d0f5d2eb0695ca123addcb92bab202096",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442628 25200",
            "message"=> "Add 4",
            "expected_hg_changeset"=> "f01e186c165a2fbe931fd1bf4454235398c591c9",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "5" => Some("5\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "f01e186c165a2fbe931fd1bf4454235398c591c9",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506442629 25200",
            "message"=> "Add 5",
            "expected_hg_changeset"=> "33fb49d8a47b29290f5163e30b294339c89505a2",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "6" => Some("6\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "33fb49d8a47b29290f5163e30b294339c89505a2",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443446 25200",
            "message"=> "Add 6",
            "expected_hg_changeset"=> "76096af83f52cc9a225ccfd8ddfb05ea18132343",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "7" => Some("7\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "76096af83f52cc9a225ccfd8ddfb05ea18132343",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443446 25200",
            "message"=> "Add 7",
            "expected_hg_changeset"=> "5a3e8d5a475ec07895e64ec1e1b2ec09bfa70e4e",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "8" => Some("8\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "5a3e8d5a475ec07895e64ec1e1b2ec09bfa70e4e",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443446 25200",
            "message"=> "Add 8",
            "expected_hg_changeset"=> "e819f2dd9a01d3e63d9a93e298968df275e6ad7c",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "9" => Some("9\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "e819f2dd9a01d3e63d9a93e298968df275e6ad7c",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443447 25200",
            "message"=> "Add 9",
            "expected_hg_changeset"=> "c1d5375bf73caab8725d759eaca56037c725c7d1",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "10" => Some("10\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "c1d5375bf73caab8725d759eaca56037c725c7d1",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443447 25200",
            "message"=> "Add 10",
            "expected_hg_changeset"=> "64011f64aaf9c2ad2e674f57c033987da4016f51",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {
            "10" => Some("10\n"),
            "6" => Some("6\n"),
            "7" => Some("7\n"),
            "8" => Some("8\n"),
            "9" => Some("9\n"),
            "side" => Some("Merge\n"),
        };
        let commit_metadata = btreemap! {
            "parents"=> "64011f64aaf9c2ad2e674f57c033987da4016f51 03b0589d9788870817d03ce7b87516648ed5b33a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443464 25200",
            "message"=> "Merge",
            "expected_hg_changeset"=> "9c6dd4e2c2f43c89613b094efb426cc42afdee2a",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        let files = btreemap! {};
        let commit_metadata = btreemap! {
            "parents"=> "9c6dd4e2c2f43c89613b094efb426cc42afdee2a",
            "author"=> "Simon Farnsworth <simonfar@fb.com>",
            "author_date"=> "1506443471 25200",
            "message"=> "And remove",
            "expected_hg_changeset"=> "dd993aab2bed7276e17c88470286ba8459ba6d94",
        };
        create_bonsai_changeset_from_test_data(fb, repo, files, commit_metadata).await;

        set_bookmark(
            fb,
            repo,
            "dd993aab2bed7276e17c88470286ba8459ba6d94",
            BookmarkKey::new("master").unwrap(),
        )
        .await;
    }
}

pub async fn save_diamond_commits(
    ctx: &CoreContext,
    repo: &impl Repo,
    parents: Vec<ChangesetId>,
) -> Result<ChangesetId, Error> {
    let first_bcs = create_bonsai_changeset(parents);
    let first_bcs_id = first_bcs.get_changeset_id();

    let second_bcs = create_bonsai_changeset(vec![first_bcs_id]);
    let second_bcs_id = second_bcs.get_changeset_id();

    let third_bcs =
        create_bonsai_changeset_with_author(vec![first_bcs_id], "another_author".to_string());
    let third_bcs_id = third_bcs.get_changeset_id();

    let fourth_bcs = create_bonsai_changeset(vec![second_bcs_id, third_bcs_id]);
    let fourth_bcs_id = fourth_bcs.get_changeset_id();

    changesets_creation::save_changesets(
        ctx,
        repo,
        vec![first_bcs, second_bcs, third_bcs, fourth_bcs],
    )
    .await
    .map(move |()| fourth_bcs_id)
}

pub fn create_bonsai_changeset(parents: Vec<ChangesetId>) -> BonsaiChangeset {
    BonsaiChangesetMut {
        parents,
        author: "author".to_string(),
        author_date: DateTime::from_timestamp(0, 0).unwrap(),
        committer: None,
        committer_date: None,
        message: "message".to_string(),
        hg_extra: Default::default(),
        git_extra_headers: None,
        git_tree_hash: None,
        file_changes: Default::default(),
        is_snapshot: false,
        git_annotated_tag: None,
    }
    .freeze()
    .unwrap()
}

pub fn create_bonsai_changeset_with_author(
    parents: Vec<ChangesetId>,
    author: String,
) -> BonsaiChangeset {
    BonsaiChangesetMut {
        parents,
        author,
        author_date: DateTime::from_timestamp(0, 0).unwrap(),
        committer: None,
        committer_date: None,
        message: "message".to_string(),
        hg_extra: Default::default(),
        git_extra_headers: None,
        git_tree_hash: None,
        file_changes: Default::default(),
        is_snapshot: false,
        git_annotated_tag: None,
    }
    .freeze()
    .unwrap()
}

pub fn create_bonsai_changeset_with_files(
    parents: Vec<ChangesetId>,
    file_changes: impl Into<SortedVectorMap<NonRootMPath, FileChange>>,
) -> BonsaiChangeset {
    BonsaiChangesetMut {
        parents,
        author: "author".to_string(),
        author_date: DateTime::from_timestamp(0, 0).unwrap(),
        committer: None,
        committer_date: None,
        message: "message".to_string(),
        hg_extra: Default::default(),
        git_extra_headers: None,
        git_tree_hash: None,
        file_changes: file_changes.into(),
        is_snapshot: false,
        git_annotated_tag: None,
    }
    .freeze()
    .unwrap()
}

pub struct ManyDiamonds;

#[async_trait]
impl TestRepoFixture for ManyDiamonds {
    const REPO_NAME: &'static str = "many_diamonds";

    async fn initrepo(fb: FacebookInit, repo: &impl Repo) {
        let ctx = CoreContext::test_mock(fb);

        let mut last_bcs_id = save_diamond_commits(&ctx, repo, vec![]).await.unwrap();

        let diamond_stack_size = 50u8;
        for _ in 1..diamond_stack_size {
            let new_bcs_id = save_diamond_commits(&ctx, repo, vec![last_bcs_id])
                .await
                .unwrap();
            last_bcs_id = new_bcs_id;
        }

        let ctx = CoreContext::test_mock(fb);
        let mut txn = repo.bookmarks().create_transaction(ctx.clone());
        txn.force_set(
            &BookmarkKey::new("master").unwrap(),
            last_bcs_id,
            BookmarkUpdateReason::TestMove,
        )
        .unwrap();
        txn.commit().await.unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[fbinit::test]
    async fn test_branch_even(fb: FacebookInit) {
        BranchEven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_branch_uneven(fb: FacebookInit) {
        BranchUneven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_branch_wide(fb: FacebookInit) {
        BranchWide::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_many_files_dirs(fb: FacebookInit) {
        ManyFilesDirs::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_linear(fb: FacebookInit) {
        Linear::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_merge_even(fb: FacebookInit) {
        MergeEven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_merge_uneven(fb: FacebookInit) {
        MergeUneven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_merge_multiple_files(fb: FacebookInit) {
        MergeMultipleFiles::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_unshared_merge_even(fb: FacebookInit) {
        UnsharedMergeEven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_unshared_merge_uneven(fb: FacebookInit) {
        UnsharedMergeUneven::getrepo(fb).await;
    }

    #[fbinit::test]
    async fn test_many_diamonds(fb: FacebookInit) {
        ManyDiamonds::getrepo(fb).await;
    }
}
