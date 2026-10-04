// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The span of each node request, and the W3C Trace Context the request
//! carries to the node.
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
//! part of it comes from the client. A client's own `traceparent` and
//! `tracestate` are never read: a trace id a client chooses can encode
//! anything, a patient identifier included, so the gateway neither
//! continues it, which would carry it to every node, nor links or records
//! it, which would carry it into the operator's telemetry (§5.4.1, N33;
//! W3C Trace Context §3.4 and §6.1 let a service restart the trace).
//! Without the export, no span has a trace context and no node request
//! carries a `traceparent`. No specification governs tracing: our own
//! design.

use std::future::Future;

use ferrofed_registry::id::EndpointId;
use openehr_federation::status::EndpointStatus;
use opentelemetry::propagation::{Injector, TextMapPropagator as _};
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

#[cfg(test)]
mod tests {
    use super::outbound;

    #[test]
    fn a_span_outside_an_exported_trace_sends_no_traceparent() {
        let span = tracing::info_span!("node_request");
        let _entered = span.enter();
        assert_eq!(None, outbound());
    }
}
