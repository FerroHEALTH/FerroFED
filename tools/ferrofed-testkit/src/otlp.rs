// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An in-process OTLP collector: the OTLP/gRPC `TraceService` on a loopback
//! port, keeping every export request it receives for a test to read
//! (<https://opentelemetry.io/docs/specs/otlp/>).
//!
//! [`Collector::start`] serves it over TLS, requiring a client certificate,
//! when it is given the synthetic set of [`ListenerCertificates`]. The kit
//! terminates that TLS itself and hands tonic the plain streams, so no crate
//! of the build turns on tonic's own TLS, under which tonic refuses the
//! `https` dial `ferrohealth-telemetry` makes over its own connector.
//!
//! It runs on a thread and a runtime of its own, so a gateway exporting from
//! its batch thread, a `serve` process, or a test runtime that has stopped
//! still reaches it, and it answers every export as accepted. No
//! specification governs this test device: our own design.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::thread::JoinHandle;

use ferrohealth_config::tls::TlsVersion;
use ferrohealth_server::section::{ClientAuth, TlsSection};
use ferrohealth_server::tls::{TlsError, TlsKeys, server_config_of};

use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::{
    TraceService, TraceServiceServer,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::trace::v1::Span;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::transport::server::{Connected, TcpIncoming};

use crate::listener::ListenerCertificates;

/// The export requests received so far, in the order they arrived.
type Received = Arc<Mutex<Vec<ExportTraceServiceRequest>>>;

/// The collector could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CollectorError {
    /// No loopback port could be bound.
    #[error("the collector could not bind a loopback port")]
    Bind(#[source] io::Error),
    /// Its runtime could not be built.
    #[error("the collector's runtime could not be built")]
    Runtime(#[source] io::Error),
    /// Its certificates could not be written for the TLS configuration.
    #[error("the collector's certificates could not be written")]
    Write(#[source] io::Error),
    /// Its TLS configuration was refused.
    #[error("the collector's TLS configuration was refused")]
    Tls(#[source] TlsError),
}

/// An OTLP/gRPC trace collector on a loopback port, stopped when dropped.
#[derive(Debug)]
pub struct Collector {
    endpoint: String,
    received: Received,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<(), tonic::transport::Error>>>,
}

impl Collector {
    /// Starts a collector on a free loopback port, over TLS when
    /// `certificates` is set: it then presents their server certificate,
    /// admits only a client that presents a certificate their client CA
    /// signed, and its endpoint is `https://127.0.0.1:<port>`.
    ///
    /// # Errors
    ///
    /// Returns [`CollectorError::Bind`] when no port can be bound,
    /// [`CollectorError::Runtime`] when the collector's runtime cannot be
    /// built, [`CollectorError::Write`] when the certificates cannot be
    /// written, and [`CollectorError::Tls`] when the TLS configuration is
    /// refused.
    pub fn start(certificates: Option<&ListenerCertificates>) -> Result<Self, CollectorError> {
        let tls = certificates.map(server_tls).transpose()?;
        let scheme = if tls.is_some() { "https" } else { "http" };
        let builder = Server::builder();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(CollectorError::Runtime)?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(CollectorError::Bind)?;
        listener
            .set_nonblocking(true)
            .map_err(CollectorError::Bind)?;
        let address = listener.local_addr().map_err(CollectorError::Bind)?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener).map_err(CollectorError::Bind)?
        };
        let received = Received::default();
        let service = TraceServiceServer::new(Keeper(Arc::clone(&received)));
        let (stop, stopped) = oneshot::channel::<()>();
        // NOTE: tonic's `serve_with_incoming_shutdown` waits for accepted connections to close
        // (tonic 0.14 `Server` docs); a client on the test's blocked runtime never closes one (#1507).
        let thread = std::thread::spawn(move || {
            let mut builder = builder;
            let router = builder.add_service(service);
            // A dropped sender stops the collector as the sent signal does. Dropping the
            // runtime with the thread then drops every connection task it still holds.
            runtime.block_on(async move {
                match tls {
                    None => {
                        let serving = router.serve_with_incoming(TcpIncoming::from(listener));
                        tokio::select! {
                            served = serving => served,
                            _stopped = stopped => Ok(()),
                        }
                    }
                    Some(config) => {
                        let serving = router.serve_with_incoming(accepted(listener, config));
                        tokio::select! {
                            served = serving => served,
                            _stopped = stopped => Ok(()),
                        }
                    }
                }
            })
        });
        Ok(Self {
            endpoint: format!("{scheme}://{address}"),
            received,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The URL an OTLP exporter sends to: `http://`, or `https://` for a
    /// collector started over TLS.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Every export request received so far, in the order they arrived.
    #[must_use]
    pub fn received(&self) -> Vec<ExportTraceServiceRequest> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every span received so far, across every request, resource and
    /// scope.
    #[must_use]
    pub fn spans(&self) -> Vec<Span> {
        self.received()
            .into_iter()
            .flat_map(|request| request.resource_spans)
            .flat_map(|resource| resource.scope_spans)
            .flat_map(|scope| scope.spans)
            .collect()
    }
}

impl Drop for Collector {
    /// Stops the server and joins its thread, re-raising a panic it raised
    /// unless this thread is already panicking. It never waits for a client
    /// to close its connection.
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            // A server that already stopped dropped the receiver; nothing is left to stop.
            let _sent: Result<(), ()> = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && let Err(panic) = thread.join()
            && !std::thread::panicking()
        {
            std::panic::resume_unwind(panic);
        }
    }
}

/// The server configuration of `certificates`, built by `ferrohealth-server`
/// as a listener's is: their server certificate, a client certificate their
/// client CA signed required, and the ALPN protocol `h2` of gRPC.
fn server_tls(certificates: &ListenerCertificates) -> Result<Arc<ServerConfig>, CollectorError> {
    let dir = tempfile::tempdir().map_err(CollectorError::Write)?;
    let write = |name: &str, pem: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, pem)
            .map(|()| path)
            .map_err(CollectorError::Write)
    };
    let section = TlsSection {
        certificate_file: write("server.pem", certificates.chain())?,
        key_file: write("server.key", certificates.key())?,
        client_auth: ClientAuth::Required,
        client_ca_file: Some(write("client-ca.pem", certificates.client_ca())?),
        reload_interval: None,
        healthcheck_identity_file: None,
        min_version: TlsVersion::Tls12,
    };
    let mut config = Arc::unwrap_or_clone(
        server_config_of(&section, TlsKeys::SERVER).map_err(CollectorError::Tls)?,
    );
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(Arc::new(config))
}

/// The TLS connections `listener` accepts under `config`, each handshake on
/// a task of its own; a refused handshake is dropped, as the client sees.
fn accepted(
    listener: tokio::net::TcpListener,
    config: Arc<ServerConfig>,
) -> ReceiverStream<Result<Accepted, io::Error>> {
    let (connections, incoming) = mpsc::channel(16);
    let acceptor = TlsAcceptor::from(config);
    tokio::spawn(async move {
        while let Ok((tcp, _peer)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let connections = connections.clone();
            tokio::spawn(async move {
                if let Ok(stream) = acceptor.accept(tcp).await {
                    // A server that stopped dropped the receiver; the connection goes with it.
                    let _sent = connections.send(Ok(Accepted(stream))).await;
                }
            });
        }
    });
    ReceiverStream::new(incoming)
}

/// One TLS connection the collector accepted, handed to tonic as plain IO.
#[derive(Debug)]
struct Accepted(tokio_rustls::server::TlsStream<tokio::net::TcpStream>);

impl Connected for Accepted {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

impl AsyncRead for Accepted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(context, buffer)
    }
}

impl AsyncWrite for Accepted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(context)
    }
}

/// The trace service: keeps each export request and accepts it whole.
#[derive(Debug)]
struct Keeper(Received);

#[async_trait::async_trait]
impl TraceService for Keeper {
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.into_inner());
        Ok(tonic::Response::new(ExportTraceServiceResponse::default()))
    }
}
