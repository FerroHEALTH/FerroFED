// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The live parts of the report, asked of the gateway running on this host:
//! readiness, the dependency states, the integrity incidents and the
//! metrics.
//!
//! Each is asked on loopback, the way `ferrofed healthcheck` asks, with the
//! same timeout and the same TLS. An answer is read up to a bound and kept
//! only when it is the document the route serves; any other outcome is a
//! [`SourceError`] the manifest names, never an empty file. No
//! specification governs the report: our own design.

use std::time::Duration;

use http::{StatusCode, header};
use secrecy::{ExposeSecret, SecretString};

use crate::healthcheck::{self, Tls};

/// The most bytes read of any one answer.
pub const ANSWER_LIMIT: usize = 4 * 1024 * 1024;

/// What a live part could not be read for.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    /// The configuration runs no listener the part is served on.
    #[error("{0}")]
    NotServed(&'static str),
    /// The listener's TLS material could not be read.
    #[error("the TLS material of the listener could not be read")]
    Tls(#[source] crate::listener::certificates::CertificateError),
    /// The HTTP client could not be built.
    #[error("the HTTP client could not be built")]
    Client(#[source] reqwest::Error),
    /// The request could not be made or was not answered.
    #[error("GET {path} was not answered")]
    Request {
        /// The path asked.
        path: String,
        /// Why.
        #[source]
        source: reqwest::Error,
    },
    /// The gateway answered with a status the part is not served with.
    #[error("GET {path} answered {status}{hint}")]
    Status {
        /// The path asked.
        path: String,
        /// The status.
        status: StatusCode,
        /// What to do about it, or empty.
        hint: &'static str,
    },
    /// The answer is longer than [`ANSWER_LIMIT`].
    #[error("GET {path} answered more than {ANSWER_LIMIT} bytes")]
    TooLong {
        /// The path asked.
        path: String,
    },
    /// The answer is not the JSON document the route serves.
    #[error("GET {path} answered a body that is not its document")]
    NotDocument {
        /// The path asked.
        path: String,
        /// Why it does not read.
        #[source]
        source: serde_json::Error,
    },
    /// The answer is not UTF-8 text.
    #[error("GET {path} answered a body that is not text")]
    NotText {
        /// The path asked.
        path: String,
        /// Why it does not read.
        #[source]
        source: std::string::FromUtf8Error,
    },
}

/// One listener on this host and how to ask it.
#[derive(Debug)]
pub struct Listener {
    client: reqwest::Client,
    origin: String,
}

impl Listener {
    /// Returns the listener bound to `listen`, asked over `tls` when it
    /// serves it.
    ///
    /// # Errors
    /// [`SourceError::Tls`] for TLS material that does not read, and
    /// [`SourceError::Client`] when the client cannot be built.
    pub fn new(
        listen: std::net::SocketAddr,
        tls: Option<&crate::listener::certificates::TlsFiles>,
        timeout: Duration,
    ) -> Result<Self, SourceError> {
        let tls = tls.map(Tls::read).transpose().map_err(SourceError::Tls)?;
        let (client, scheme) =
            healthcheck::client(timeout, tls.as_ref()).map_err(SourceError::Client)?;
        let address = healthcheck::target(listen);
        Ok(Self {
            client,
            origin: format!("{scheme}://{address}"),
        })
    }

    /// Asks `GET path`, with `bearer` when given, and returns the status
    /// and the body.
    ///
    /// # Errors
    /// [`SourceError::Request`] when no answer arrives, and
    /// [`SourceError::TooLong`] for a body past [`ANSWER_LIMIT`].
    pub async fn get(
        &self,
        path: &str,
        bearer: Option<&SecretString>,
    ) -> Result<(StatusCode, Vec<u8>), SourceError> {
        let failed = |source| SourceError::Request {
            path: path.to_owned(),
            source,
        };
        let mut request = self.client.get(format!("{}{path}", self.origin));
        if let Some(token) = bearer {
            request = request.header(
                header::AUTHORIZATION,
                format!("Bearer {}", token.expose_secret()),
            );
        }
        let mut response = request.send().await.map_err(failed)?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if body.len().saturating_add(chunk.len()) > ANSWER_LIMIT {
                return Err(SourceError::TooLong {
                    path: path.to_owned(),
                });
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, body))
    }
}

/// Returns `body`, answered to `GET path`, when it reads as the document
/// `T`, written again as pretty JSON.
///
/// # Errors
/// [`SourceError::NotDocument`] when it does not.
pub fn document<T>(path: &str, body: &[u8]) -> Result<Vec<u8>, SourceError>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let not_document = |source| SourceError::NotDocument {
        path: path.to_owned(),
        source,
    };
    let read: T = serde_json::from_slice(body).map_err(not_document)?;
    let mut written = serde_json::to_vec_pretty(&read).map_err(not_document)?;
    written.push(b'\n');
    Ok(written)
}

/// Returns `body` when it is JSON, unchanged.
///
/// # Errors
/// [`SourceError::NotDocument`] when it is not.
pub fn json(path: &str, body: Vec<u8>) -> Result<Vec<u8>, SourceError> {
    match serde_json::from_slice::<serde::de::IgnoredAny>(&body) {
        Ok(_) => Ok(body),
        Err(source) => Err(SourceError::NotDocument {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Returns `body` when it is UTF-8 text, unchanged.
///
/// # Errors
/// [`SourceError::NotText`] when it is not.
pub fn text(path: &str, body: Vec<u8>) -> Result<Vec<u8>, SourceError> {
    String::from_utf8(body)
        .map(String::into_bytes)
        .map_err(|source| SourceError::NotText {
            path: path.to_owned(),
            source,
        })
}
