/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use edenapi_types::wire::WireToApiConversionError;
use edenapi_types::EdenApiServerError;
use http::header::HeaderMap;
use http::status::StatusCode;
use http_client::HttpClientError;
use http_client::TlsError;
use http_client::TlsErrorKind;
use thiserror::Error;
use types::errors::NetworkError;

#[derive(Debug, Error)]
pub enum EdenApiError {
    #[error("failed to serialize request: {0}")]
    RequestSerializationFailed(#[source] serde_cbor::Error),
    #[error("failed to parse response: {0}")]
    ParseResponse(String),
    #[error(transparent)]
    BadConfig(#[from] ConfigError),
    #[error(transparent)]
    Http(#[from] HttpClientError),
    #[error("server responded {status} for {url}: {message}. Headers: {headers:#?}")]
    HttpError {
        status: StatusCode,
        message: String,
        headers: HeaderMap,
        url: String,
    },
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    WireToApiConversionFailed(#[from] WireToApiConversionError),
    #[error(transparent)]
    ServerError(#[from] EdenApiServerError),
    #[error("expected response, but none returned by the server")]
    NoResponse,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("not supported by the server")]
    NotSupported,
    #[error(transparent)]
    MissingCerts(#[from] auth::MissingCerts),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required config item: {0}")]
    Missing(String),
    #[error("invalid config item: '{0}' ({1})")]
    Invalid(String, #[source] anyhow::Error),
}

impl EdenApiError {
    pub fn is_rate_limiting(&self) -> bool {
        use EdenApiError::*;
        match self {
            HttpError { status, .. } => {
                if status.is_client_error() {
                    match *status {
                        StatusCode::TOO_MANY_REQUESTS => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn is_retryable(&self) -> bool {
        use http_client::HttpClientError::*;
        use EdenApiError::*;
        match self {
            Http(client_error) => match client_error {
                Tls(TlsError { kind, .. }) => kind == &TlsErrorKind::RecvError,
                _ => true,
            },
            HttpError { status, .. } => {
                // 300-399
                if status.is_redirection() {
                    false
                // 400-499
                } else if status.is_client_error() {
                    match *status {
                        StatusCode::REQUEST_TIMEOUT => true,
                        StatusCode::TOO_MANY_REQUESTS => true,
                        _ => false,
                    }
                // 500-599
                } else if status.is_server_error() {
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    // Report whether this error may be a network error. Err on the side of saying "yes".
    pub fn maybe_network_error(&self) -> bool {
        use EdenApiError::*;

        match self {
            Http(_) | HttpError { .. } | ServerError(_) | NoResponse | Other(_) => true,

            RequestSerializationFailed(_)
            | ParseResponse(_)
            | BadConfig(_)
            | InvalidUrl(_)
            | WireToApiConversionFailed(_)
            | NotSupported
            | MissingCerts(_) => false,
        }
    }

    pub fn tag_network(self) -> anyhow::Error {
        if self.maybe_network_error() {
            NetworkError::wrap(self)
        } else {
            self.into()
        }
    }
}
