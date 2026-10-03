// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a capturing log writer, test settings, a test state, the
//! typed shapes the tests read the server's JSON with, a mock node's routes
//! and what it was asked, and the reads of a routed answer.

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::error::{CODE_MEMBER, REQUEST_ID_MEMBER};
use ferrofed_server::state::AppState;
use http::{HeaderMap, Request, Response, StatusCode};
use openehr_federation::headers::{ENDPOINT, SYSTEM_ID};
use openehr_its::rest::generated::common::Error;
use serde::Deserialize;
use std::error::Error as StdError;
use std::io::{self, Write};
use std::num::TryFromIntError;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tower::ServiceExt as _;
use tracing_subscriber::fmt::MakeWriter;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The time a loaded host may add to any wait a test makes.
///
/// A bound on elapsed time sits this far past what the code under test
/// should take, and a node meant to be abandoned stays silent at least this
/// far past the bound, so neither side of the claim depends on how busy the
/// machine is.
pub(crate) const SLACK: Duration = Duration::from_secs(3);

/// Returns `duration` in whole milliseconds, as the configuration spells it.
pub(crate) fn millis(duration: Duration) -> Result<u64, TryFromIntError> {
    u64::try_from(duration.as_millis())
}

/// A `tracing` writer that keeps every line in memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Returns everything written so far.
    pub(crate) fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// The writer [`Logs`] hands out.
#[derive(Debug)]
pub(crate) struct LogsWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = LogsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogsWriter(Arc::clone(&self.0))
    }
}

/// One JSON log line, read for the fields the tests assert on.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct LogLine {
    /// The event message.
    pub(crate) message: String,
    /// The HTTP method of a request line.
    pub(crate) method: Option<String>,
    /// The matched route of a request line.
    pub(crate) route: Option<String>,
    /// The status of a request line.
    pub(crate) status: Option<u16>,
    /// The latency of a request line.
    pub(crate) latency_ms: Option<f64>,
    /// The logged query pairs of a request line.
    pub(crate) query: Option<String>,
    /// The request id of a request line: the gateway's outbound id.
    pub(crate) request_id: Option<String>,
    /// Whether the client named its request, on a request line.
    pub(crate) client_named: Option<bool>,
    /// Where a thread panicked, on the panic hook's line.
    pub(crate) location: Option<String>,
}

/// Returns every JSON line in `text`, in order.
pub(crate) fn lines(text: &str) -> Result<Vec<LogLine>, serde_json::Error> {
    text.lines().map(serde_json::from_str).collect()
}

/// Returns the request lines in `text`.
pub(crate) fn request_lines(text: &str) -> Result<Vec<LogLine>, serde_json::Error> {
    Ok(lines(text)?
        .into_iter()
        .filter(|line| line.message == "request")
        .collect())
}

/// Returns server settings a test drives the middleware with.
pub(crate) fn settings() -> ServerSettings {
    ServerSettings {
        listen: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        base_path: ferrofed_server::base_path::BasePath::default(),
        request_timeout: Duration::from_secs(5),
        shutdown_timeout: Duration::from_secs(5),
        body_limit: 1024,
    }
}

/// Returns a state with no indicator.
pub(crate) fn state() -> Arc<AppState> {
    Arc::new(AppState::default())
}

/// The application under test, with no indicator registered.
pub(crate) fn app() -> Router {
    ferrofed_server::router(state(), &settings())
}

/// Sends `request` through `app` and returns the whole response.
pub(crate) async fn send(
    app: Router,
    request: Request<Body>,
) -> Result<Response<Body>, Box<dyn StdError>> {
    Ok(app.oneshot(request).await?)
}

/// Sends `request` through `app` and reads the status and the body.
pub(crate) async fn call(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn StdError>> {
    let response = send(app, request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

/// What the tests read of an error body.
#[derive(Debug)]
pub(crate) struct ErrorBody {
    /// The ITS-REST message.
    pub(crate) message: String,
    /// The ITS-REST validation errors.
    pub(crate) validation_errors: Vec<String>,
    /// The stable error code.
    pub(crate) code: String,
    /// The request id.
    pub(crate) request_id: String,
}

/// Reads `text` as the generated ITS-REST `Error`, which must carry the
/// stable code and the request id as its only extra members.
pub(crate) fn error_body(text: &str) -> Result<ErrorBody, Box<dyn StdError>> {
    let error: Error = serde_json::from_str(text)?;
    let member = |name: &str| -> Result<String, Box<dyn StdError>> {
        Ok(error
            .additional_properties
            .get(name)
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("the error body carries a string {name}: {text}"))?
            .to_owned())
    };
    let code = member(CODE_MEMBER)?;
    let request_id = member(REQUEST_ID_MEMBER)?;
    if error.additional_properties.len() != 2 {
        return Err(format!("the error body carries only code and request_id: {text}").into());
    }
    Ok(ErrorBody {
        message: error.message,
        validation_errors: error.validation_errors,
        code,
        request_id,
    })
}

/// The status, the headers and the body bytes `app` answers `request` with.
pub(crate) async fn exchange(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), Box<dyn StdError>> {
    let response = send(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, bytes.to_vec()))
}

/// A node answering `verb` at `at` with `answer`, and `404` to the rest.
pub(crate) async fn mount(server: &MockServer, verb: &str, at: String, answer: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(server)
        .await;
}

/// The method and path of every request `server` received, in order.
pub(crate) async fn asked(server: &MockServer) -> Result<Vec<(String, String)>, Box<dyn StdError>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect())
}

/// The value of the field `name` in `headers`, when it is text.
pub(crate) fn field<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Asserts that `headers` name `endpoint` and its node's `system_id` as the
/// ones that acted (§7a.3, N31, §9.6).
pub(crate) fn acted(headers: &HeaderMap, endpoint: &str, system_id: &str) {
    assert_eq!(Some(endpoint), field(headers, ENDPOINT), "N31");
    assert_eq!(Some(system_id), field(headers, SYSTEM_ID), "§9.6");
}

/// Asserts that `request` is refused `400` with `code`, names no acting
/// endpoint, and that neither node received anything.
pub(crate) async fn refused_at_neither(
    app: Router,
    request: Request<Body>,
    code: &str,
    (a, b): (&MockServer, &MockServer),
) -> Result<(), Box<dyn StdError>> {
    let (status, headers, body) = exchange(app, request).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(code, error_body(&text)?.code, "{text}");
    assert_eq!(None, field(&headers, ENDPOINT), "no endpoint acted: {text}");
    assert!(asked(a).await?.is_empty(), "node A received nothing");
    assert!(asked(b).await?.is_empty(), "node B received nothing");
    Ok(())
}
