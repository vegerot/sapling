/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::io::Cursor;

use anyhow::Context;
use anyhow::Result;
use blobrepo_hg::BlobRepoHg;
use bookmarks::BookmarkKey;
use bytes::Bytes;
use bytes::BytesMut;
use context::CoreContext;
use futures::future::try_join;
use futures::TryStreamExt;
use futures_stats::TimedTryFutureExt;
use getbundle_response::create_getbundle_response;
use getbundle_response::PhasesPart;
use getbundle_response::SessionLfsParams;
use mercurial_bundles::create_bundle_stream_new;
use mercurial_bundles::parts;
use mercurial_bundles::Bundle2EncodeBuilder;
use mercurial_bundles::PartId;
use mercurial_derivation::DeriveHgChangeset;
use metaconfig_types::PushrebaseParams;
use mononoke_types::ChangesetId;
use scuba_ext::FutureStatsScubaExt;

use crate::CommonHeads;
use crate::Repo;

/// Data, needed to generate a `Push` response
pub struct UnbundlePushResponse {
    pub changegroup_id: Option<PartId>,
    pub bookmark_ids: Vec<PartId>,
}

/// Data, needed to generate an `InfinitePush` response
pub struct UnbundleInfinitePushResponse {
    pub changegroup_id: Option<PartId>,
}

/// Data, needed to generate a `PushRebase` response
pub struct UnbundlePushRebaseResponse {
    pub commonheads: CommonHeads,
    pub pushrebased_rev: ChangesetId,
    pub pushrebased_changesets: Vec<pushrebase::PushrebaseChangesetPair>,
    pub onto: BookmarkKey,
    pub bookmark_push_part_id: Option<PartId>,
}

/// Data, needed to generate a bookmark-only `PushRebase` response
pub struct UnbundleBookmarkOnlyPushRebaseResponse {
    pub bookmark_push_part_id: PartId,
}

pub enum UnbundleResponse {
    Push(UnbundlePushResponse),
    InfinitePush(UnbundleInfinitePushResponse),
    PushRebase(UnbundlePushRebaseResponse),
    BookmarkOnlyPushRebase(UnbundleBookmarkOnlyPushRebaseResponse),
}

impl UnbundleResponse {
    fn get_bundle_builder() -> Bundle2EncodeBuilder<Cursor<Vec<u8>>> {
        Bundle2EncodeBuilder::new(Cursor::new(Vec::new()))
    }

    async fn generate_push_or_infinitepush_response(
        changegroup_id: Option<PartId>,
        bookmark_ids: Vec<PartId>,
    ) -> Result<Bytes> {
        let mut bundle = Self::get_bundle_builder();
        if let Some(changegroup_id) = changegroup_id {
            bundle.add_part(parts::replychangegroup_part(
                parts::ChangegroupApplyResult::Success { heads_num_diff: 0 },
                changegroup_id,
            )?);
        }
        for part_id in bookmark_ids {
            bundle.add_part(parts::replypushkey_part(true, part_id)?);
        }
        let cursor = bundle.build().await?;
        Ok(Bytes::from(cursor.into_inner()))
    }

    async fn generate_push_response_bytes(
        _ctx: &CoreContext,
        data: UnbundlePushResponse,
    ) -> Result<Bytes> {
        let UnbundlePushResponse {
            changegroup_id,
            bookmark_ids,
        } = data;
        Self::generate_push_or_infinitepush_response(changegroup_id, bookmark_ids)
            .await
            .context("While preparing push response")
    }

    async fn generate_inifinitepush_response_bytes(
        _ctx: &CoreContext,
        data: UnbundleInfinitePushResponse,
    ) -> Result<Bytes> {
        let UnbundleInfinitePushResponse { changegroup_id } = data;
        Self::generate_push_or_infinitepush_response(changegroup_id, vec![])
            .await
            .context("While preparing infinitepush response")
    }

    async fn generate_pushrebase_response_bytes(
        ctx: &CoreContext,
        data: UnbundlePushRebaseResponse,
        repo: &impl Repo,
        pushrebase_params: PushrebaseParams,
        lfs_params: &SessionLfsParams,
    ) -> Result<Bytes> {
        let UnbundlePushRebaseResponse {
            commonheads,
            pushrebased_rev,
            pushrebased_changesets,
            onto,
            bookmark_push_part_id,
        } = data;

        // Send to the client both pushrebased commit and current "onto" bookmark. Normally they
        // should be the same, however they might be different if bookmark
        // suddenly moved before current pushrebase finished.
        let common = commonheads.heads;
        let maybe_onto_head = repo.get_bookmark_hg(ctx.clone(), &onto);
        let pushrebased_hg_rev = repo.derive_hg_changeset(ctx, pushrebased_rev);

        let bookmark_reply_part = match bookmark_push_part_id {
            Some(part_id) => Some(parts::replypushkey_part(true, part_id)?),
            None => None,
        };

        let obsmarkers_part = match pushrebase_params.emit_obsmarkers {
            true => obsolete::pushrebased_changesets_to_obsmarkers_part(
                ctx,
                repo,
                pushrebased_changesets,
            )
            .transpose()?,
            false => None,
        };

        let scuba_logger = ctx.scuba().clone();
        let response_bytes = async move {
            let (maybe_onto_head, pushrebased_hg_rev) =
                try_join(maybe_onto_head, pushrebased_hg_rev).await?;

            let mut heads = vec![];
            if let Some(onto_head) = maybe_onto_head {
                heads.push(onto_head);
            }
            heads.push(pushrebased_hg_rev);
            let mut cg_part_builder =
                create_getbundle_response(ctx, repo, common, &heads, PhasesPart::Yes, lfs_params)
                    .await?;

            cg_part_builder.extend(bookmark_reply_part.into_iter());
            cg_part_builder.extend(obsmarkers_part.into_iter());
            let chunks = create_bundle_stream_new(cg_part_builder)
                .try_collect::<Vec<_>>()
                .await?;

            let mut total_capacity = 0;
            for c in chunks.iter() {
                total_capacity += c.len();
            }

            let mut res = BytesMut::with_capacity(total_capacity);
            for c in chunks {
                res.extend_from_slice(&c);
            }
            Result::<_>::Ok(res.freeze())
        }
        .try_timed()
        .await
        .context("While preparing pushrebase response")?
        .log_future_stats(scuba_logger, "Pushrebase: prepared the response", None);
        Ok(response_bytes)
    }

    async fn generate_bookmark_only_pushrebase_response_bytes(
        _ctx: &CoreContext,
        data: UnbundleBookmarkOnlyPushRebaseResponse,
    ) -> Result<Bytes> {
        let UnbundleBookmarkOnlyPushRebaseResponse {
            bookmark_push_part_id,
        } = data;

        let mut bundle = Self::get_bundle_builder();
        bundle.add_part(parts::replypushkey_part(true, bookmark_push_part_id)?);
        let cursor = bundle
            .build()
            .await
            .context("While preparing bookmark-only pushrebase response")?;

        Ok(Bytes::from(cursor.into_inner()))
    }

    /// Produce bundle2 response parts for the completed `unbundle` processing
    pub async fn generate_bytes(
        self,
        ctx: &CoreContext,
        repo: &impl Repo,
        pushrebase_params: PushrebaseParams,
        lfs_params: &SessionLfsParams,
        respondlightly: Option<bool>,
    ) -> Result<Bytes> {
        if let Some(true) = respondlightly {
            let bundle = Self::get_bundle_builder();
            let cursor = bundle.build().await?;
            return Ok(Bytes::from(cursor.into_inner()));
        }
        match self {
            UnbundleResponse::Push(data) => Self::generate_push_response_bytes(ctx, data).await,
            UnbundleResponse::InfinitePush(data) => {
                Self::generate_inifinitepush_response_bytes(ctx, data).await
            }
            UnbundleResponse::PushRebase(data) => {
                Self::generate_pushrebase_response_bytes(
                    ctx,
                    data,
                    repo,
                    pushrebase_params,
                    lfs_params,
                )
                .await
            }
            UnbundleResponse::BookmarkOnlyPushRebase(data) => {
                Self::generate_bookmark_only_pushrebase_response_bytes(ctx, data).await
            }
        }
    }
}
