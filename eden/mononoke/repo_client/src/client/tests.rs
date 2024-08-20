/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![cfg(test)]
use std::pin::pin;

use blobstore::Loadable;
use fbinit::FacebookInit;
use fixtures::ManyFilesDirs;
use fixtures::TestRepoFixture;
use futures::compat::Future01CompatExt;
use justknobs::test_helpers::with_just_knobs_async;
use justknobs::test_helpers::JustKnobsInMemory;
use justknobs::test_helpers::KnobVal;
use manifest::Entry;
use manifest::ManifestOps;
use maplit::hashset;
use mercurial_derivation::DeriveHgChangeset;
use metaconfig_types::LfsParams;
use mononoke_types_mocks::changesetid::ONES_CSID;
use repo_blobstore::RepoBlobstoreRef;
use scuba_ext::MononokeScubaSampleBuilder;
use serde_json::json;
use tests_utils::CreateCommitContext;

use super::*;
use crate::repo::RepoClientRepo;

#[test]
fn test_parsing_caps_simple() {
    assert_eq!(
        parse_utf8_getbundle_caps(b"cap"),
        Some((String::from("cap"), HashMap::new())),
    );

    let caps = b"bundle2=HG20";

    assert_eq!(
        parse_utf8_getbundle_caps(caps),
        Some((
            String::from("bundle2"),
            hashmap! { "HG20".to_string() => hashset!{} }
        )),
    );

    let caps = b"bundle2=HG20%0Ab2x%253Ainfinitepush%0Ab2x%253Ainfinitepushscratchbookmarks\
        %0Ab2x%253Arebase%0Abookmarks%0Achangegroup%3D01%2C02%0Adigests%3Dmd5%2Csha1%2Csha512%0A\
        error%3Dabort%2Cunsupportedcontent%2Cpushraced%2Cpushkey%0Ahgtagsfnodes%0Alistkeys%0A\
        pushkey%0Aremote-changegroup%3Dhttp%2Chttps%0Aremotefilelog%3DTrue%0Atreemanifest%3DTrue%0Atreeonly%3DTrue";

    assert_eq!(
        parse_utf8_getbundle_caps(caps),
        Some((
            String::from("bundle2"),
            hashmap! {
                "HG20".to_string() => hashset!{},
                "b2x:rebase".to_string() => hashset!{},
                "digests".to_string() => hashset!{"md5".to_string(), "sha512".to_string(), "sha1".to_string()},
                "listkeys".to_string() => hashset!{},
                "remotefilelog".to_string() => hashset!{"True".to_string()},
                "hgtagsfnodes".to_string() => hashset!{},
                "bookmarks".to_string() => hashset!{},
                "b2x:infinitepushscratchbookmarks".to_string() => hashset!{},
                "treeonly".to_string() => hashset!{"True".to_string()},
                "pushkey".to_string() => hashset!{},
                "error".to_string() => hashset!{
                    "pushraced".to_string(),
                    "pushkey".to_string(),
                    "unsupportedcontent".to_string(),
                    "abort".to_string(),
                },
                "b2x:infinitepush".to_string() => hashset!{},
                "changegroup".to_string() => hashset!{"01".to_string(), "02".to_string()},
                "remote-changegroup".to_string() => hashset!{"http".to_string(), "https".to_string()},
                "treemanifest".to_string() => hashset!{"True".to_string()},
            }
        )),
    );
}

#[fbinit::test]
fn get_changed_manifests_stream_test(fb: FacebookInit) -> Result<(), Error> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(get_changed_manifests_stream_test_impl(fb))
}

async fn get_changed_manifests_stream_test_impl(fb: FacebookInit) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: RepoClientRepo = ManyFilesDirs::get_repo(fb).await;

    // Commit that has only dir2 directory
    let root_mf_id = HgChangesetId::from_str("051946ed218061e925fb120dac02634f9ad40ae2")?
        .load(&ctx, &repo.repo_blobstore().clone())
        .await?
        .manifestid();

    let fetched_mfs = fetch_mfs(
        &ctx,
        &repo,
        root_mf_id,
        HgManifestId::new(NULL_HASH),
        MPath::ROOT,
        65536,
    )
    .await?;

    let mut res = fetched_mfs
        .into_iter()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    res.sort();
    let mut expected = vec![MPath::ROOT, MPath::new("dir2")?];
    expected.sort();
    assert_eq!(res, expected);

    // Now commit that added a few files and directories

    let root_mf_id = HgChangesetId::from_str("d261bc7900818dea7c86935b3fb17a33b2e3a6b4")?
        .load(&ctx, &repo.repo_blobstore().clone())
        .await?
        .manifestid();

    let base_root_mf_id = HgChangesetId::from_str("2f866e7e549760934e31bf0420a873f65100ad63")?
        .load(&ctx, &repo.repo_blobstore().clone())
        .await?
        .manifestid();

    let fetched_mfs =
        fetch_mfs(&ctx, &repo, root_mf_id, base_root_mf_id, MPath::ROOT, 65536).await?;

    let mut res = fetched_mfs
        .into_iter()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();
    res.sort();
    let mut expected = vec![
        MPath::ROOT,
        MPath::new("dir1")?,
        MPath::new("dir1/subdir1")?,
        MPath::new("dir1/subdir1/subsubdir1")?,
        MPath::new("dir1/subdir1/subsubdir2")?,
    ];
    expected.sort();
    assert_eq!(res, expected);

    Ok(())
}

#[fbinit::test]
fn get_changed_manifests_stream_test_depth(fb: FacebookInit) -> Result<(), Error> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(get_changed_manifests_stream_test_depth_impl(fb))
}

async fn get_changed_manifests_stream_test_depth_impl(fb: FacebookInit) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: RepoClientRepo = ManyFilesDirs::get_repo(fb).await;

    let root_mf_id = HgChangesetId::from_str("d261bc7900818dea7c86935b3fb17a33b2e3a6b4")?
        .load(&ctx, &repo.repo_blobstore().clone())
        .await?
        .manifestid();

    let base_mf_id = HgManifestId::new(NULL_HASH);
    let fetched_mfs = fetch_mfs(&ctx, &repo, root_mf_id, base_mf_id, MPath::ROOT, 65536).await?;

    let paths = fetched_mfs
        .into_iter()
        .map(|(_, path)| path)
        .collect::<Vec<_>>();

    let max_depth = paths
        .iter()
        .map(|path| path.num_components())
        .max()
        .unwrap();

    for depth in 0..max_depth + 1 {
        println!("depth: {}", depth);
        let fetched_mfs =
            fetch_mfs(&ctx, &repo, root_mf_id, base_mf_id, MPath::ROOT, depth).await?;
        let mut actual = fetched_mfs
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>();
        actual.sort();
        let iter = paths.clone().into_iter();
        // We have a weird hard-coded behaviour for depth=1 that we are preserving for now
        let mut expected: Vec<_> = if depth == 1 {
            let expected: Vec<_> = iter.filter(|path| path.is_root()).collect();
            assert_eq!(expected.len(), 1);
            expected
        } else {
            iter.filter(|path| path.num_components() <= depth).collect()
        };
        expected.sort();
        assert_eq!(actual, expected);
    }

    Ok(())
}

#[fbinit::test]
fn get_changed_manifests_stream_test_base_path(fb: FacebookInit) -> Result<(), Error> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(get_changed_manifests_stream_test_base_path_impl(fb))
}

async fn get_changed_manifests_stream_test_base_path_impl(fb: FacebookInit) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: RepoClientRepo = ManyFilesDirs::get_repo(fb).await;

    let root_mf_id = HgChangesetId::from_str("d261bc7900818dea7c86935b3fb17a33b2e3a6b4")?
        .load(&ctx, &repo.repo_blobstore().clone())
        .await?
        .manifestid();

    let base_mf_id = HgManifestId::new(NULL_HASH);
    let fetched_mfs = fetch_mfs(&ctx, &repo, root_mf_id, base_mf_id, MPath::ROOT, 65536).await?;

    for (hash, path) in &fetched_mfs {
        println!("base path: {:?}", path);
        let mut actual = fetch_mfs(&ctx, &repo, *hash, base_mf_id, path.clone(), 65536).await?;
        actual.sort();

        let mut expected: Vec<_> = fetched_mfs
            .clone()
            .into_iter()
            .filter(|(_, curpath)| {
                if path.is_root() {
                    true
                } else {
                    path.is_prefix_of(curpath.as_ref())
                }
            })
            .collect();
        expected.sort();
        assert_eq!(actual, expected);
    }

    Ok(())
}

#[fbinit::test]
async fn test_lfs_rollout(fb: FacebookInit) -> Result<(), Error> {
    with_just_knobs_async(
        JustKnobsInMemory::new(hashmap! {
            "scm/mononoke_timeouts:repo_client_getpack_timeout_secs".to_string() => KnobVal::Int(18000),
        }),
        pin!{

            async move {

                let ctx = CoreContext::test_mock(fb);

                assert!(!run_and_check_if_lfs(&ctx, LfsParams::default()).await?);

                // Rollout percentage is 100 and threshold is set - enable lfs
                let lfs_params = LfsParams {
                    threshold: Some(5),
                    rollout_percentage: 100,
                    ..Default::default()
                };
                assert!(run_and_check_if_lfs(&ctx, lfs_params).await?);

                // Rollout percentage is 0 - no lfs is enabled
                let lfs_params = LfsParams {
                    threshold: Some(5),
                    rollout_percentage: 0,
                    ..Default::default()
                };
                assert!(!run_and_check_if_lfs(&ctx, lfs_params).await?);

                // Rollout percentage is 100, but threshold is too high
                let lfs_params = LfsParams {
                    threshold: Some(500),
                    rollout_percentage: 100,
                    ..Default::default()
                };
                assert!(!run_and_check_if_lfs(&ctx, lfs_params).await?);
                Ok(())
        }
    },
    ).await
}

#[fbinit::test]
async fn test_maybe_validate_pushed_bonsais(fb: FacebookInit) -> Result<(), Error> {
    let ctx = CoreContext::test_mock(fb);
    let repo: RepoClientRepo = test_repo_factory::build_empty(ctx.fb).await?;
    let commit = CreateCommitContext::new_root(&ctx, &repo)
        .add_file("largefile", "11111_11111")
        .commit()
        .await?;

    let hg_cs_id = repo.derive_hg_changeset(&ctx, commit).await?;

    // No replay data - ignore
    maybe_validate_pushed_bonsais(&ctx, &repo, &None).await?;

    // Has replay data, but no hgbonsaimapping - ignore
    maybe_validate_pushed_bonsais(&ctx, &repo, &Some("{}".to_string())).await?;

    // Has valid replay data - should succeed
    maybe_validate_pushed_bonsais(
        &ctx,
        &repo,
        &Some(
            json!({
                "hgbonsaimapping": {
                    format!("{}", hg_cs_id): commit,
                }
            })
            .to_string(),
        ),
    )
    .await?;

    // Additional fields doesn't change the result
    maybe_validate_pushed_bonsais(
        &ctx,
        &repo,
        &Some(
            json!({
                "hgbonsaimapping": {
                    format!("{}", hg_cs_id): commit,
                },
                "somefield": "somevalue"
            })
            .to_string(),
        ),
    )
    .await?;

    // Now invalid bonsai - should fail
    assert!(
        maybe_validate_pushed_bonsais(
            &ctx,
            &repo,
            &Some(
                json!({
                    "hgbonsaimapping": {
                        format!("{}", hg_cs_id): ONES_CSID,
                    },
                    "somefield": "somevalue"
                })
                .to_string(),
            ),
        )
        .await
        .is_err()
    );

    // Now invalid hgbonsaimapping field - should fail
    assert!(
        maybe_validate_pushed_bonsais(
            &ctx,
            &repo,
            &Some(
                json!({
                    "hgbonsaimapping": "somevalue"
                })
                .to_string(),
            ),
        )
        .await
        .is_err()
    );
    Ok(())
}

async fn run_and_check_if_lfs(ctx: &CoreContext, lfs_params: LfsParams) -> Result<bool, Error> {
    let repo: Arc<RepoClientRepo> = Arc::new(
        test_repo_factory::TestRepoFactory::new(ctx.fb)?
            .with_config_override(|config| config.lfs = lfs_params)
            .build()
            .await?,
    );
    let commit = CreateCommitContext::new_root(ctx, &repo)
        .add_file("largefile", "11111_11111")
        .commit()
        .await?;

    let hg_cs_id = repo.derive_hg_changeset(ctx, commit).await?;

    let hg_cs = hg_cs_id.load(ctx, &repo.repo_blobstore().clone()).await?;

    let path = NonRootMPath::new("largefile")?;
    let maybe_entry = hg_cs
        .manifestid()
        .find_entry(
            ctx.clone(),
            repo.repo_blobstore().clone(),
            path.clone().into(),
        )
        .await?
        .unwrap();

    let filenode_id = match maybe_entry {
        Entry::Leaf((_, filenode_id)) => filenode_id,
        Entry::Tree(_) => {
            panic!("should be a leaf");
        }
    };

    let logging = LoggingContainer::new(
        ctx.fb,
        ctx.logger().clone(),
        MononokeScubaSampleBuilder::with_discard(),
    );

    let repo_client = RepoClient::new(
        repo,
        ctx.session().clone(),
        logging,
        None, // No PushRedirectorArgs
        Default::default(),
        None, // No backup repo source
    );

    let bytes = repo_client
        .getpackv2(
            futures::stream::iter(vec![(path.clone(), vec![filenode_id])])
                .map(anyhow::Ok)
                .boxed(),
        )
        .try_fold(BytesMut::new(), |mut buf, b| async move {
            buf.extend_from_slice(&b);
            anyhow::Ok(buf)
        })
        .await?;

    let lfs_url: &[u8] = b"version https://git-lfs.github.com/spec/v1";

    let found = bytes.windows(lfs_url.len()).any(|w| w == lfs_url);

    Ok(found)
}

async fn fetch_mfs(
    ctx: &CoreContext,
    repo: &RepoClientRepo,
    root_mf_id: HgManifestId,
    base_root_mf_id: HgManifestId,
    base_path: MPath,
    depth: usize,
) -> Result<Vec<(HgManifestId, MPath)>, Error> {
    let fetched_mfs = get_changed_manifests_stream(
        ctx.clone(),
        repo,
        root_mf_id,
        base_root_mf_id,
        base_path,
        depth,
    )
    .collect()
    .compat()
    .await?;

    // Make sure that Manifest ids are present in the repo
    for (hash, _) in &fetched_mfs {
        hash.load(ctx, repo.repo_blobstore()).await?;
    }
    Ok(fetched_mfs)
}

#[test]
fn test_debug_format_directories() {
    assert_eq!(&debug_format_directories(vec![&"foo"]), "foo,");
    assert_eq!(&debug_format_directories(vec![&"foo,bar"]), "foo:obar,");
    assert_eq!(&debug_format_directories(vec![&"foo", &"bar"]), "foo,bar,");
}

#[test]
fn test_parse_git_lookup() -> Result<(), Error> {
    assert!(parse_git_lookup("ololo").is_none());
    assert!(parse_git_lookup("_gitlookup_hg_badhash").is_none());
    assert!(parse_git_lookup("_gitlookup_git_badhash").is_none());
    assert_eq!(
        parse_git_lookup("_gitlookup_hg_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Some(GitLookup::HgToGit(HgChangesetId::from_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )?))
    );

    assert_eq!(
        parse_git_lookup("_gitlookup_git_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Some(GitLookup::GitToHg(GitSha1::from_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )?))
    );

    Ok(())
}
