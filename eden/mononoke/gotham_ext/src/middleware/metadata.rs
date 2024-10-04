/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::net::IpAddr;
use std::net::SocketAddr;

use cats::try_get_cats_idents;
use clientinfo::ClientEntryPoint;
use clientinfo::ClientInfo;
use clientinfo::CLIENT_INFO_HEADER;
use fbinit::FacebookInit;
use gotham::state::client_addr;
use gotham::state::FromState;
use gotham::state::State;
use gotham_derive::StateData;
use hyper::header::HeaderMap;
use hyper::Body;
use hyper::Response;
use hyper::StatusCode;
use hyper::Uri;
use metaconfig_types::Identity;
use metadata::Metadata;
use percent_encoding::percent_decode;
use permission_checker::MononokeIdentity;
use permission_checker::MononokeIdentitySet;
use slog::error;
use slog::Logger;

use super::Middleware;
use crate::socket_data::TlsCertificateIdentities;
use crate::state_ext::StateExt;

const INGRESS_LEAF_CERT_HEADER: &str = "X-Amzn-Mtls-Clientcert-Leaf";
const ENCODED_CLIENT_IDENTITY: &str = "x-fb-validated-client-encoded-identity";
const CLIENT_IP: &str = "tfb-orig-client-ip";
const CLIENT_PORT: &str = "tfb-orig-client-port";
const HEADER_REVPROXY_REGION: &str = "x-fb-revproxy-region";

#[derive(StateData, Default)]
pub struct MetadataState(Metadata);

impl MetadataState {
    pub fn metadata(&self) -> &Metadata {
        &self.0
    }
}

pub struct MetadataMiddleware {
    fb: FacebookInit,
    logger: Logger,
    internal_identity: Identity,
    entry_point: ClientEntryPoint,
    mtls_disabled: bool,
}

impl MetadataMiddleware {
    pub fn new(
        fb: FacebookInit,
        logger: Logger,
        internal_identity: Identity,
        entry_point: ClientEntryPoint,
        mtls_disabled: bool,
    ) -> Self {
        Self {
            fb,
            logger,
            internal_identity,
            entry_point,
            mtls_disabled,
        }
    }

    fn extract_client_identities(
        &self,
        tls_certificate_identities: TlsCertificateIdentities,
        headers: &HeaderMap,
    ) -> Option<MononokeIdentitySet> {
        match tls_certificate_identities {
            TlsCertificateIdentities::TrustedProxy(idents) => {
                match request_identities_from_headers(headers) {
                    Some(identies_from_headers) => Some(identies_from_headers),
                    None => Some(idents),
                }
            }
            TlsCertificateIdentities::Authenticated(idents) => Some(idents),
        }
    }

    fn require_client_info(&self, state: &State) -> bool {
        let is_health_check =
            Uri::try_borrow_from(state).map_or(false, |uri| uri.path().ends_with("/health_check"));
        let is_git_server = self.entry_point == ClientEntryPoint::MononokeGitServer;
        !is_health_check && !is_git_server
    }
}

fn request_ip_from_headers(headers: &HeaderMap) -> Option<IpAddr> {
    let header = headers.get(CLIENT_IP)?;
    let header = header.to_str().ok()?;
    let ip = header.parse().ok()?;
    Some(ip)
}

fn request_port_from_headers(headers: &HeaderMap) -> Option<u16> {
    let header = headers.get(CLIENT_PORT)?;
    let header = header.to_str().ok()?;
    let ip = header.parse().ok()?;
    Some(ip)
}

fn revproxy_region_from_headers(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(HEADER_REVPROXY_REGION)?;
    let header = header.to_str().ok()?;
    let region = header.parse().ok()?;
    Some(region)
}

fn request_identities_from_headers(headers: &HeaderMap) -> Option<MononokeIdentitySet> {
    let encoded_identities = headers.get(ENCODED_CLIENT_IDENTITY)?;
    let json_identities = percent_decode(encoded_identities.as_bytes())
        .decode_utf8()
        .ok()?;
    MononokeIdentity::try_from_json_encoded(&json_identities).ok()
}

pub fn ingress_request_identities_from_headers(headers: &HeaderMap) -> Option<MononokeIdentitySet> {
    let encoded_cert = headers.get(INGRESS_LEAF_CERT_HEADER)?;
    let cert = openssl::x509::X509::from_pem(
        &percent_decode(encoded_cert.as_bytes()).collect::<Vec<u8>>(),
    )
    .ok()?;
    MononokeIdentity::try_from_x509(&cert).ok()
}

#[async_trait::async_trait]
impl Middleware for MetadataMiddleware {
    async fn inbound(&self, state: &mut State) -> Option<Response<Body>> {
        let cert_idents = TlsCertificateIdentities::try_take_from(state);
        let mut metadata = Metadata::default();

        if let Some(headers) = HeaderMap::try_borrow_from(state) {
            metadata = metadata
                .set_client_ip(request_ip_from_headers(headers))
                .set_client_port(request_port_from_headers(headers));

            if let Some(revproxy_region) = revproxy_region_from_headers(headers) {
                metadata.add_revproxy_region(revproxy_region);
            }

            let maybe_identities = if self.mtls_disabled {
                ingress_request_identities_from_headers(headers)
            } else {
                let maybe_cat_idents =
                    match try_get_cats_idents(self.fb, headers, &self.internal_identity) {
                        Err(e) => {
                            let msg = format!("Error extracting CATs identities: {}.", &e,);
                            error!(self.logger, "{}", &msg,);
                            let response = Response::builder()
                                .status(StatusCode::UNAUTHORIZED)
                                .body(
                                    format!(
                                        "{{\"message:\"{}\", \"request_id\":\"{}\"}}",
                                        msg,
                                        state.short_request_id()
                                    )
                                    .into(),
                                )
                                .expect("Couldn't build http response");

                            return Some(response);
                        }
                        Ok(maybe_cats) => maybe_cats,
                    };

                let maybe_tls_or_proxied_idents: Option<MononokeIdentitySet> =
                    cert_idents.and_then(|x| self.extract_client_identities(x, headers));

                match (maybe_cat_idents, maybe_tls_or_proxied_idents) {
                    (None, None) => None,
                    (Some(cat_idents), Some(tls_or_proxied_idents)) => {
                        Some(cat_idents.union(&tls_or_proxied_idents).cloned().collect())
                    }
                    (Some(cat_idents), None) => Some(cat_idents),
                    (None, Some(tls_or_proxied_idents)) => Some(tls_or_proxied_idents),
                }
            };
            if let Some(identities) = maybe_identities {
                metadata = metadata.set_identities(identities)
            }
            let client_info: Option<ClientInfo> = headers
                .get(CLIENT_INFO_HEADER)
                .and_then(|h| h.to_str().ok())
                .and_then(|ci| serde_json::from_str(ci).ok());

            if client_info.is_none() && self.require_client_info(state) {
                let msg = format!(
                    "Error: {} header not provided or wrong format (expected json).",
                    CLIENT_INFO_HEADER
                );
                error!(self.logger, "{}", &msg,);
                let response = Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(format!("{{\"message:\"{}\"}}", msg,).into())
                    .expect("Couldn't build http response");
                return Some(response);
            }

            let client_info = client_info
                .unwrap_or_else(|| ClientInfo::default_with_entry_point(self.entry_point.clone()));
            metadata.add_client_info(client_info);
            metadata.update_client_untrusted(
                metadata::security::is_client_untrusted(|h| {
                    Ok(headers
                        .get(h)
                        .map(|h| h.to_str().map(|s| s.to_owned()))
                        .transpose()?)
                })
                .unwrap_or_default(),
            );
        }

        // For the IP, we can fallback to the peer IP
        if metadata.client_ip().is_none() {
            let client_addr = client_addr(state);

            metadata = metadata
                .set_client_ip(client_addr.as_ref().map(SocketAddr::ip))
                .set_client_port(client_addr.as_ref().map(SocketAddr::port));
        }

        state.put(MetadataState(metadata));

        None
    }
}
