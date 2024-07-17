/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::format_err;
use anyhow::Error;
use bonsai_hg_mapping::BonsaiHgMappingRef;
use cloned::cloned;
use context::CoreContext;
use futures::try_join;
use megarepolib::common::create_save_and_generate_hg_changeset;
use megarepolib::common::ChangesetArgs;
use megarepolib::working_copy::get_colliding_paths_between_commits;
use mercurial_derivation::DeriveHgChangeset;
use mercurial_types::HgChangesetId;
use mononoke_types::ChangesetId;
use slog::info;

use crate::Repo;

async fn fail_on_path_conflicts(
    ctx: &CoreContext,
    repo: &Repo,
    hg_cs_id_1: HgChangesetId,
    hg_cs_id_2: HgChangesetId,
) -> Result<(), Error> {
    info!(ctx.logger(), "Checking if there are any path conflicts");
    let (bcs_1, bcs_2) = try_join!(
        repo.bonsai_hg_mapping().get_bonsai_from_hg(ctx, hg_cs_id_1),
        repo.bonsai_hg_mapping().get_bonsai_from_hg(ctx, hg_cs_id_2)
    )?;
    let collisions =
        get_colliding_paths_between_commits(ctx, repo, bcs_1.unwrap(), bcs_2.unwrap()).await?;
    if !collisions.is_empty() {
        Err(format_err!(
            "There are paths present in both parents: {:?} ...",
            collisions.iter().take(10).collect::<Vec<_>>(),
        ))
    } else {
        info!(ctx.logger(), "Done checking path conflicts");
        Ok(())
    }
}

pub async fn perform_merge(
    ctx: CoreContext,
    repo: Repo,
    first_bcs_id: ChangesetId,
    second_bcs_id: ChangesetId,
    resulting_changeset_args: ChangesetArgs,
) -> Result<HgChangesetId, Error> {
    cloned!(ctx, repo);
    let (first_hg_cs_id, second_hg_cs_id) = try_join!(
        repo.derive_hg_changeset(&ctx, first_bcs_id.clone()),
        repo.derive_hg_changeset(&ctx, second_bcs_id.clone()),
    )?;
    fail_on_path_conflicts(&ctx, &repo, first_hg_cs_id, second_hg_cs_id).await?;
    info!(
        ctx.logger(),
        "Creating a merge bonsai changeset with parents: {:?}, {:?}", &first_bcs_id, &second_bcs_id
    );
    create_save_and_generate_hg_changeset(
        &ctx,
        &repo,
        vec![first_bcs_id, second_bcs_id],
        Default::default(),
        resulting_changeset_args,
    )
    .await
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use fbinit::FacebookInit;
    use fixtures::MergeEven;
    use fixtures::TestRepoFixture;

    use super::*;

    #[fbinit::test]
    async fn test_path_conflict_detection(fb: FacebookInit) {
        let repo = MergeEven::get_custom_test_repo(fb).await;
        let ctx = CoreContext::test_mock(fb);
        let p1 = HgChangesetId::from_str("4f7f3fd428bec1a48f9314414b063c706d9c1aed").unwrap();
        let p2 = HgChangesetId::from_str("16839021e338500b3cf7c9b871c8a07351697d68").unwrap();
        assert!(
            fail_on_path_conflicts(&ctx, &repo, p1, p2).await.is_err(),
            "path conflicts should've been detected"
        );
    }
}
