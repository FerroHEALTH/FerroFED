// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The live parts of the report, asked of the gateway running on this host:
//! readiness, the dependency states, the integrity incidents and the
//! metrics.
//!
//! Each is asked on loopback, the way `ferrofed healthcheck` asks, through
//! `ferrohealth_server::healthcheck::Get`, with the same timeout and the
//! same TLS: the listener's own certificate and nothing else. An answer is read up to a bound and kept
//! only when it is the document the route serves; any other outcome is a
//! [`SourceError`] the manifest names, never an empty file. No
//! specification governs the report: our own design.

use std::time::Duration;

use ferrohealth_server::healthcheck::{self, Get, GetError};
use ferrohealth_server::tls::{HealthcheckTls, TlsError};
use http::{HeaderValue, StatusCode, header};
use secrecy::{ExposeSecret, SecretString};

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
    Tls(#[source] TlsError),
    /// The bearer token makes no `Authorization` header value.
    #[error("the operator token makes no Authorization header")]
    Bearer(#[source] header::InvalidHeaderValue),
    /// The request could not be made or was not answered.
    #[error("GET {path} was not answered")]
    Request {
        /// The path asked.
        path: String,
        /// Why.
        #[source]
        source: GetError,
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

impl SourceError {
    /// Returns why the part is missing, as the manifest records it.
    ///
    /// A body that does not read is named without its cause, because a
    /// parser's message can quote the body.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::NotDocument { .. } | Self::NotText { .. } | Self::Status { .. } => {
                self.to_string()
            }
            Self::NotServed(_)
            | Self::Tls(_)
            | Self::Bearer(_)
            | Self::Request { .. }
            | Self::TooLong { .. } => crate::chain(self),
        }
    }
}

/// One listener on this host and how to ask it.
#[derive(Debug)]
pub struct Listener {
    address: std::net::SocketAddr,
    tls: Option<HealthcheckTls>,
    timeout: Duration,
}

impl Listener {
    /// Returns the listener bound to `listen`, asked over `tls` when it
    /// serves it.
    ///
    /// # Errors
    /// [`SourceError::Tls`] for TLS material that does not read.
    pub fn new(
        listen: std::net::SocketAddr,
        tls: Option<&crate::config::server::ListenerTlsSettings>,
        timeout: Duration,
    ) -> Result<Self, SourceError> {
        let tls = tls
            .map(|tls| HealthcheckTls::read_of(&tls.section, tls.keys))
            .transpose()
            .map_err(SourceError::Tls)?;
        Ok(Self {
            address: healthcheck::target(listen),
            tls,
            timeout,
        })
    }

    /// Asks `GET path`, with `bearer` when given, and returns the status
    /// and the body.
    ///
    /// # Errors
    /// [`SourceError::Bearer`] for a token that makes no header value,
    /// [`SourceError::Request`] when no answer arrives, and
    /// [`SourceError::TooLong`] for a body past [`ANSWER_LIMIT`].
    pub async fn get(
        &self,
        path: &str,
        bearer: Option<&SecretString>,
    ) -> Result<(StatusCode, Vec<u8>), SourceError> {
        let mut get = Get::new(self.address, path, self.timeout, ANSWER_LIMIT);
        if let Some(tls) = &self.tls {
            get = get.with_tls(tls.clone());
        }
        if let Some(token) = bearer {
            let mut value = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
                .map_err(SourceError::Bearer)?;
            value.set_sensitive(true);
            get = get.with_header(header::AUTHORIZATION, value);
        }
        match get.send().await {
            Ok(answer) => Ok((answer.status(), answer.into_body())),
            Err(GetError::TooLong { .. }) => Err(SourceError::TooLong {
                path: path.to_owned(),
            }),
            Err(source) => Err(SourceError::Request {
                path: path.to_owned(),
                source,
            }),
        }
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

/// Returns `body` when it is UTF-8 text.
///
/// # Errors
/// [`SourceError::NotText`] when it is not.
pub fn text(path: &str, body: Vec<u8>) -> Result<String, SourceError> {
    String::from_utf8(body).map_err(|source| SourceError::NotText {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{SourceError, document};

    #[test]
    fn a_body_that_does_not_read_is_never_quoted_in_the_reason() {
        let error = document::<crate::report::answers::Incidents>(
            "/operator/incidents",
            br#"{"counts":"CANARY-BODY","recent":[]}"#,
        )
        .expect_err("not an incident report");
        assert!(matches!(error, SourceError::NotDocument { .. }));
        assert!(
            crate::chain(&error).contains("CANARY-BODY"),
            "the parser quotes the body, which is why the reason leaves it out"
        );
        assert!(!error.reason().contains("CANARY"), "{}", error.reason());
    }
}
