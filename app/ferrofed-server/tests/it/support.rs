// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a capturing log writer, test settings, a test state and the
//! typed shapes the tests read the server's JSON with.

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::state::AppState;
use http::{Request, Response, StatusCode};
use serde::Deserialize;
use std::error::Error as StdError;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tower::ServiceExt as _;
use tracing_subscriber::fmt::MakeWriter;

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
    /// The request id of a request line.
    pub(crate) request_id: Option<String>,
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

/// The body this server answers a refusal with: the ITS-REST `Error` with the
/// stable code and the request id, and nothing else.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ErrorBody {
    /// The ITS-REST message.
    pub(crate) message: String,
    /// The ITS-REST validation errors.
    #[serde(rename = "validationErrors")]
    pub(crate) validation_errors: Vec<String>,
    /// The stable error code.
    pub(crate) code: String,
    /// The request id.
    pub(crate) request_id: String,
}
