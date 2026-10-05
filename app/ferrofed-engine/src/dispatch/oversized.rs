// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node's answer longer than the gateway reads of one answer.
//!
//! The HTTP engine a node client sends through ([`Transport`]) reads at most
//! a bound the deployment sets of each answer, and ends a longer one unread,
//! carrying [`Oversized`] as the source of a [`TransportError::Send`]. The
//! node was reached and answered, and nothing usable came back, so the
//! endpoint is `node-error` (§11.1), never `offline`: it fails the query
//! with `424` under all-or-nothing and clears `meta.federation.complete`
//! (§11.4). No specification governs the bound itself: our own design.
//!
//! [`Transport`]: openehr_its::rest::client::Transport

use std::error::Error;

use http::StatusCode;
use openehr_its::rest::client::{ClientError, TransportError};

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
    /// engine ended it there.
    ///
    /// The engine reports it as the source of a [`TransportError::Send`], or
    /// further down that error's chain where an engine wraps another.
    #[must_use]
    pub fn of_client_error(error: &ClientError) -> Option<&Self> {
        let ClientError::Transport {
            source: TransportError::Send { source },
            ..
        } = error
        else {
            return None;
        };
        let top: &(dyn Error + 'static) = &**source;
        std::iter::successors(Some(top), |cause| cause.source())
            .find_map(|cause| cause.downcast_ref::<Self>())
    }
}

#[cfg(test)]
mod tests {
    use super::Oversized;
    use http::{Method, StatusCode};
    use openehr_its::rest::client::{ClientError, TransportError};

    /// A wrapping engine's error whose source is the bound.
    #[derive(Debug, thiserror::Error)]
    #[error("the wrapped engine failed")]
    struct Wrapped(#[source] Oversized);

    fn sent(source: Box<dyn std::error::Error + Send + Sync>) -> ClientError {
        ClientError::Transport {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source: TransportError::Send { source },
        }
    }

    #[test]
    fn the_bound_is_found_at_the_top_of_a_send_error_and_down_its_chain() {
        let bound = Oversized::new(StatusCode::OK, 1024);
        assert_eq!(
            Some(&bound),
            Oversized::of_client_error(&sent(Box::new(bound)))
        );
        assert_eq!(
            Some(&bound),
            Oversized::of_client_error(&sent(Box::new(Wrapped(bound))))
        );
    }

    #[test]
    fn a_send_error_of_another_kind_is_not_the_bound() {
        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        assert_eq!(None, Oversized::of_client_error(&sent(Box::new(refused))));
        let timeout = ClientError::Transport {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source: TransportError::Timeout {
                source: Box::new(Oversized::new(StatusCode::OK, 1)),
            },
        };
        assert_eq!(
            None,
            Oversized::of_client_error(&timeout),
            "only a send error carries the bound"
        );
    }
}
