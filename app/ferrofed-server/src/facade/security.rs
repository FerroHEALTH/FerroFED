// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security events of identifier hygiene (§5.4.3).
//!
//! Every strip, every refusal and every outbound-gate stop is logged under
//! [`TARGET`], naming what happened and where in the query it was written, by
//! byte range, and never the text written there or any identifier value.

use std::ops::Range;

use ferrofed_engine::dispatch::DispatchError;
use ferrofed_engine::fanout::FanOutError;
use openehr_federation::aql::PatientQuery;
use openehr_federation::aql::refusal::Refusal;

/// The tracing target of every security event, so an operator can route them
/// apart from the request log.
pub const TARGET: &str = "ferrofed::security";

/// A query was refused before anything was dispatched (§5.4.3).
pub(super) fn refused(refusal: &Refusal, request_id: &str) {
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
        tracing::info!(
            target: TARGET,
            event = "patient-predicate-stripped",
            at = %Position(at.as_ref()),
            request_id,
            "a patient predicate was consumed as resolution input and stripped from every node query"
        );
    }
}

/// A fan-out failed; when the outbound gate stopped a request, that is a
/// security event naming the endpoint and the part of the request.
pub(super) fn fan_out(error: &FanOutError, request_id: &str) {
    if let FanOutError::Dispatch(DispatchError::Withheld { endpoint, part }) = error {
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
