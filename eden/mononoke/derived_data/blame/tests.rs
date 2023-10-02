/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use anyhow::anyhow;
use anyhow::Error;
use bonsai_hg_mapping::BonsaiHgMapping;
use bookmarks::Bookmarks;
use borrowed::borrowed;
use changeset_fetcher::ChangesetFetcher;
use changesets::Changesets;
use context::CoreContext;
use fbinit::FacebookInit;
use filestore::FilestoreConfig;
use maplit::btreemap;
use maplit::hashmap;
use metaconfig_types::BlameVersion;
use mononoke_types::blame_v2::BlameRejected;
use mononoke_types::blame_v2::BlameV2;
use mononoke_types::ChangesetId;
use mononoke_types::NonRootMPath;
use repo_blobstore::RepoBlobstore;
use repo_derived_data::RepoDerivedData;
use test_repo_factory::TestRepoFactory;
use tests_utils::create_commit;
use tests_utils::store_files;
use tests_utils::store_rename;
use tests_utils::CreateCommitContext;

use crate::fetch_blame_v2;

#[facet::container]
struct TestRepo {
    #[facet]
    bonsai_hg_mapping: dyn BonsaiHgMapping,
    #[facet]
    bookmarks: dyn Bookmarks,
    #[facet]
    repo_blobstore: RepoBlobstore,
    #[facet]
    repo_derived_data: RepoDerivedData,
    #[facet]
    filestore_config: FilestoreConfig,
    #[facet]
    changeset_fetcher: dyn ChangesetFetcher,
    #[facet]
    changesets: dyn Changesets,
}

// File with multiple changes and a merge
const F0: &[&str] = &[
    // c0
    r#"|
1 0
1 1
"#,
    // c1
    r#"|
2 0
1 0
2 1
"#,
    // c2
    r#"|
2 0
1 0
3 0
3 1
2 1
3 2
"#,
    // c3
    r#"|
1 0
1 1
3 2
4 0
"#,
    // c4
    r#"|
2 0
1 0
3 0
3 1
2 1
3 2
4 0
"#,
];

const F0_AT_C4: &str = r#"c0: |
c1: 2 0
c0: 1 0
c2: 3 0
c2: 3 1
c1: 2 1
c2: 3 2
c3: 4 0
"#;

// file with multiple change only in one parent and a merge
const F1: &[&str] = &[
    // c0
    r#"|
1 0
1 1
"#,
    // c3
    r#"|
1 0
4 0
1 1
"#,
];

const F1_AT_C4: &str = r#"c0: |
c0: 1 0
c3: 4 0
c0: 1 1
"#;

// renamed file
const F2: &[&str] = &[
    // c0 as _f2
    r#"|
1 0
1 1
"#,
    // c1 as _f2 => f2
    r#"|
1 0
2 0
1 1
"#,
    // c3 as new f2
    r#"|
1 0
4 0
1 1
"#,
    // c4 as f2
    r#"|
5 0
1 0
2 0
4 0
1 1
"#,
];

const F2_AT_C4: &str = r#"c0: |
c4: 5 0
c0: 1 0
c1: 2 0
c3: 4 0
c0: 1 1
"#;

#[fbinit::test]
async fn test_blame_v2(fb: FacebookInit) -> Result<(), Error> {
    test_blame_version(fb, BlameVersion::V2).await
}

async fn test_blame_version(fb: FacebookInit, version: BlameVersion) -> Result<(), Error> {
    // Commits structure
    //
    //   0
    //  / \
    // 1   3
    // |   |
    // 2   |
    //  \ /
    //   4
    //
    let ctx = CoreContext::test_mock(fb);
    let repo: TestRepo = TestRepoFactory::new(fb)?
        .with_config_override(|config| {
            config
                .derived_data_config
                .get_active_config()
                .expect("No enabled derived data types config")
                .blame_version = version
        })
        .build()
        .await?;
    borrowed!(ctx, repo);

    let c0 = create_commit(
        ctx.clone(),
        repo,
        vec![],
        store_files(
            ctx,
            btreemap! {
                "f0" => Some(F0[0]),
                "f1" => Some(F1[0]),
                "_f2" => Some(F2[0]),
            },
            repo,
        )
        .await,
    )
    .await;

    let mut c1_changes = store_files(ctx, btreemap! {"f0" => Some(F0[1])}, repo).await;
    let (f2_path, f2_change) =
        store_rename(ctx, (NonRootMPath::new("_f2")?, c0), "f2", F2[1], repo).await;
    c1_changes.insert(f2_path, f2_change);
    let c1 = create_commit(ctx.clone(), repo, vec![c0], c1_changes).await;

    let c2 = create_commit(
        ctx.clone(),
        repo,
        vec![c1],
        store_files(ctx, btreemap! {"f0" => Some(F0[2])}, repo).await,
    )
    .await;

    let c3 = create_commit(
        ctx.clone(),
        repo,
        vec![c0],
        store_files(
            ctx,
            btreemap! {
                "f0" => Some(F0[3]),
                "f1" => Some(F1[1]),
                "f2" => Some(F2[2]),
            },
            repo,
        )
        .await,
    )
    .await;

    let c4 = create_commit(
        ctx.clone(),
        repo,
        vec![c2, c3],
        store_files(
            ctx,
            btreemap! {
                "f0" => Some(F0[4]),
                "f1" => Some(F1[1]), // did not change after c3
                "f2" => Some(F2[3]),
            },
            repo,
        )
        .await,
    )
    .await;

    let names = hashmap! {
        c0 => "c0",
        c1 => "c1",
        c2 => "c2",
        c3 => "c3",
        c4 => "c4",
    };

    let (blame, _) = fetch_blame_v2(ctx, repo, c4, NonRootMPath::new("f0")?).await?;
    assert_eq!(annotate(F0[4], blame, &names)?, F0_AT_C4);

    let (blame, _) = fetch_blame_v2(ctx, repo, c4, NonRootMPath::new("f1")?).await?;
    assert_eq!(annotate(F1[1], blame, &names)?, F1_AT_C4);

    let (blame, _) = fetch_blame_v2(ctx, repo, c4, NonRootMPath::new("f2")?).await?;
    assert_eq!(annotate(F2[3], blame, &names)?, F2_AT_C4);

    Ok(())
}

#[fbinit::test]
async fn test_blame_size_rejected_v2(fb: FacebookInit) -> Result<(), Error> {
    test_blame_size_rejected_version(fb, BlameVersion::V2).await
}

async fn test_blame_size_rejected_version(
    fb: FacebookInit,
    version: BlameVersion,
) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: TestRepo = test_repo_factory::build_empty(fb).await.unwrap();
    borrowed!(ctx, repo);
    let file1 = "file1";
    let content = "content";
    let c1 = CreateCommitContext::new_root(ctx, &repo)
        .add_file(file1, content)
        .commit()
        .await?;

    // Default file size is 10MiB, so blame should be computed
    // without problems.
    let (blame, _) = fetch_blame_v2(ctx, repo, c1, NonRootMPath::new(file1)?).await?;
    let _ = blame.ranges()?;

    let repo: TestRepo = TestRepoFactory::new(fb)?
        .with_config_override(|config| {
            config
                .derived_data_config
                .get_active_config()
                .expect("No enabled derived data types config")
                .blame_version = version;
            config
                .derived_data_config
                .get_active_config()
                .expect("No enabled derived data types config")
                .blame_filesize_limit = Some(4);
        })
        .build()
        .await?;

    let file2 = "file2";
    let c2 = CreateCommitContext::new_root(ctx, &repo)
        .add_file(file2, content)
        .commit()
        .await?;

    // This repo has a decreased limit, so derivation should fail now
    let (blame, _) = fetch_blame_v2(ctx, &repo, c2, NonRootMPath::new(file2)?).await?;

    match blame.ranges() {
        Err(BlameRejected::TooBig) => {}
        _ => {
            return Err(anyhow!("unexpected result"));
        }
    }

    Ok(())
}

#[fbinit::test]
async fn test_blame_copy_source(fb: FacebookInit) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: TestRepo = TestRepoFactory::new(fb)?
        .with_config_override(|config| {
            config
                .derived_data_config
                .get_active_config()
                .expect("No enabled derived data types config")
                .blame_version = BlameVersion::V2
        })
        .build()
        .await?;
    borrowed!(ctx, repo);

    let c1 = CreateCommitContext::new_root(ctx, &repo)
        .add_file("file1", "one\ntwo\nthree\n")
        .add_file("file2", "zero\none\ntwo\nfour\n")
        .commit()
        .await?;

    let data = "none\none\ntwo\nthree\n";
    let c2 = CreateCommitContext::new(ctx, &repo, vec![c1])
        .add_file_with_copy_info("file1", data, (c1, "file2"))
        .commit()
        .await?;

    let (blame, _) = fetch_blame_v2(ctx, repo, c2, NonRootMPath::new("file1")?).await?;
    let lines = blame
        .lines()?
        .map(|line| (line.changeset_id, line.path.to_string(), line.origin_offset))
        .collect::<Vec<_>>();

    // The "one" and "two" lines are blamed to the copy source, and not the
    // parent.  The "three" line blames to the commit that performed the copy,
    // and not the parent.
    assert_eq!(
        lines,
        vec![
            (&c2, "file1".to_string(), 0),
            (&c1, "file2".to_string(), 1),
            (&c1, "file2".to_string(), 2),
            (&c2, "file1".to_string(), 3),
        ]
    );
    Ok(())
}

fn annotate(
    content: &str,
    blame: BlameV2,
    names: &HashMap<ChangesetId, &'static str>,
) -> Result<String, Error> {
    let mut result = String::new();
    let mut ranges = blame.ranges()?;
    let mut range = ranges
        .next()
        .ok_or_else(|| Error::msg("empty blame for non empty content"))?;
    for (index, line) in content.lines().enumerate() {
        if index as u32 >= range.offset + range.length {
            range = ranges
                .next()
                .ok_or_else(|| Error::msg("not enough ranges in a blame"))?;
        }
        let name = names
            .get(&range.csid)
            .ok_or_else(|| Error::msg("unresolved csid"))?;
        result.push_str(name);
        result.push_str(": ");
        result.push_str(line);
        result.push('\n');
    }
    Ok(result)
}
