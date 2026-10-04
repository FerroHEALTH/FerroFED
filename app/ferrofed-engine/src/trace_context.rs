// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The span of each node request, the W3C Trace Context the request carries
//! to the node, and the one a client request may continue.
//!
//! A node request runs inside one `node_request` span, a child of whatever
//! span is current: the client request's, or the fan-out's. Its fields are
//! the endpoint id, the ITS-REST `operationId`, what the request showed of
//! the node and, for a query, its §11.1 status. None of them comes from the
//! client request, so no patient identifier and no query text can reach
//! one (§5.4.1, N33).
//!
//! When the gateway exports traces, a node request carries the `traceparent`
//! of its own span (W3C Trace Context, <https://www.w3.org/TR/trace-context/>),
//! so the node's spans join the same trace. ITS-REST declares no trace
//! header, and RFC 9110 §5.1 has a recipient ignore a field it does not
//! recognize, so a node that does not trace loses nothing. The value is
//! written by the propagator of `opentelemetry_sdk` from the span, never
//! copied from the client. A client's own `traceparent` is read for its trace
//! id and parent span id, and its `tracestate`, free text a node could read,
//! is never read and never sent. Without the export, no span has a trace
//! context and no node request carries a `traceparent`. No specification
//! governs tracing: our own design.

use std::future::Future;

use ferrofed_registry::id::EndpointId;
use http::HeaderMap;
use openehr_federation::status::EndpointStatus;
use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator as _};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::field::Empty;
use tracing::{Instrument as _, Span};
use tracing_opentelemetry::{OpenTelemetrySpanExt as _, SetParentError};

use crate::dispatch::Contact;

/// The W3C Trace Context field that names a request's trace and the span it
/// was sent from.
pub const TRACEPARENT: &str = "traceparent";

/// Returns the `traceparent` of the current span, or `None` when the span
/// belongs to no exported trace.
#[must_use]
pub fn outbound() -> Option<String> {
    let mut carrier = Traceparent::default();
    TraceContextPropagator::new().inject_context(&Span::current().context(), &mut carrier);
    carrier.0
}

/// Makes `span` continue the trace the `traceparent` of `headers` names, when
/// the client sent exactly one that parses.
///
/// Its `tracestate` is never read. A request with no such field, or a
/// gateway that exports no traces, leaves `span` the root of a trace of its
/// own.
pub fn continue_from(span: &Span, headers: &HeaderMap) {
    let parent = TraceContextPropagator::new().extract(&OnlyTraceparent(headers));
    match span.set_parent(parent) {
        // NOTE: tracing-opentelemetry SetParentError (docs.rs): without the export layer there is
        // no trace to continue, which is the configured state, never a failure.
        Ok(()) | Err(SetParentError::LayerNotFound) => {}
        Err(error) => tracing::debug!(%error, "the client's trace context was not continued"),
    }
}

/// Runs `call`, one request to the node at `endpoint` through the ITS-REST
/// operation `operation`, inside a `node_request` span, and records on the
/// span what `observed` reads of its result.
pub(crate) async fn node_request<R>(
    endpoint: &EndpointId,
    operation: &'static str,
    call: impl Future<Output = R>,
    observed: impl FnOnce(&R) -> (Option<Contact>, Option<EndpointStatus>),
) -> R {
    let span = tracing::info_span!(
        "node_request",
        otel.kind = "client",
        endpoint_id = endpoint.as_str(),
        operation,
        contact = Empty,
        http.response.status_code = Empty,
        outcome = Empty,
    );
    let result = call.instrument(span.clone()).await;
    let (contact, outcome) = observed(&result);
    if let Some(contact) = contact {
        record(&span, contact);
    }
    if let Some(outcome) = outcome {
        span.record("outcome", outcome.as_str());
    }
    result
}

/// Records on `span` what a node request showed of the node.
fn record(span: &Span, contact: Contact) {
    let reached = match contact {
        Contact::Unsent => "unsent",
        Contact::Silent => "silent",
        Contact::Answered(status) => {
            span.record("http.response.status_code", status.as_u16());
            "answered"
        }
    };
    span.record("contact", reached);
}

/// A carrier that keeps the `traceparent` a propagator writes, and nothing
/// else.
#[derive(Default)]
struct Traceparent(Option<String>);

impl Injector for Traceparent {
    fn set(&mut self, key: &str, value: String) {
        if key.eq_ignore_ascii_case(TRACEPARENT) {
            self.0 = Some(value);
        }
    }
}

/// The client headers as a propagator reads them: the one `traceparent`
/// field, and no other.
struct OnlyTraceparent<'a>(&'a HeaderMap);

impl OnlyTraceparent<'_> {
    /// The value of the single `traceparent` field, or `None` when there is
    /// none, more than one, or one that is not visible ASCII.
    fn value(&self) -> Option<&str> {
        let mut fields = self.0.get_all(TRACEPARENT).iter();
        // NOTE: no specification governs this: our own design; two fields name two parents, so
        // neither is continued.
        match (fields.next(), fields.next()) {
            (Some(only), None) => only.to_str().ok(),
            _ => None,
        }
    }
}

impl Extractor for OnlyTraceparent<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        if key.eq_ignore_ascii_case(TRACEPARENT) {
            self.value()
        } else {
            None
        }
    }

    fn keys(&self) -> Vec<&str> {
        self.value().map_or_else(Vec::new, |_| vec![TRACEPARENT])
    }
}

#[cfg(test)]
mod tests {
    use super::{Extractor as _, OnlyTraceparent, TRACEPARENT, outbound};
    use http::{HeaderMap, HeaderValue};

    const PARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    #[test]
    fn only_the_single_traceparent_is_read_and_never_the_tracestate() {
        let mut headers = HeaderMap::new();
        headers.insert(TRACEPARENT, HeaderValue::from_static(PARENT));
        headers.insert(
            "tracestate",
            HeaderValue::from_static("vendor=SYNTHETIC-0001"),
        );
        let read = OnlyTraceparent(&headers);
        assert_eq!(Some(PARENT), read.get("traceparent"));
        assert_eq!(None, read.get("tracestate"));
        assert_eq!(vec![TRACEPARENT], read.keys());
    }

    #[test]
    fn two_traceparent_fields_continue_neither() {
        let mut headers = HeaderMap::new();
        headers.append(TRACEPARENT, HeaderValue::from_static(PARENT));
        headers.append(TRACEPARENT, HeaderValue::from_static(PARENT));
        let read = OnlyTraceparent(&headers);
        assert_eq!(None, read.get(TRACEPARENT));
        assert!(read.keys().is_empty());
    }

    #[test]
    fn a_span_outside_an_exported_trace_sends_no_traceparent() {
        let span = tracing::info_span!("node_request");
        let _entered = span.enter();
        assert_eq!(None, outbound());
    }
}
