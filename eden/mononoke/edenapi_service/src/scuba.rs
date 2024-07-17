/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use gotham::state::State;
use gotham_ext::middleware::request_context::RequestContext;
use gotham_ext::middleware::MetadataState;
use gotham_ext::middleware::PostResponseInfo;
use gotham_ext::middleware::ScubaHandler;
use permission_checker::MononokeIdentitySetExt;
use scuba_ext::MononokeScubaSampleBuilder;

use crate::handlers::HandlerInfo;

#[derive(Copy, Clone, Debug)]
pub enum SaplingRemoteApiScubaKey {
    Repo,
    Method,
    User,
    HandlerError,
    HandlerErrorCount,
}

impl AsRef<str> for SaplingRemoteApiScubaKey {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::Method => "edenapi_method",
            Self::User => "edenapi_user",
            Self::HandlerError => "edenapi_error",
            Self::HandlerErrorCount => "edenapi_error_count",
        }
    }
}

impl From<SaplingRemoteApiScubaKey> for String {
    fn from(key: SaplingRemoteApiScubaKey) -> Self {
        key.as_ref().to_string()
    }
}

#[derive(Clone)]
pub struct SaplingRemoteApiScubaHandler {
    request_context: Option<RequestContext>,
    handler_info: Option<HandlerInfo>,
    client_username: Option<String>,
}

impl ScubaHandler for SaplingRemoteApiScubaHandler {
    fn from_state(state: &State) -> Self {
        Self {
            request_context: state.try_borrow::<RequestContext>().cloned(),
            handler_info: state.try_borrow::<HandlerInfo>().cloned(),
            client_username: state
                .try_borrow::<MetadataState>()
                .and_then(|metadata_state| metadata_state.metadata().identities().username())
                .map(ToString::to_string),
        }
    }

    fn log_processed(self, info: &PostResponseInfo, mut scuba: MononokeScubaSampleBuilder) {
        scuba.add_opt(SaplingRemoteApiScubaKey::User, self.client_username);

        if let Some(info) = self.handler_info {
            scuba.add_opt(SaplingRemoteApiScubaKey::Repo, info.repo.clone());
            scuba.add_opt(
                SaplingRemoteApiScubaKey::Method,
                info.method.map(|m| m.to_string()),
            );
        }

        if let Some(ctx) = self.request_context {
            ctx.ctx.perf_counters().insert_perf_counters(&mut scuba);
        }

        if let Some(err) = info.first_error() {
            scuba.add(SaplingRemoteApiScubaKey::HandlerError, format!("{:?}", err));
        }

        scuba.add(
            SaplingRemoteApiScubaKey::HandlerErrorCount,
            info.error_count(),
        );

        scuba.add("log_tag", "EdenAPI Request Processed");
        scuba.log();
    }

    fn log_cancelled(mut scuba: MononokeScubaSampleBuilder) {
        scuba.add("log_tag", "EdenAPI Request Cancelled");
        scuba.log();
    }
}
