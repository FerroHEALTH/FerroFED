// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification of what a node answered into exactly one §11.1
//! endpoint status, or into a [`DispatchError`] when the failure was the
//! gateway's own and nothing reached the node.

use std::time::Instant;

use ferrofed_registry::id::EndpointId;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_its::rest::client::{ClientError, ErrorBody, TransportError};
use openehr_its::rest::generated::query::client::QueryExecuteAdhocQueryBodyOutcome;

use super::{DispatchError, NodeReply};

/// The longest node message a `node-error` copies into `error`, in characters.
const MESSAGE_LIMIT: usize = 512;

/// `reply`, or a `node-error` when one of its rows has fewer than `width`
/// cells (§11.1).
pub(super) fn narrow(reply: NodeReply, width: usize) -> NodeReply {
    let NodeReply::Answered {
        result_set,
        latency_ms,
    } = reply
    else {
        return reply;
    };
    match result_set
        .rows
        .iter()
        .map(Vec::len)
        .find(|found| *found < width)
    {
        Some(found) => NodeReply::Failed {
            outcome: Outcome::NodeError {
                latency_ms,
                error: text(format!(
                    "the node answered a row with {found} cells where the dispatched query selects {width}"
                )),
            },
        },
        None => NodeReply::Answered {
            result_set,
            latency_ms,
        },
    }
}

/// The milliseconds since `started`, saturating at `u64::MAX`.
pub(super) fn elapsed_ms(started: Instant) -> u64 {
    // NOTE: §9.5 reports latency in whole milliseconds; a duration past
    // u64::MAX ms cannot occur inside any budget, so saturating loses nothing.
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The reply for a documented answer of `POST /query/aql`.
pub(super) fn answered(outcome: QueryExecuteAdhocQueryBodyOutcome, latency_ms: u64) -> NodeReply {
    match outcome {
        QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => NodeReply::Answered {
            result_set: Box::new(body),
            latency_ms,
        },
        QueryExecuteAdhocQueryBodyOutcome::BadRequest { body } => {
            node_error(latency_ms, "400 Bad Request", &body)
        }
        QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { body } => {
            node_error(latency_ms, "408 Request Timeout", &body)
        }
    }
}

/// The reply, or the gateway-side error, for a call that reached no
/// documented answer.
pub(super) fn failed(
    endpoint: &EndpointId,
    error: ClientError,
    latency_ms: u64,
) -> Result<NodeReply, DispatchError> {
    let failure = |outcome| Ok(NodeReply::Failed { outcome });
    match error {
        ClientError::DeadlineElapsed { .. } => failure(Outcome::TimeOut {
            latency_ms,
            error: text("no answer before the deadline, which passed before the request was sent"),
        }),
        ClientError::Transport {
            source: TransportError::Timeout { source },
            ..
        } => failure(Outcome::TimeOut {
            latency_ms,
            error: text(format!(
                "no answer before the deadline: {}",
                chain(&*source)
            )),
        }),
        ClientError::Transport {
            source: TransportError::Send { source },
            ..
        } => failure(Outcome::Offline {
            latency_ms,
            error: text(format!(
                "the node could not be reached: {}",
                chain(&*source)
            )),
        }),
        ClientError::Unauthorized { body, .. } => {
            Ok(node_error(latency_ms, "401 Unauthorized", &body))
        }
        ClientError::Forbidden { body, .. } => Ok(node_error(latency_ms, "403 Forbidden", &body)),
        ClientError::ServiceFailure { status, body, .. }
        | ClientError::UndocumentedStatus { status, body, .. } => {
            Ok(node_error(latency_ms, &status.to_string(), &body))
        }
        ClientError::Body { status, source, .. } => failure(Outcome::NodeError {
            latency_ms,
            error: text(format!(
                "the node answered {status} with a body that is not an ITS-REST RESULT_SET (at `{}`, a {:?} defect)",
                source.path(),
                source.inner().classify(),
            )),
        }),
        credentials @ ClientError::Credentials { .. } => Err(DispatchError::Credentials {
            endpoint: endpoint.clone(),
            source: Box::new(credentials),
        }),
        other => Err(DispatchError::Compose {
            endpoint: endpoint.clone(),
            source: Box::new(other),
        }),
    }
}

/// A `node-error` reply carrying the node's `status` and its own message.
fn node_error(latency_ms: u64, status: &str, body: &ErrorBody) -> NodeReply {
    let message = match body.message().or_else(|| body.text()) {
        Some(said) if !said.trim().is_empty() => {
            format!("the node answered {status}: {}", bounded(said.trim()))
        }
        _ => format!("the node answered {status}"),
    };
    NodeReply::Failed {
        outcome: Outcome::NodeError {
            latency_ms,
            error: text(message),
        },
    }
}

/// An `error` message; every message here starts with fixed text, so it is
/// never empty.
fn text(message: impl Into<String>) -> ErrorDetail {
    ErrorDetail::Text(message.into())
}

/// `error` and its causes, joined, so the reason a node was unreachable is
/// kept (the engine reports it, never only "offline").
fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = error.to_string();
    let mut next = error.source();
    while let Some(cause) = next {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        next = cause.source();
    }
    out
}

/// At most [`MESSAGE_LIMIT`] characters of `said`, cut on a character
/// boundary.
fn bounded(said: &str) -> String {
    let mut kept: String = said.chars().take(MESSAGE_LIMIT).collect();
    if said.chars().nth(MESSAGE_LIMIT).is_some() {
        kept.push('…');
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::{MESSAGE_LIMIT, bounded, failed};
    use crate::dispatch::DispatchError;
    use ferrofed_registry::id::EndpointId;
    use http::Method;
    use openehr_its::rest::client::ClientError;
    use url::Url;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Whether the classification of `error` is a gateway-side compose
    /// failure, the one place a call no node answered can land.
    fn is_compose(error: ClientError) -> Result<bool, Box<dyn std::error::Error>> {
        let endpoint = EndpointId::new("node-a-pub")?;
        Ok(matches!(
            failed(&endpoint, error, 0),
            Err(DispatchError::Compose { .. })
        ))
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn the_client_errors_no_node_can_cause_are_compose_failures() -> TestResult {
        let base = Url::parse("data:text/plain,not-a-base")?;
        assert!(is_compose(ClientError::BaseUrl { base })?);
        let build = http::Request::builder().uri("http://[::1").body(());
        let Err(source) = build else {
            return Err("an unparsable URI built a request".into());
        };
        assert!(is_compose(ClientError::Build {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source,
        })?);
        let Err(source) = http::HeaderName::from_bytes(b"not a name") else {
            return Err("an illegal header name parsed".into());
        };
        assert!(is_compose(ClientError::HeaderName {
            header: "not a name".to_owned(),
            source,
        })?);
        let Err(source) = http::HeaderValue::from_str("line\nbreak") else {
            return Err("a line break parsed as a header value".into());
        };
        assert!(is_compose(ClientError::HeaderValue {
            header: "X-Request-Id".to_owned(),
            source,
        })?);
        assert!(is_compose(ClientError::UnsupportedMediaType {
            requested: "application/xml".to_owned(),
        })?);
        let Err(source) = serde_json::from_str::<u8>("not json") else {
            return Err("an illegal JSON text parsed".into());
        };
        assert!(is_compose(ClientError::Serialize { source })?);
        Ok(())
    }

    #[test]
    fn a_long_node_message_is_cut_on_a_character_boundary() {
        let said = "é".repeat(MESSAGE_LIMIT + 10);
        let kept = bounded(&said);
        assert_eq!(kept.chars().count(), MESSAGE_LIMIT + 1);
        assert!(kept.ends_with('…'));
        assert_eq!(bounded("short"), "short");
    }
}
