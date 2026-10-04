// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The span of each node request, the W3C Trace Context the request carries
//! to the node, and the link to the trace a client request names.
//!
//! A node request runs inside one `node_request` span, a child of whatever
//! span is current: the client request's, or the fan-out's. Its fields are
//! the endpoint id, the ITS-REST `operationId`, what the request showed of
//! the node and, for a query, its §11.1 status. None of them comes from the
//! client request, so no patient identifier and no query text can reach
//! one (§5.4.1, N33).
//!
//! Every client request starts a trace of the gateway's own, with a random
//! trace id, and a node request carries the `traceparent` of its own span
//! in that trace (W3C Trace Context, <https://www.w3.org/TR/trace-context/>),
//! so the node's spans join the gateway's trace. ITS-REST declares no trace
//! header, and RFC 9110 §5.1 has a recipient ignore a field it does not
//! recognize, so a node that does not trace loses nothing. The value is
//! written by the propagator of `opentelemetry_sdk` from the span, and no
//! part of it comes from the client: a trace id a client chooses can encode
//! anything, a patient identifier included (§5.4.1, N33). A client's own
//! `traceparent` is recorded as a span link on the request span
//! ([`link_from`]), which only the operator's collector sees, and its
//! `tracestate` is never read. Without the export, no span has a trace
//! context and no node request carries a `traceparent`. No specification
//! governs tracing: our own design.

use std::future::Future;

use ferrofed_registry::id::EndpointId;
use http::HeaderMap;
use openehr_federation::status::EndpointStatus;
use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator as _};
use opentelemetry::trace::TraceContextExt as _;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::field::Empty;
use tracing::{Instrument as _, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

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

/// Links `span`, the root of the gateway's own trace for one client request,
/// to the span the `traceparent` of `headers` names, when the client sent
/// exactly one that parses.
///
/// The client's trace is never the parent, so its trace id never reaches a
/// node; the link reaches the operator's collector alone. Its `tracestate`
/// is never read. A `traceparent` that does not parse leaves no link, and a
/// gateway that exports no traces records nothing.
pub fn link_from(span: &Span, headers: &HeaderMap) {
    // NOTE: §5.4.1, N33; W3C Trace Context §3.4 and §6.1 let a service restart the trace: a client
    // chooses its trace id and can encode an identifier in it, so the gateway starts its own.
    let client = TraceContextPropagator::new().extract(&OnlyTraceparent(headers));
    span.add_link(client.span().span_context().clone());
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
        // neither is linked.
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
    fn two_traceparent_fields_link_neither() {
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
