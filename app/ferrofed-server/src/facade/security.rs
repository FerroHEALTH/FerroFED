// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security events of identifier hygiene (§5.4.3).
//!
//! Every strip, every refusal and every outbound-gate stop is logged under
//! [`TARGET`], naming what happened and where in the query it was written, by
//! byte range, and never the text written there or any identifier value. The
//! `request_id` of an event is the gateway's outbound id, never the client's
//! free-text `x-request-id` ([`crate::request_id`]).

use std::ops::Range;

use ferrofed_engine::declared::Carrier;
use ferrofed_engine::dispatch::DispatchError;
use ferrofed_engine::fanout::FanOutError;
use ferrofed_engine::hygiene::Part;
use ferrofed_registry::definition::{QueryName, QueryVersion};
use ferrofed_registry::id::EndpointId;
use openehr_federation::aql::PatientQuery;
use openehr_federation::aql::refusal::Refusal;

use crate::metrics::security::Event;

/// The tracing target of every security event, so an operator can route them
/// apart from the request log.
pub const TARGET: &str = "ferrofed::security";

/// A query was refused before anything was dispatched (§5.4.3).
pub(super) fn refused(refusal: &Refusal, request_id: &str) {
    Event::AqlRefused.record();
    tracing::warn!(
        target: TARGET,
        event = "aql-refused",
        kind = refusal.kind(),
        at = %Position(refusal.at()),
        request_id,
        "a federated query was refused before dispatch"
    );
}

/// The rewrite consumed the patient predicates of `query`: one event per
/// stripped predicate, by position (§5.4.3).
pub(super) fn stripped(query: &PatientQuery, request_id: &str) {
    for at in query.stripped() {
        Event::PatientPredicateStripped.record();
        tracing::info!(
            target: TARGET,
            event = "patient-predicate-stripped",
            at = %Position(at.as_ref()),
            request_id,
            "a patient predicate was consumed as resolution input and stripped from every node query"
        );
    }
}

/// The `subject_id` and `subject_namespace` of `GET {base}/v1/ehr` were
/// consumed as resolution input, and no node request carries them (§5.4.3).
pub(super) fn subject_consumed(request_id: &str) {
    Event::SubjectParametersConsumed.record();
    tracing::info!(
        target: TARGET,
        event = "subject-parameters-consumed",
        request_id,
        "the subject query parameters were consumed as resolution input and are carried by no node request"
    );
}

/// A fan-out failed; when the outbound gate stopped a request, that is a
/// security event naming the endpoint and the part of the request.
pub(super) fn fan_out(error: &FanOutError, request_id: &str) {
    if let FanOutError::Dispatch(DispatchError::Withheld { endpoint, part }) = error {
        Event::OutboundGateStopped.record();
        tracing::warn!(
            target: TARGET,
            event = "outbound-gate-stopped",
            endpoint = %endpoint,
            part = %part,
            request_id,
            "a request to a node would have carried a patient identifier and was not sent"
        );
    }
}

/// The outbound gate stopped a request the gateway composed for
/// `endpoint`, because `part` would have carried a patient identifier.
pub(super) fn gate_stopped(endpoint: &EndpointId, part: &Part, request_id: &str) {
    Event::OutboundGateStopped.record();
    tracing::warn!(
        target: TARGET,
        event = "outbound-gate-stopped",
        endpoint = %endpoint,
        part = %part,
        request_id,
        "a request to a node would have carried a patient identifier and was not sent"
    );
}

/// The patient identifiers of a received document were consumed as
/// resolution input, and no node request carries them in its path, query
/// string or headers (§5.4.3).
pub(super) fn document_subject_consumed(request_id: &str) {
    Event::SubjectParametersConsumed.record();
    tracing::info!(
        target: TARGET,
        event = "document-subject-consumed",
        request_id,
        "the patient identifiers of a received document were consumed as resolution input"
    );
}

/// A request was refused for a query parameter its ITS-REST operation does
/// not admit at the gateway, named by its position and never by its name or
/// value (§5.4.3).
///
/// A routed request admits only the parameters its operation declares and the
/// route forwards, and a request the gateway answers itself, such as a
/// stored-query definition `PUT`, only those its operation declares.
pub(super) fn query_parameter_refused(position: usize, request_id: &str) {
    Event::QueryParameterRefused.record();
    tracing::warn!(
        target: TARGET,
        event = "query-parameter-refused",
        position,
        request_id,
        "a request carried a query parameter the gateway does not admit for its operation, and was refused"
    );
}

/// A routed request was refused for a declared value that does not match its
/// declared kind, named by where it travelled and never by the value
/// (§5.4.3).
pub(super) fn value_refused(carrier: Carrier, request_id: &str) {
    Event::ParameterValueRefused.record();
    tracing::warn!(
        target: TARGET,
        event = "parameter-value-refused",
        carrier = %carrier,
        request_id,
        "a routed request carried a declared value that does not match its declared kind, and was refused"
    );
}

/// A read, or a query scoped to one `ehr_id`, was refused the ask-all probe
/// because its `ehr_id` is no bare UUID, and nothing was sent; the event
/// never names the `ehr_id` (§5.4.3).
pub(super) fn probe_refused(request_id: &str) {
    Event::EhrIdProbeRefused.record();
    tracing::warn!(
        target: TARGET,
        event = "ehr-id-probe-refused",
        request_id,
        "a read or a query whose ehr_id is not a UUID was refused the ask-all probe, and no member was asked"
    );
}

/// The outbound gate stopped a routed request, naming the endpoint and the
/// part of the request.
pub(super) fn forward_withheld(endpoint: &EndpointId, part: Part, request_id: &str) {
    Event::OutboundGateStopped.record();
    tracing::warn!(
        target: TARGET,
        event = "outbound-gate-stopped",
        endpoint = %endpoint,
        part = %part,
        request_id,
        "a routed request would have carried a patient identifier and was not sent"
    );
}

/// A stored-query definition naming its patient by a literal was refused, so
/// the registry holds no identifier (§12.7, §5.4.3).
pub(super) fn definition_refused(at: Option<&Range<usize>>, request_id: &str) {
    Event::DefinitionSubjectLiteral.record();
    tracing::warn!(
        target: TARGET,
        event = "definition-subject-literal",
        at = %Position(at),
        request_id,
        "a stored-query definition named its patient by a literal, and was not stored"
    );
}

/// A definition a stored-query store holds was refused on a read, because it
/// names its patient by a literal or does not admit as a definition, so it
/// is never served or run (§12.7, §5.4.1, N33); the event names the
/// definition, never its text.
pub(crate) fn held_definition_refused(name: &QueryName, version: QueryVersion) {
    Event::HeldDefinitionRefused.record();
    tracing::warn!(
        target: TARGET,
        event = "held-definition-refused",
        name = %name,
        version = %version,
        "a definition the stored-query store holds was refused, and is not served or run"
    );
}

/// A byte range as `a..b`, or `unknown`.
struct Position<'a>(Option<&'a Range<usize>>);

impl std::fmt::Display for Position<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(at) => write!(f, "{}..{}", at.start, at.end),
            None => f.write_str("unknown"),
        }
    }
}
