/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Error;
use anyhow::Result;
pub use async_requests_types_thrift::AsynchronousRequestParams as ThriftAsynchronousRequestParams;
pub use async_requests_types_thrift::AsynchronousRequestParamsId as ThriftAsynchronousRequestParamsId;
pub use async_requests_types_thrift::AsynchronousRequestResult as ThriftAsynchronousRequestResult;
pub use async_requests_types_thrift::AsynchronousRequestResultId as ThriftAsynchronousRequestResultId;
use async_trait::async_trait;
use blobstore::impl_loadable_storable;
use blobstore::Blobstore;
use context::CoreContext;
use fbthrift::compact_protocol;
pub use megarepo_config::SyncTargetConfig;
pub use megarepo_config::Target;
use mononoke_api::Mononoke;
use mononoke_api::MononokeRepo;
use mononoke_types::hash::Blake2;
use mononoke_types::impl_typed_context;
use mononoke_types::impl_typed_hash_no_context;
use mononoke_types::BlobstoreKey;
use mononoke_types::RepositoryId;
pub use requests_table::RequestStatus;
pub use requests_table::RequestType;
pub use requests_table::RowId;
pub use source_control::AsyncPingParams as ThriftAsyncPingParams;
pub use source_control::AsyncPingPollResponse as ThriftAsyncPingPollResponse;
pub use source_control::AsyncPingResponse as ThriftAsyncPingResponse;
pub use source_control::AsyncPingResult as ThriftAsyncPingResult;
pub use source_control::AsyncPingToken as ThriftAsyncPingToken;
pub use source_control::MegarepoAddBranchingTargetParams as ThriftMegarepoAddBranchingTargetParams;
pub use source_control::MegarepoAddBranchingTargetPollResponse as ThriftMegarepoAddBranchingTargetPollResponse;
pub use source_control::MegarepoAddBranchingTargetResponse as ThriftMegarepoAddBranchingTargetResponse;
pub use source_control::MegarepoAddBranchingTargetResult as ThriftMegarepoAddBranchingTargetResult;
pub use source_control::MegarepoAddBranchingTargetToken as ThriftMegarepoAddBranchingTargetToken;
pub use source_control::MegarepoAddTargetParams as ThriftMegarepoAddTargetParams;
pub use source_control::MegarepoAddTargetPollResponse as ThriftMegarepoAddTargetPollResponse;
pub use source_control::MegarepoAddTargetResponse as ThriftMegarepoAddTargetResponse;
pub use source_control::MegarepoAddTargetResult as ThriftMegarepoAddTargetResult;
pub use source_control::MegarepoAddTargetToken as ThriftMegarepoAddTargetToken;
pub use source_control::MegarepoChangeConfigToken as ThriftMegarepoChangeConfigToken;
pub use source_control::MegarepoChangeTargetConfigParams as ThriftMegarepoChangeTargetConfigParams;
pub use source_control::MegarepoChangeTargetConfigPollResponse as ThriftMegarepoChangeTargetConfigPollResponse;
pub use source_control::MegarepoChangeTargetConfigResponse as ThriftMegarepoChangeTargetConfigResponse;
pub use source_control::MegarepoChangeTargetConfigResult as ThriftMegarepoChangeTargetConfigResult;
pub use source_control::MegarepoRemergeSourceParams as ThriftMegarepoRemergeSourceParams;
pub use source_control::MegarepoRemergeSourcePollResponse as ThriftMegarepoRemergeSourcePollResponse;
pub use source_control::MegarepoRemergeSourceResponse as ThriftMegarepoRemergeSourceResponse;
pub use source_control::MegarepoRemergeSourceResult as ThriftMegarepoRemergeSourceResult;
pub use source_control::MegarepoRemergeSourceToken as ThriftMegarepoRemergeSourceToken;
pub use source_control::MegarepoSyncChangesetParams as ThriftMegarepoSyncChangesetParams;
pub use source_control::MegarepoSyncChangesetPollResponse as ThriftMegarepoSyncChangesetPollResponse;
pub use source_control::MegarepoSyncChangesetResponse as ThriftMegarepoSyncChangesetResponse;
pub use source_control::MegarepoSyncChangesetResult as ThriftMegarepoSyncChangesetResult;
pub use source_control::MegarepoSyncChangesetToken as ThriftMegarepoSyncChangesetToken;
pub use source_control::MegarepoSyncTargetConfig as ThriftMegarepoSyncTargetConfig;
pub use source_control::MegarepoTarget as ThriftMegarepoTarget;
pub use source_control::RepoSpecifier as ThriftRepoSpecifier;

use crate::error::AsyncRequestsError;

const LEGACY_VALUE_TYPE_PARAMS: [&str; 1] = [
    // Support the old format during the transition
    "MegarepoAsynchronousRequestParams",
];

/// Grouping of types and behaviors for an asynchronous request
pub trait Request: Sized + Send + Sync {
    /// Name of the request
    const NAME: &'static str;
    /// Rust newtype for a polling token
    type Token: Token;

    /// Underlying thrift type for request params
    type ThriftParams: ThriftParams<R = Self>;

    /// Underlying thrift type for successful request response
    type ThriftResponse;

    /// Underlying thrift type for for request result (response or error)
    type ThriftResult: ThriftResult<R = Self>;

    /// A type representing potentially present response
    type PollResponse;

    /// Convert thrift result into a result of a poll response
    fn thrift_result_into_poll_response(tr: Self::ThriftResult) -> Self::PollResponse;

    /// Return an empty poll response. This indicates
    /// that the request hasn't been processed yet
    fn empty_poll_response() -> Self::PollResponse;
}

/// Thrift type representing async service method parameters
pub trait ThriftParams: Sized + Send + Sync + Into<AsynchronousRequestParams> + Debug {
    type R: Request<ThriftParams = Self>;

    /// Every *Params argument referes to some Target
    /// This method is needed to extract it from the
    /// implementor of this trait
    fn target(&self) -> String;
}
pub trait ThriftResult:
    Sized + Send + Sync + TryFrom<AsynchronousRequestResult, Error = AsyncRequestsError>
{
    type R: Request<ThriftResult = Self>;
}

/// Polling token for an async service method
pub trait Token: Clone + Sized + Send + Sync + Debug {
    type R: Request<Token = Self>;
    type ThriftToken;

    fn into_thrift(self) -> Self::ThriftToken;
    fn from_db_id(id: RowId) -> Result<Self, AsyncRequestsError>;
    fn to_db_id(&self) -> Result<RowId, AsyncRequestsError>;

    fn id(&self) -> RowId;
}

/// This macro implements an async service method type,
/// which can be stored/retrieved from the blobstore.
/// Such types are usually represented as value/handle pairs.
/// Since we need to implement (potentially foreign) traits
/// on these types, we also define corrensponding Rust types
/// Some of the defined types (like context or thrift_type_newtype)
/// are not used from outside of the macro, but we still need
/// to pass identifiers for them from the outside, because
/// Rusts' macro hygiene does not allow identifier generation ¯\_(ツ)_/¯
macro_rules! impl_async_svc_stored_type {
    {
        /// Rust type for the Loadable handle
        handle_type => $handle_type: ident,
        /// Underlying thrift type for the handle
        handle_thrift_type => $handle_thrift_type: ident,
        /// A name for a Newtype-style trait, required by `impl_typed_hash_no_context`
        /// Rust type for the Storable value
        value_type => $value_type: ident,
        /// Underlying thrift type for the value
        value_thrift_type => $value_thrift_type: ident,
        /// A helper struct for hash computations
        context_type => $context_type: ident,
    } => {
        /// Rust handle type, wrapper around a Blake2 instance
        #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
        pub struct $handle_type(Blake2);

        impl_typed_hash_no_context! {
            hash_type => $handle_type,
            thrift_type => $handle_thrift_type,
            blobstore_key => concat!("async.svc.", stringify!($value_type)),
        }

        // Typed context type is needed for hash computation
        impl_typed_context! {
            hash_type => $handle_type,
            context_type => $context_type,
            context_key => stringify!($value_type),
        }

        /// Main value type
        #[derive(Debug, Clone, PartialEq)]
        pub struct $value_type {
            id: $handle_type,
            thrift: $value_thrift_type,
        }

        impl $value_type {
            pub fn from_thrift(thrift: $value_thrift_type) -> Self {
                let data = compact_protocol::serialize(&thrift);
                let mut context = $context_type::new();
                context.update(&data);
                let id = context.finish();
                Self { id, thrift }
            }

            pub async fn load_from_key(ctx: &CoreContext, blobstore: &Arc<dyn Blobstore>, key: &str) -> Result<Self, AsyncRequestsError> {
                let bytes = blobstore.get(ctx, key).await?;
                Self::check_prefix(key)?;
                match bytes {
                    Some(bytes) => Ok(bytes.into_bytes().try_into()?),
                    None => Err(AsyncRequestsError::internal(anyhow!("Missing blob: {}", key))),
                }
            }

            pub fn check_prefix(key: &str) -> Result<(), AsyncRequestsError> {
                let prefix = concat!("async.svc.", stringify!($value_type), ".blake2.");
                if key.strip_prefix(prefix).is_some() {
                    return Ok(());
                }

                // if the standard prefix is not valid, this might be in one of an alternative prefixes we support
                for vt in LEGACY_VALUE_TYPE_PARAMS {
                    let prefix = format!("async.svc.{}.blake2.", vt);
                    if key.strip_prefix(&prefix).is_some() {
                        return Ok(());
                    }
                }

                return Err(AsyncRequestsError::internal(anyhow!("{} is not a blobstore key for {}", key, stringify!($value_type))));
            }

            pub fn handle(&self) -> &$handle_type {
                &self.id
            }

            pub fn thrift(&self) -> &$value_thrift_type {
                &self.thrift
            }

        }

        // Conversions between thrift types and their Rust counterparts

        impl TryFrom<$handle_thrift_type> for $handle_type {
            type Error = Error;

            fn try_from(t: $handle_thrift_type) -> Result<Self, Self::Error> {
                Self::from_thrift(t)
            }
        }

        impl From<$handle_type> for $handle_thrift_type {
            fn from(other: $handle_type) -> Self {
                Self(mononoke_types_serialization::id::Id::Blake2(other.0.into_thrift()))
            }
        }

        impl TryFrom<$value_thrift_type> for $value_type {
            type Error = Error;

            fn try_from(t: $value_thrift_type) -> Result<Self, Self::Error> {
                Ok(Self::from_thrift(t))
            }
        }

        impl From<$value_type> for $value_thrift_type {
            fn from(other: $value_type) -> Self {
                other.thrift
            }
        }

        impl_loadable_storable! {
            handle_type => $handle_type,
            handle_thrift_type => $handle_thrift_type,
            value_type => $value_type,
            value_thrift_type => $value_thrift_type,
        }
    }
}

/// A macro to call impl_async_svc_stored_type for params/result
/// types, as well as define a bunch of relationships between
/// these types, and their Request-related frients.
/// An underlying idea is to define as much behavior and relationships
/// as possible in the type system, so that we
/// (a) minimize a chance of using incorrent pair of types somewhere
/// (b) can write generic enqueing/polling functions
macro_rules! impl_async_svc_method_types {
    {
        method_name => $method_name: expr,
        request_struct => $request_struct: ident,

        params_value_thrift_type => $params_value_thrift_type: ident,
        params_union_variant => $params_union_variant: ident,

        result_value_thrift_type => $result_value_thrift_type: ident,
        result_union_variant => $result_union_variant: ident,

        response_type => $response_type: ident,
        poll_response_type => $poll_response_type: ident,
        token_type => $token_type: ident,
        token_thrift_type => $token_thrift_type: ident,

        fn target(&$self_ident: ident: ThriftParams) -> String $target_in_params: tt

    } => {
        impl ThriftParams for $params_value_thrift_type {
            type R = $request_struct;

            fn target(&$self_ident) -> String {
                $target_in_params
            }
        }

        #[derive(Clone, Debug)]
        pub struct $token_type(pub $token_thrift_type);

        impl Token for $token_type {
            type ThriftToken = $token_thrift_type;
            type R = $request_struct;

            fn from_db_id(id: RowId) -> Result<Self, AsyncRequestsError> {
                // Thrift token is a string alias
                // but's guard ourselves here against
                // it changing unexpectedly.
                let thrift_token = $token_thrift_type {
                    id: id.0 as i64,
                    ..Default::default()
                };
                Ok(Self(thrift_token))
            }

            fn to_db_id(&self) -> Result<RowId, AsyncRequestsError> {
                let row_id = self.0.id as u64;
                let row_id = RowId(row_id);

                Ok(row_id)
            }

            fn id(&self) -> RowId {
                RowId(self.0.id as u64)
            }

            fn into_thrift(self) -> $token_thrift_type {
                self.0
            }
        }

        impl From<Result<$response_type, AsyncRequestsError>> for AsynchronousRequestResult {
            fn from(r: Result<$response_type, AsyncRequestsError>) -> AsynchronousRequestResult {
                let thrift = match r {
                    Ok(payload) => ThriftAsynchronousRequestResult::$result_union_variant($result_value_thrift_type::success(payload)),
                    Err(e) => ThriftAsynchronousRequestResult::$result_union_variant($result_value_thrift_type::error(e.into()))
                };

                AsynchronousRequestResult::from_thrift(thrift)
            }
        }

        impl From<$result_value_thrift_type> for AsynchronousRequestResult {
            fn from(r: $result_value_thrift_type) -> AsynchronousRequestResult {
                let thrift = ThriftAsynchronousRequestResult::$result_union_variant(r);
                AsynchronousRequestResult::from_thrift(thrift)
            }
        }

        impl From<$params_value_thrift_type> for AsynchronousRequestParams{
            fn from(params: $params_value_thrift_type) -> AsynchronousRequestParams {
                AsynchronousRequestParams::from_thrift(
                    ThriftAsynchronousRequestParams::$params_union_variant(params)
                )
            }
        }

        impl ThriftResult for $result_value_thrift_type {
            type R = $request_struct;
        }

        impl TryFrom<AsynchronousRequestResult> for $result_value_thrift_type {
            type Error = AsyncRequestsError;

            fn try_from(r: AsynchronousRequestResult) -> Result<$result_value_thrift_type, Self::Error> {
                match r.thrift {
                    ThriftAsynchronousRequestResult::$result_union_variant(payload) => Ok(payload),
                    ThriftAsynchronousRequestResult::UnknownField(x) => {
                        // TODO: maybe use structured error?
                        Err(AsyncRequestsError::internal(
                            anyhow!(
                                "failed to parse {} thrift. UnknownField: {}",
                                stringify!($result_value_thrift_type),
                                x,
                            )
                        ))
                    },
                    x => {
                        Err(AsyncRequestsError::internal(
                            anyhow!(
                                "failed to parse {} thrift. The result union contains the wrong result variant: {:?}",
                                stringify!($result_value_thrift_type),
                                x,
                            )
                        ))
                    }
                }
            }
        }

        pub struct $request_struct;

        impl Request for $request_struct {
            const NAME: &'static str = $method_name;

            type Token = $token_type;
            type ThriftParams = $params_value_thrift_type;
            type ThriftResult = $result_value_thrift_type;
            type ThriftResponse = $response_type;

            type PollResponse = $poll_response_type;

            fn thrift_result_into_poll_response(
                thrift_result: Self::ThriftResult,
            ) -> Self::PollResponse {
                $poll_response_type { result: Some(thrift_result), ..Default::default() }
            }

            fn empty_poll_response() -> Self::PollResponse {
                $poll_response_type { result: None, ..Default::default() }
            }
        }

    }
}

// Params and result types for megarepo_add_sync_target

impl_async_svc_method_types! {
    method_name => "megarepo_add_sync_target",
    request_struct => MegarepoAddSyncTarget,

    params_value_thrift_type => ThriftMegarepoAddTargetParams,
    params_union_variant => megarepo_add_target_params,

    result_value_thrift_type => ThriftMegarepoAddTargetResult,
    result_union_variant => megarepo_add_target_result,

    response_type => ThriftMegarepoAddTargetResponse,
    poll_response_type => ThriftMegarepoAddTargetPollResponse,
    token_type => MegarepoAddTargetToken,
    token_thrift_type => ThriftMegarepoAddTargetToken,

    fn target(&self: ThriftParams) -> String {
        render_target(&self.config_with_new_target.target)
    }
}

// Params and result types for megarepo_add_branching_sync_target

impl_async_svc_method_types! {
    method_name => "megarepo_add_branching_sync_target",
    request_struct => MegarepoAddBranchingSyncTarget,

    params_value_thrift_type => ThriftMegarepoAddBranchingTargetParams,
    params_union_variant => megarepo_add_branching_target_params,

    result_value_thrift_type => ThriftMegarepoAddBranchingTargetResult,
    result_union_variant => megarepo_add_branching_target_result,

    response_type => ThriftMegarepoAddBranchingTargetResponse,
    poll_response_type => ThriftMegarepoAddBranchingTargetPollResponse,
    token_type => MegarepoAddBranchingTargetToken,
    token_thrift_type => ThriftMegarepoAddBranchingTargetToken,

    fn target(&self: ThriftParams) -> String {
        render_target(&self.target)
    }
}

// Params and result types for megarepo_change_target_config

impl_async_svc_method_types! {
    method_name => "megarepo_change_target_config",
    request_struct => MegarepoChangeTargetConfig,

    params_value_thrift_type => ThriftMegarepoChangeTargetConfigParams,
    params_union_variant => megarepo_change_target_params,

    result_value_thrift_type => ThriftMegarepoChangeTargetConfigResult,
    result_union_variant => megarepo_change_target_result,

    response_type => ThriftMegarepoChangeTargetConfigResponse,
    poll_response_type => ThriftMegarepoChangeTargetConfigPollResponse,
    token_type => MegarepoChangeTargetConfigToken,
    token_thrift_type => ThriftMegarepoChangeConfigToken,

    fn target(&self: ThriftParams) -> String {
        render_target(&self.target)
    }
}

// Params and result types for megarepo_sync_changeset

impl_async_svc_method_types! {
    method_name => "megarepo_sync_changeset",
    request_struct => MegarepoSyncChangeset,

    params_value_thrift_type => ThriftMegarepoSyncChangesetParams,
    params_union_variant => megarepo_sync_changeset_params,

    result_value_thrift_type => ThriftMegarepoSyncChangesetResult,
    result_union_variant => megarepo_sync_changeset_result,

    response_type => ThriftMegarepoSyncChangesetResponse,
    poll_response_type => ThriftMegarepoSyncChangesetPollResponse,
    token_type => MegarepoSyncChangesetToken,
    token_thrift_type => ThriftMegarepoSyncChangesetToken,

    fn target(&self: ThriftParams) -> String {
        render_target(&self.target)
    }
}

// Params and result types for megarepo_remerge_source

impl_async_svc_method_types! {
    method_name => "megarepo_remerge_source",
    request_struct => MegarepoRemergeSource,

    params_value_thrift_type => ThriftMegarepoRemergeSourceParams,
    params_union_variant => megarepo_remerge_source_params,

    result_value_thrift_type => ThriftMegarepoRemergeSourceResult,
    result_union_variant => megarepo_remerge_source_result,

    response_type => ThriftMegarepoRemergeSourceResponse,
    poll_response_type => ThriftMegarepoRemergeSourcePollResponse,
    token_type => MegarepoRemergeSourceToken,
    token_thrift_type => ThriftMegarepoRemergeSourceToken,

    fn target(&self: ThriftParams) -> String {
        render_target(&self.target)
    }
}

// Params and result types for async_ping

impl_async_svc_method_types! {
    method_name => "async_ping",
    request_struct => AsyncPing,

    params_value_thrift_type => ThriftAsyncPingParams,
    params_union_variant => async_ping_params,

    result_value_thrift_type => ThriftAsyncPingResult,
    result_union_variant => async_ping_result,

    response_type => ThriftAsyncPingResponse,
    poll_response_type => ThriftAsyncPingPollResponse,
    token_type => AsyncPingToken,
    token_thrift_type => ThriftAsyncPingToken,

    fn target(&self: ThriftParams) -> String {
        "".to_string()
    }
}

impl_async_svc_stored_type! {
    handle_type => AsynchronousRequestParamsId,
    handle_thrift_type => ThriftAsynchronousRequestParamsId,
    value_type => AsynchronousRequestParams,
    value_thrift_type => ThriftAsynchronousRequestParams,
    context_type => AsynchronousRequestParamsIdContext,
}

impl_async_svc_stored_type! {
    handle_type => AsynchronousRequestResultId,
    handle_thrift_type => ThriftAsynchronousRequestResultId,
    value_type => AsynchronousRequestResult,
    value_thrift_type => ThriftAsynchronousRequestResult,
    context_type => AsynchronousRequestResultIdContext,
}

fn render_target(target: &ThriftMegarepoTarget) -> String {
    format!(
        "{}: {}, bookmark: {}",
        target
            .repo
            .as_ref()
            .map_or_else(|| "repo_id".to_string(), |_| "repo_name".to_string(),),
        target.repo.as_ref().map_or_else(
            || target.repo_id.unwrap_or(0).to_string(),
            |repo| repo.name.clone()
        ),
        target.bookmark
    )
}

impl AsynchronousRequestParams {
    pub fn target(&self) -> Result<String, AsyncRequestsError> {
        match &self.thrift {
            ThriftAsynchronousRequestParams::megarepo_add_target_params(params) => {
                Ok(params.target())
            }
            ThriftAsynchronousRequestParams::megarepo_add_branching_target_params(params) => {
                Ok(params.target())
            }
            ThriftAsynchronousRequestParams::megarepo_change_target_params(params) => {
                Ok(params.target())
            }
            ThriftAsynchronousRequestParams::megarepo_remerge_source_params(params) => {
                Ok(params.target())
            }
            ThriftAsynchronousRequestParams::megarepo_sync_changeset_params(params) => {
                Ok(params.target())
            }
            ThriftAsynchronousRequestParams::async_ping_params(params) => Ok(params.target()),
            ThriftAsynchronousRequestParams::UnknownField(union_tag) => {
                Err(AsyncRequestsError::internal(anyhow!(
                    "this type of request (AsynchronousRequestParams tag {}) not supported by this worker!",
                    union_tag
                )))
            }
        }
    }
}

/// Convert an item into a thrift type we use for storing configuration
pub trait IntoConfigFormat<T, R> {
    fn into_config_format(self, mononoke: &Mononoke<R>) -> Result<T, AsyncRequestsError>;
}

impl<R: MononokeRepo> IntoConfigFormat<Target, R> for ThriftMegarepoTarget {
    fn into_config_format(self, mononoke: &Mononoke<R>) -> Result<Target, AsyncRequestsError> {
        let repo_id = match (self.repo, self.repo_id) {
            (Some(repo), _) => mononoke
                .repo_id_from_name(repo.name.clone())
                .ok_or_else(|| anyhow!("Invalid repo_name {}", repo.name))?
                .id() as i64,
            (_, Some(repo_id)) => repo_id,
            (None, None) => Err(anyhow!("both repo_id and repo_name are None!"))?,
        };

        Ok(Target {
            repo_id,
            bookmark: self.bookmark,
        })
    }
}

impl<R: MononokeRepo> IntoConfigFormat<SyncTargetConfig, R> for ThriftMegarepoSyncTargetConfig {
    fn into_config_format(
        self,
        mononoke: &Mononoke<R>,
    ) -> Result<SyncTargetConfig, AsyncRequestsError> {
        Ok(SyncTargetConfig {
            target: self.target.into_config_format(mononoke)?,
            sources: self.sources,
            version: self.version,
        })
    }
}

/// Convert an item into a thrift type we use in APIs
pub trait IntoApiFormat<T, R> {
    fn into_api_format(self, mononoke: &Mononoke<R>) -> Result<T, AsyncRequestsError>;
}

#[async_trait]
impl<R: MononokeRepo> IntoApiFormat<ThriftMegarepoTarget, R> for Target {
    fn into_api_format(
        self,
        mononoke: &Mononoke<R>,
    ) -> Result<ThriftMegarepoTarget, AsyncRequestsError> {
        let repo = mononoke
            .repo_name_from_id(RepositoryId::new(self.repo_id as i32))
            .map(|name| ThriftRepoSpecifier {
                name,
                ..Default::default()
            });
        Ok(ThriftMegarepoTarget {
            repo_id: Some(self.repo_id),
            bookmark: self.bookmark,
            repo,
            ..Default::default()
        })
    }
}

#[async_trait]
impl<R: MononokeRepo> IntoApiFormat<ThriftMegarepoSyncTargetConfig, R> for SyncTargetConfig {
    fn into_api_format(
        self,
        mononoke: &Mononoke<R>,
    ) -> Result<ThriftMegarepoSyncTargetConfig, AsyncRequestsError> {
        Ok(ThriftMegarepoSyncTargetConfig {
            target: self.target.into_api_format(mononoke)?,
            sources: self.sources,
            version: self.version,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod test {
    use blobstore::Loadable;
    use blobstore::PutBehaviour;
    use blobstore::Storable;
    use context::CoreContext;
    use fbinit::FacebookInit;
    use memblob::Memblob;
    use mononoke_macros::mononoke;

    use super::*;

    macro_rules! test_blobstore_key {
        {
            $type: ident,
            $prefix: expr
        } => {
            let id = $type::from_byte_array([1; 32]);
            assert_eq!(id.blobstore_key(), format!(concat!($prefix, ".blake2.{}"), id));
        }
    }

    macro_rules! serialize_deserialize {
        {
            $type: ident
        } => {
            let id = $type::from_byte_array([1; 32]);
            let serialized = serde_json::to_string(&id).unwrap();
            let deserialized = serde_json::from_str(&serialized).unwrap();
            assert_eq!(id, deserialized);
        }
    }

    #[mononoke::test]
    fn blobstore_key() {
        // These IDs are persistent, and this test is really to make sure that they don't change
        // accidentally. Same as in typed_hash.rs
        test_blobstore_key!(
            AsynchronousRequestParamsId,
            "async.svc.AsynchronousRequestParams"
        );
        test_blobstore_key!(
            AsynchronousRequestResultId,
            "async.svc.AsynchronousRequestResult"
        );
    }

    #[mononoke::test]
    fn test_serialize_deserialize() {
        serialize_deserialize!(AsynchronousRequestParamsId);
        serialize_deserialize!(AsynchronousRequestResultId);
    }

    macro_rules! test_store_load {
        { $type: ident, $ctx: ident, $blobstore: ident } => {
            let obj = $type::from_thrift(Default::default());

            let id = obj
                .clone()
                .store(&$ctx, &$blobstore)
                .await
                .expect(&format!("Failed to store {}", stringify!($type)));

            let obj2 = id
                .load(&$ctx, &$blobstore)
                .await
                .expect(&format!("Failed to load {}", stringify!($type)));

            assert_eq!(obj, obj2);
        }
    }

    #[mononoke::fbinit_test]
    async fn test_megaerpo_add_target_params_type(fb: FacebookInit) {
        let blobstore = Memblob::new(PutBehaviour::IfAbsent);
        let ctx = CoreContext::test_mock(fb);
        test_store_load!(AsynchronousRequestParams, ctx, blobstore);
        test_store_load!(AsynchronousRequestResult, ctx, blobstore);
    }
}
