// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node's answer longer than the gateway reads of one answer.
//!
//! The HTTP engine a node client sends through reads at most a bound the
//! deployment sets of each answer (`ReqwestTransport::with_max_body`), and
//! the client ends a longer one unread with [`ClientError::AnswerTooLarge`].
//! The node was reached and answered, and nothing usable came back, so the
//! endpoint is `node-error` (§11.1), never `offline`: it fails the query
//! with `424` under all-or-nothing and clears `meta.federation.complete`
//! (§11.4). No specification governs the bound itself: our own design.

use http::StatusCode;
use openehr_its::rest::client::ClientError;

/// An answer of the node's whose body is longer than the bound, so the
/// engine stopped reading it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "the node answered {status} with a body longer than the {limit} bytes the gateway reads of one answer, so the answer was not read"
)]
pub struct Oversized {
    status: StatusCode,
    limit: usize,
}

impl Oversized {
    /// An answer with `status` whose body ran past `limit` bytes.
    #[must_use]
    pub fn new(status: StatusCode, limit: usize) -> Self {
        Self { status, limit }
    }

    /// The node's status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The most bytes the gateway reads of one answer.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The [`Oversized`] a call that ended in `error` stopped at, when the
    /// engine stopped reading its answer at the bound.
    #[must_use]
    pub fn of_client_error(error: &ClientError) -> Option<Self> {
        match error {
            ClientError::AnswerTooLarge { status, limit, .. } => Some(Self::new(*status, *limit)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Oversized;
    use http::{Method, StatusCode};
    use openehr_its::rest::client::{ClientError, TransportError};

    #[test]
    fn an_answer_past_the_bound_is_the_bound() {
        let error = ClientError::AnswerTooLarge {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            status: StatusCode::OK,
            limit: 1024,
        };
        assert_eq!(
            Some(Oversized::new(StatusCode::OK, 1024)),
            Oversized::of_client_error(&error)
        );
    }

    #[test]
    fn a_transport_error_is_not_the_bound() {
        let refused = ClientError::Transport {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source: TransportError::Send {
                source: Box::new(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)),
            },
        };
        assert_eq!(None, Oversized::of_client_error(&refused));
        let timeout = ClientError::Transport {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source: TransportError::Timeout {
                source: Box::new(std::io::Error::from(std::io::ErrorKind::TimedOut)),
            },
        };
        assert_eq!(None, Oversized::of_client_error(&timeout));
    }
}
