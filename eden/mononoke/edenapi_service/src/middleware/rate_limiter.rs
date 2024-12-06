/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use context::CoreContext;
use gotham::state::FromState;
use gotham::state::State;
use gotham_ext::middleware::request_context::RequestContext;
use gotham_ext::middleware::MetadataState;
use gotham_ext::middleware::Middleware;
use hyper::Body;
use hyper::Response;
use hyper::StatusCode;
use hyper::Uri;
use maplit::hashmap;
use rate_limiting::Metric;
use rate_limiting::RateLimitStatus;
use slog::debug;

use crate::utils::build_counter;
use crate::utils::counter_check_and_bump;

const EDENAPI_QPS_LIMIT: &str = "edenapi_qps";

// NOTE: Our Throttling middleware is implemented as Gotham middleware for 3 reasons:
// - It needs to replace responses.
// - It needs to do asynchronously.
// - It only needs to run if we're going to serve a request.

#[derive(Clone)]
pub struct ThrottleMiddleware;
impl ThrottleMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl Middleware for ThrottleMiddleware {
    async fn inbound(&self, state: &mut State) -> Option<Response<Body>> {
        if let Some(uri) = Uri::try_borrow_from(state) {
            if uri.path() == "/health_check" || uri.path() == "/proxygen/health_check" {
                return None;
            }
        }

        let rctx: RequestContext = RequestContext::borrow_from(state).clone();
        let ctx: CoreContext = rctx.ctx;

        let client_request_info = state
            .try_borrow::<MetadataState>()?
            .metadata()
            .client_request_info()
            .or_else(|| {
                debug!(ctx.logger(), "No client request info found");
                None
            })?;
        // Retrieve main client ID
        let main_client_id = client_request_info.main_id.clone().or_else(|| {
            debug!(ctx.logger(), "No main client id found");
            None
        })?;
        // Retrieve rate limiter
        let rate_limiter = ctx.session().rate_limiter().or_else(|| {
            debug!(ctx.logger(), "No rate_limiter info found");
            None
        })?;

        let category = rate_limiter.category();
        let limit = rate_limiter.find_rate_limit(Metric::EdenApiQps)?;

        let enforced = match limit.body.raw_config.status {
            RateLimitStatus::Disabled => return None,
            RateLimitStatus::Tracked => false,
            RateLimitStatus::Enforced => true,
            _ => panic!("Invalid limit status: {:?}", limit.body.raw_config.status),
        };
        let counter = build_counter(&ctx, category, EDENAPI_QPS_LIMIT, &main_client_id);
        let max_value = limit.body.raw_config.limit;
        let time_window = limit.body.window.as_secs() as u32;

        match counter_check_and_bump(
            &ctx,
            counter,
            EDENAPI_QPS_LIMIT,
            max_value,
            time_window,
            enforced,
            hashmap! {"main_client_id" => main_client_id.as_str() },
        )
        .await
        {
            Ok(_) => None,
            Err(response) => {
                let res = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(response.to_string().into())
                    .expect("Couldn't build http response");
                Some(res)
            }
        }
    }
}