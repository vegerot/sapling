/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use anyhow::anyhow;
use bytes::Bytes;
use edenapi_types::wire::pull::WirePullFastForwardRequest;
use edenapi_types::wire::pull::WirePullLazyRequest;
use edenapi_types::wire::ToWire;
use edenapi_types::wire::WireCloneData;
use edenapi_types::wire::WireIdMapEntry;
use gotham::state::FromState;
use gotham::state::State;
use gotham_derive::StateData;
use gotham_derive::StaticResponseExtender;
use gotham_ext::error::HttpError;
use gotham_ext::middleware::request_context::RequestContext;
use gotham_ext::response::BytesBody;
use mercurial_types::HgChangesetId;
use serde::Deserialize;
use types::HgId;

use crate::context::ServerContext;
use crate::errors::MononokeErrorExt;
use crate::handlers::HandlerInfo;
use crate::handlers::SaplingRemoteApiMethod;
use crate::middleware::request_dumper::RequestDumper;
use crate::utils::cbor;
use crate::utils::get_repo;
use crate::utils::parse_wire_request;

#[derive(Debug, Deserialize, StateData, StaticResponseExtender)]
pub struct PullFastForwardParams {
    repo: String,
}

#[derive(Debug, Deserialize, StateData, StaticResponseExtender)]
pub struct PullLazyParams {
    repo: String,
}

pub async fn pull_lazy(state: &mut State) -> Result<BytesBody<Bytes>, HttpError> {
    let params = PullLazyParams::take_from(state);

    state.put(HandlerInfo::new(
        &params.repo,
        SaplingRemoteApiMethod::PullLazy,
    ));
    let request = parse_wire_request::<WirePullLazyRequest>(state).await?;
    if let Some(rd) = RequestDumper::try_borrow_mut_from(state) {
        rd.add_request(&request);
    };

    let sctx = ServerContext::borrow_from(state);
    let rctx = RequestContext::borrow_from(state).clone();
    let hg_repo_ctx = get_repo(sctx, &rctx, &params.repo, None).await?;

    if justknobs::eval(
        "scm/mononoke:disable_pull_lazy",
        None,
        Some(hg_repo_ctx.repo().name()),
    )
    .map_err(HttpError::e500)?
    {
        return Err(HttpError::e500(anyhow!("pull_lazy is disabled")));
    }

    let common: Vec<HgChangesetId> = request.common.into_iter().map(Into::into).collect();
    let missing: Vec<HgChangesetId> = request.missing.into_iter().map(Into::into).collect();
    let clone_data = hg_repo_ctx
        .segmented_changelog_pull_data(common, missing)
        .await
        .map_err(|e| e.into_http_error("error getting segmented changelog data"))?;
    let idmap = clone_data
        .idmap
        .into_iter()
        .map(|(k, v)| WireIdMapEntry {
            dag_id: k.to_wire(),
            hg_id: HgId::from(v.into_nodehash()).to_wire(),
        })
        .collect();
    let wire_clone_data = WireCloneData {
        flat_segments: clone_data
            .flat_segments
            .segments
            .into_iter()
            .collect::<Vec<_>>()
            .to_wire(),
        idmap,
    };

    Ok(BytesBody::new(
        cbor::to_cbor_bytes(&wire_clone_data).map_err(HttpError::e500)?,
        cbor::cbor_mime(),
    ))
}

// TODO(quark): Remove this once callsites are migrated to pull_lazy
pub async fn pull_fast_forward_master(state: &mut State) -> Result<BytesBody<Bytes>, HttpError> {
    let params = PullFastForwardParams::take_from(state);

    state.put(HandlerInfo::new(
        &params.repo,
        SaplingRemoteApiMethod::PullFastForwardMaster,
    ));
    let request = parse_wire_request::<WirePullFastForwardRequest>(state).await?;
    if let Some(rd) = RequestDumper::try_borrow_mut_from(state) {
        rd.add_request(&request);
    };

    let sctx = ServerContext::borrow_from(state);
    let rctx = RequestContext::borrow_from(state).clone();
    let hg_repo_ctx = get_repo(sctx, &rctx, &params.repo, None).await?;

    if justknobs::eval(
        "scm/mononoke:disable_pull_fast_forward_master",
        None,
        Some(hg_repo_ctx.repo().name()),
    )
    .map_err(HttpError::e500)?
    {
        return Err(HttpError::e500(anyhow!(
            "pull_fast_forward_master is disabled"
        )));
    }

    let old_master: HgChangesetId = request.old_master.into();
    let new_master: HgChangesetId = request.new_master.into();
    let clone_data = hg_repo_ctx
        .segmented_changelog_pull_data(vec![old_master], vec![new_master])
        .await
        .map_err(|e| e.into_http_error("error getting segmented changelog data"))?;
    let idmap = clone_data
        .idmap
        .into_iter()
        .map(|(k, v)| WireIdMapEntry {
            dag_id: k.to_wire(),
            hg_id: HgId::from(v.into_nodehash()).to_wire(),
        })
        .collect();
    let wire_clone_data = WireCloneData {
        flat_segments: clone_data
            .flat_segments
            .segments
            .into_iter()
            .collect::<Vec<_>>()
            .to_wire(),
        idmap,
    };

    Ok(BytesBody::new(
        cbor::to_cbor_bytes(&wire_clone_data).map_err(HttpError::e500)?,
        cbor::cbor_mime(),
    ))
}
