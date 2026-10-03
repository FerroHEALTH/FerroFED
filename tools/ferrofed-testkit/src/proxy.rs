// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The capturing and fault proxy that stands in front of each node.
//!
//! A test reaches a node through its [`CapturingProxy`], never directly. The
//! proxy journals every request it receives (method, path, query, headers and
//! body) before it decides what to do, and a test reads that journal to judge
//! what crossed the wire on the node's side. Track 10 is judged there and not
//! on the gateway's own logs, because "a gateway that sanitises its logs but
//! not its dispatches passes the wrong test" (Federation Tier with AQL §16.3).
//!
//! [`Fault`] selects what the proxy does with a request, per proxy and at any
//! point in a test: forward it unmodified, refuse the connection, delay it, or
//! answer with a status of the test's choosing. Together with stopping the
//! node's container these produce the endpoint statuses of §11.1 that need a
//! misbehaving node (§16).
//!
//! No specification governs the proxy itself; it is FerroFED's own design.

use bytes::Bytes;
use http::header::{self, HeaderMap, HeaderName};
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// What the proxy does with the next request it receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    /// Forward the request to the node and return its answer unmodified.
    #[default]
    Forward,
    /// Close the connection without an answer, as an unreachable node does.
    Refuse,
    /// Wait this long, then forward the request, as a slow node does.
    Delay(Duration),
    /// Answer with this status and an empty body without forwarding, as a
    /// failing node does.
    Status(StatusCode),
}

/// One request as the proxy received it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// The request method, for example `POST`.
    pub method: String,
    /// The request path, without the query.
    pub path: String,
    /// The raw query string, when the request carried one.
    pub query: Option<String>,
    /// Every header in arrival order, values as raw bytes.
    pub headers: Vec<(String, Vec<u8>)>,
    /// The request body, byte for byte.
    pub body: Vec<u8>,
}

impl Capture {
    /// Returns whether `needle` occurs anywhere in this request: the path,
    /// the query, a header name or value, or the body.
    ///
    /// This is the identifier-leakage test of track 10, which passes on zero
    /// occurrences in any carrier.
    #[must_use]
    pub fn contains(&self, needle: &[u8]) -> bool {
        if needle.is_empty() {
            return true;
        }
        let found = |haystack: &[u8]| {
            haystack
                .windows(needle.len())
                .any(|window| window == needle)
        };
        found(self.path.as_bytes())
            || self
                .query
                .as_deref()
                .is_some_and(|query| found(query.as_bytes()))
            || self
                .headers
                .iter()
                .any(|(name, value)| found(name.as_bytes()) || found(value))
            || found(&self.body)
    }
}

/// The proxy could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProxyError {
    /// The listening socket could not be bound or read.
    #[error("the proxy could not bind its listening socket")]
    Bind(#[source] std::io::Error),
    /// The forwarding client could not be built.
    #[error("the proxy could not build its forwarding client")]
    Client(#[source] reqwest::Error),
}

/// The state shared by the accept loop, every connection, and the test.
#[derive(Debug)]
struct Shared {
    /// The origin of the node the proxy forwards to, with no path.
    upstream: String,
    /// The forwarding client.
    client: reqwest::Client,
    /// Every request received, in arrival order.
    journal: Mutex<Vec<Capture>>,
    /// What the proxy does with the next request.
    fault: Mutex<Fault>,
    /// How many connections were closed unread under [`Fault::Refuse`].
    refused: AtomicUsize,
}

impl Shared {
    fn fault(&self) -> Fault {
        *self.fault.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A capturing and fault-injecting reverse proxy in front of one node,
/// stopped when it is dropped.
#[derive(Debug)]
pub struct CapturingProxy {
    /// The origin the proxy listens on, with no path.
    origin: String,
    /// The state the accept loop shares.
    shared: Arc<Shared>,
    /// The accept loop, aborted on drop.
    task: JoinHandle<()>,
}

/// The refusal a connection handler returns, which makes the HTTP layer close
/// the connection without writing an answer.
#[derive(Debug, thiserror::Error)]
#[error("the proxy refused the request")]
struct Refused;

impl CapturingProxy {
    /// Starts a proxy on a free loopback port that forwards to `upstream`,
    /// the node's origin with no path (`http://host:port`).
    ///
    /// # Errors
    ///
    /// Returns [`ProxyError::Bind`] when no loopback port can be bound and
    /// [`ProxyError::Client`] when the forwarding client cannot be built.
    pub async fn start(upstream: impl Into<String>) -> Result<Self, ProxyError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(ProxyError::Bind)?;
        let address = listener.local_addr().map_err(ProxyError::Bind)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(ProxyError::Client)?;
        let shared = Arc::new(Shared {
            upstream: upstream.into().trim_end_matches('/').to_owned(),
            client,
            journal: Mutex::new(Vec::new()),
            fault: Mutex::new(Fault::Forward),
            refused: AtomicUsize::new(0),
        });
        let task = tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        Ok(Self {
            origin: format!("http://{address}"),
            shared,
            task,
        })
    }

    /// Returns the origin the proxy listens on, with no path.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the origin of the node behind the proxy.
    #[must_use]
    pub fn upstream(&self) -> &str {
        &self.shared.upstream
    }

    /// Selects what the proxy does with every request from now on.
    pub fn set_fault(&self, fault: Fault) {
        *self
            .shared
            .fault
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = fault;
    }

    /// Returns to forwarding every request unmodified.
    pub fn clear_fault(&self) {
        self.set_fault(Fault::Forward);
    }

    /// Returns a copy of every request received so far, in arrival order.
    #[must_use]
    pub fn journal(&self) -> Vec<Capture> {
        self.shared
            .journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns how many connections the proxy has closed before reading a
    /// byte, because they arrived under [`Fault::Refuse`].
    ///
    /// Such a connection leaves nothing in the journal, so this count is
    /// how a test tells that nobody tried to reach a node that is down.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.shared.refused.load(Ordering::SeqCst)
    }

    /// Forgets every request received so far.
    pub fn clear_journal(&self) {
        self.shared
            .journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Returns whether `needle` occurs in any request received so far.
    #[must_use]
    pub fn journal_contains(&self, needle: &[u8]) -> bool {
        self.shared
            .journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|capture| capture.contains(needle))
    }
}

impl Drop for CapturingProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Accepts connections until the proxy is dropped.
///
/// A connection that arrives while the fault is [`Fault::Refuse`] is closed
/// before a byte is read; one that is already open is closed by its handler.
async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        if shared.fault() == Fault::Refuse {
            shared.refused.fetch_add(1, Ordering::SeqCst);
            drop(stream);
            continue;
        }
        let connection_shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let service =
                service_fn(move |request| handle(Arc::clone(&connection_shared), request));
            // A connection ends in an error whenever a client hangs up or the
            // handler refuses it; both are outcomes a test causes on purpose.
            match http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                Ok(()) | Err(_) => {}
            }
        });
    }
}

/// Journals one request, then forwards, delays, answers or refuses it.
async fn handle(
    shared: Arc<Shared>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Refused> {
    let (parts, body) = request.into_parts();
    let body = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Ok(status(StatusCode::BAD_REQUEST)),
    };
    shared
        .journal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Capture {
            method: parts.method.to_string(),
            path: parts.uri.path().to_owned(),
            query: parts.uri.query().map(str::to_owned),
            headers: parts
                .headers
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
                .collect(),
            body: body.to_vec(),
        });

    match shared.fault() {
        Fault::Refuse => return Err(Refused),
        Fault::Status(code) => return Ok(status(code)),
        Fault::Delay(delay) => tokio::time::sleep(delay).await,
        Fault::Forward => {}
    }

    let target = format!(
        "{}{}",
        shared.upstream,
        parts
            .uri
            .path_and_query()
            .map_or("/", http::uri::PathAndQuery::as_str)
    );
    let forwarded = shared
        .client
        .request(parts.method, target)
        .headers(end_to_end(&parts.headers))
        .body(body)
        .send()
        .await;
    let Ok(answer) = forwarded else {
        return Ok(status(StatusCode::BAD_GATEWAY));
    };
    let code = answer.status();
    let headers = end_to_end(answer.headers());
    let Ok(bytes) = answer.bytes().await else {
        return Ok(status(StatusCode::BAD_GATEWAY));
    };
    let mut response = Response::new(Full::new(bytes));
    *response.status_mut() = code;
    *response.headers_mut() = headers;
    Ok(response)
}

/// Returns an empty answer with `code`.
fn status(code: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = code;
    response
}

/// The connection-specific fields of RFC 9110 §7.6.1, plus `Host` and
/// `Content-Length`, which the proxy recomputes for its own hop.
const NOT_FORWARDED: [HeaderName; 6] = [
    header::CONNECTION,
    header::TE,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
    header::HOST,
    header::CONTENT_LENGTH,
];

/// The connection-specific fields RFC 9110 §7.6.1 names that `http` has no
/// constant for.
const NOT_FORWARDED_BY_NAME: [&str; 2] = ["keep-alive", "proxy-connection"];

/// Returns `headers` without the fields that must not cross a proxy.
fn end_to_end(headers: &HeaderMap) -> HeaderMap {
    let mut kept = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        if !NOT_FORWARDED.contains(name) && !NOT_FORWARDED_BY_NAME.contains(&name.as_str()) {
            kept.append(name.clone(), value.clone());
        }
    }
    kept
}
