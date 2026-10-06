// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS a listener serves when `[server.tls]` or `[metrics.tls]` is set.
//!
//! A listener without it serves plain HTTP, for a proxy or a service mesh
//! that terminates TLS in front of the gateway. With it, [`TlsListener`]
//! accepts each TCP connection and completes its TLS handshake before the
//! HTTP server reads a byte, over the configuration [`certificates`] holds,
//! which `SIGHUP` reads again from the same files. Each handshake runs on a
//! task of its own, bounded by [`HANDSHAKE_TIMEOUT`], so a client that stalls
//! in its handshake holds up no other connection. No specification governs
//! the listener: our own design.

pub mod certificates;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::server::TlsStream;

use crate::listener::certificates::Certificates;

/// How long a client may take to complete its TLS handshake.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How many completed handshakes wait for the HTTP server to take them.
const ACCEPTED: usize = 64;

/// A listener whose connections are TLS, handshaken before the HTTP server
/// reads them.
///
/// It stops accepting, and closes its socket, once the HTTP server drops
/// it, as the drain does when the listener closes.
#[derive(Debug)]
pub struct TlsListener {
    local: SocketAddr,
    accepted: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
}

impl TlsListener {
    /// Serves TLS over `listener` with the configuration `certificates`
    /// holds at each handshake.
    ///
    /// It must be called inside a Tokio runtime, which runs the accepting
    /// task.
    ///
    /// # Errors
    ///
    /// The I/O error of reading the listener's local address.
    pub fn new(listener: TcpListener, certificates: Arc<Certificates>) -> std::io::Result<Self> {
        let local = listener.local_addr()?;
        let (sender, accepted) = mpsc::channel(ACCEPTED);
        tokio::spawn(accept(listener, certificates, sender));
        Ok(Self { local, accepted })
    }
}

/// The address a connection came from, read the same way from a plain
/// [`TcpListener`] and from a [`TlsListener`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer(pub SocketAddr);

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, TcpListener>> for Peer {
    fn connect_info(stream: axum::serve::IncomingStream<'_, TcpListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, TlsListener>> for Peer {
    fn connect_info(stream: axum::serve::IncomingStream<'_, TlsListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.accepted.recv().await {
            Some(connection) => connection,
            // NOTE: no specification governs this: our own design; the
            // accepting task ends only once this receiver is gone, so no
            // connection can follow.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

/// Accepts connections on `listener` and hands each one whose handshake
/// completes to `sender`, until its receiver is dropped.
async fn accept(
    listener: TcpListener,
    certificates: Arc<Certificates>,
    sender: mpsc::Sender<(TlsStream<TcpStream>, SocketAddr)>,
) {
    loop {
        let (stream, peer) = tokio::select! {
            () = sender.closed() => return,
            accepted = listener.accept() => match accepted {
                Ok(connection) => connection,
                Err(error) => {
                    refused(&error).await;
                    continue;
                }
            },
        };
        let acceptor = certificates.acceptor();
        let sender = sender.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
                Ok(Ok(connection)) => {
                    if sender.send((connection, peer)).await.is_err() {
                        tracing::debug!("a TLS connection arrived as the listener closed");
                    }
                }
                Ok(Err(error)) => tracing::debug!(%error, "a TLS handshake failed"),
                Err(_elapsed) => tracing::debug!(
                    timeout_ms = HANDSHAKE_TIMEOUT.as_millis(),
                    "a TLS handshake did not finish in time"
                ),
            }
        });
    }
}

/// Waits out an `accept` that failed: a connection the peer gave up on is
/// skipped at once, and any other failure, such as too many open files, is
/// logged and waited on for a second, as axum's own listener does.
async fn refused(error: &std::io::Error) {
    use std::io::ErrorKind;

    if matches!(
        error.kind(),
        ErrorKind::ConnectionRefused | ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
    ) {
        return;
    }
    tracing::error!(%error, "the TLS listener could not accept a connection");
    tokio::time::sleep(Duration::from_secs(1)).await;
}
