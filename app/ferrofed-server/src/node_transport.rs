// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP engine every request to a node and its token endpoint goes through.
//!
//! It is `reqwest`, with redirects off, the per-request timeout a call's
//! deadline only shortens, and a bound on what it reads of one answer.
//!
//! An answer whose `Content-Length` is past the bound is refused unread, and
//! one without that header is read chunk by chunk and dropped the moment it
//! passes the bound, so a member that answers without end holds at most the
//! bound in memory. Either way the call ends in
//! [`Oversized`], which the engine reads as the node's answer: `node-error`
//! with its status (§11.1). No specification governs the bound: our own
//! design.

use std::time::Duration;

use async_trait::async_trait;
use ferrofed_engine::dispatch::oversized::Oversized;
use openehr_its::rest::client::{RequestTimeout, Transport, TransportError};

/// The `reqwest` engine of the node clients, reading at most `limit` bytes
/// of each answer.
///
/// `Debug` leaves out the `reqwest` client, whose default headers may hold a
/// credential.
#[derive(Clone)]
pub struct BoundedTransport {
    client: reqwest::Client,
    // The engine's own per-request timeout, which a request deadline only
    // ever shortens.
    timeout: Duration,
    limit: usize,
}

impl std::fmt::Debug for BoundedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundedTransport")
            .field("timeout", &self.timeout)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl BoundedTransport {
    /// Returns an engine over a `reqwest` client the caller configures (TLS
    /// roots, a client certificate), built here with redirects off,
    /// `timeout` per request and at most `limit` bytes read of an answer.
    ///
    /// # Errors
    ///
    /// Returns the `reqwest` error when the client cannot be built.
    pub fn with_builder(
        builder: reqwest::ClientBuilder,
        timeout: Duration,
        limit: usize,
    ) -> Result<Self, reqwest::Error> {
        // NOTE: RFC 9110 §15.4: following a redirect re-sends the request, credentials and all,
        // to a host the registry never named, so a 3xx reaches the engine's caller as received.
        let client = builder
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            timeout,
            limit,
        })
    }

    /// Returns an engine with the default client, as
    /// [`BoundedTransport::with_builder`] builds it.
    ///
    /// # Errors
    ///
    /// Returns the `reqwest` error when the TLS backend cannot be
    /// initialised.
    pub fn new(timeout: Duration, limit: usize) -> Result<Self, reqwest::Error> {
        Self::with_builder(reqwest::Client::builder(), timeout, limit)
    }

    /// The most bytes this engine reads of one answer.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }
}

/// The engine's error for a `reqwest` failure: a timeout, or any other.
fn failed(source: reqwest::Error) -> TransportError {
    if source.is_timeout() {
        TransportError::Timeout {
            source: Box::new(source),
        }
    } else {
        TransportError::Send {
            source: Box::new(source),
        }
    }
}

// TODO(#714): use the body bound of openehr-its ReqwestTransport once a release carries one.
#[async_trait]
impl Transport for BoundedTransport {
    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        let budget = request.extensions().get::<RequestTimeout>().map(|t| t.0);
        let mut request =
            reqwest::Request::try_from(request).map_err(|source| TransportError::Send {
                source: Box::new(source),
            })?;
        if let Some(budget) = budget {
            *request.timeout_mut() = Some(self.timeout.min(budget));
        }
        let mut response = self.client.execute(request).await.map_err(failed)?;
        let status = response.status();
        let oversized = || TransportError::Send {
            source: Box::new(Oversized::new(status, self.limit)),
        };
        let declared = response
            .content_length()
            .map(|length| usize::try_from(length).unwrap_or(usize::MAX));
        if declared.is_some_and(|length| length > self.limit) {
            return Err(oversized());
        }
        let mut body = Vec::with_capacity(declared.unwrap_or_default());
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if body.len().saturating_add(chunk.len()) > self.limit {
                return Err(oversized());
            }
            body.extend_from_slice(&chunk);
        }
        let mut answer = http::Response::new(body);
        *answer.status_mut() = status;
        *answer.headers_mut() = response.headers().clone();
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::BoundedTransport;
    use ferrofed_engine::dispatch::oversized::Oversized;
    use http::StatusCode;
    use openehr_its::rest::client::{Transport, TransportError};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A server that answers one request `200` with a chunked body that
    /// never ends, and returns its address.
    async fn endless() -> Result<std::net::SocketAddr, Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut request = [0_u8; 4096];
            let _read = stream.read(&mut request).await;
            let head = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\n\r\n";
            if stream.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            let chunk = format!("{:x}\r\n{}\r\n", 4096, " ".repeat(4096));
            while stream.write_all(chunk.as_bytes()).await.is_ok() {}
        });
        Ok(address)
    }

    /// The [`Oversized`] a send ended in, or `None`.
    fn bound_of(error: &TransportError) -> Option<Oversized> {
        match error {
            TransportError::Send { source } => source.downcast_ref::<Oversized>().copied(),
            TransportError::Timeout { .. } => None,
        }
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn an_answer_without_end_is_dropped_at_the_bound() -> TestResult {
        let address = endless().await?;
        let transport = BoundedTransport::new(Duration::from_secs(10), 64 * 1024)?;
        let request = http::Request::get(format!("http://{address}/v1/ehr")).body(Vec::new())?;
        let started = std::time::Instant::now();
        let Err(error) = transport.send(request).await else {
            return Err("an endless answer was read whole".into());
        };
        assert_eq!(
            Some(Oversized::new(StatusCode::OK, 64 * 1024)),
            bound_of(&error),
            "{error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the read stopped at the bound, not at the engine's timeout"
        );
        Ok(())
    }
}
