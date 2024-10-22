/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::panic::RefUnwindSafe;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::Error;
use cloned::cloned;
use connection_security_checker::ConnectionSecurityChecker;
use futures::future::TryFutureExt;
use gotham::handler::Handler;
use hyper::server::conn::Http;
use openssl::ssl::Ssl;
use openssl::ssl::SslAcceptor;
use quiet_stream::QuietShutdownStream;
use slog::warn;
use slog::Logger;
use tokio::net::TcpListener;
use tokio_openssl::SslStream;

use crate::handler::MononokeHttpHandler;
use crate::handler::MononokeHttpHandlerAsService;
use crate::socket_data::TlsSocketData;

#[derive(gotham_derive::StateData, Clone)]
struct Empty {}

pub async fn https<H>(
    logger: Logger,
    listener: TcpListener,
    acceptor: SslAcceptor,
    capture_session_data: bool,
    connection_security_checker: ConnectionSecurityChecker,
    handler: MononokeHttpHandler<H>,
) -> Result<(), Error>
where
    H: Handler + Clone + Send + Sync + 'static + RefUnwindSafe,
{
    let connection_security_checker = Arc::new(connection_security_checker);
    let acceptor = Arc::new(acceptor);

    loop {
        let (socket, peer_addr) = listener
            .accept()
            .await
            .context("Error accepting connections")?;

        cloned!(acceptor, logger, handler, connection_security_checker);

        let task = async move {
            let ssl = Ssl::new(acceptor.context()).context("Error creating Ssl")?;
            let ssl_socket = SslStream::new(ssl, socket).context("Error creating SslStream")?;
            let mut ssl_socket = Box::pin(ssl_socket);

            ssl_socket
                .as_mut()
                .accept()
                .await
                .context("Error performing TLS handshake")?;

            let tls_socket_data = TlsSocketData::from_ssl(
                ssl_socket.ssl(),
                connection_security_checker.as_ref(),
                capture_session_data,
            )
            .await;

            let service: MononokeHttpHandlerAsService<_, Empty> = handler
                .clone()
                .into_service(peer_addr, Some(tls_socket_data));

            let ssl_socket = QuietShutdownStream::new(ssl_socket);

            Http::new()
                .serve_connection(ssl_socket, service)
                .await
                .context("Error serving connection")?;

            Result::<_, Error>::Ok(())
        };

        tokio::spawn(task.map_err(move |e| {
            warn!(&logger, "HTTPS Server error: {:?}", e);
        }));
    }
}

pub async fn http<H>(
    logger: Logger,
    listener: TcpListener,
    handler: MononokeHttpHandler<H>,
) -> Result<(), Error>
where
    H: Handler + Clone + Send + Sync + 'static + RefUnwindSafe,
{
    loop {
        let (socket, peer_addr) = listener
            .accept()
            .await
            .context("Error accepting connections")?;

        cloned!(logger, handler);

        let task = async move {
            let service: MononokeHttpHandlerAsService<_, Empty> =
                handler.clone().into_service(peer_addr, None);

            let socket = QuietShutdownStream::new(socket);

            Http::new()
                .serve_connection(socket, service)
                .await
                .context("Error serving connection")?;

            Result::<_, Error>::Ok(())
        };

        tokio::spawn(task.map_err(move |e| {
            warn!(&logger, "HTTP Server error: {:?}", e);
        }));
    }
}
