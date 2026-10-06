// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fan-out of a federated query: the plan dispatched under the budget,
//! what the answer showed of each member recorded, and the status it settles
//! on (§11).

use std::time::Instant;

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::fanout::{
    Budget, Completion, FanOutError, FederatedAnswer, Plan, fan_out_within,
};
use ferrofed_engine::outbound_id::OutboundId;
use http::StatusCode;
use tracing::Instrument as _;
use tracing::field::Empty;

use super::Failure;
use crate::facade::{follow_up, security};
use crate::federation::Federation;

/// Runs the fan-out of `plan` ([`fanned_out`]), then records what it showed
/// of each member ([`observed`]) and feeds the versions it saw to the
/// learned map ([`follow_up::observe`]), under `request_id`.
///
/// # Errors
///
/// Returns [`Failure::FanOut`] when the fan-out fails on the gateway's side,
/// recorded as a security event first.
pub(super) async fn dispatched(
    federation: &Federation,
    plan: Plan,
    budget: Budget,
    (started, conveyance, outbound): (Instant, &Conveyance, OutboundId),
    request_id: &str,
) -> Result<FederatedAnswer, Failure> {
    let answer = fanned_out(federation, plan, budget, (started, conveyance, outbound))
        .await
        .map_err(|error| {
            security::fan_out(&error, request_id);
            Failure::FanOut(error)
        })?;
    observed(federation, &answer);
    follow_up::observe(federation, answer.seen(), request_id);
    Ok(answer)
}

/// Records what a fan-out showed of each member it dispatched to: its last
/// state for the health surface, read from the node's own answer, and its
/// request for the metrics surface, read from its §11.1 record.
pub(in crate::facade) fn observed(federation: &Federation, answer: &FederatedAnswer) {
    let records = answer.federation().endpoints();
    for (endpoint, contact) in answer.contacts() {
        federation.dependencies().contacted(endpoint, contact);
        let record = records
            .iter()
            .find(|record| record.id().as_str() == endpoint.as_str());
        if let Some(record) = record {
            let outcome = answer.observed(endpoint).unwrap_or(record.outcome());
            federation.requests().settled(endpoint, outcome, contact);
        }
    }
}

/// The status of a fan-out that answered `status`, its resolution failed
/// at a member when `resolution_failed`, under `completion`.
pub(in crate::facade) fn settled(
    status: StatusCode,
    resolution_failed: bool,
    completion: Completion,
) -> StatusCode {
    // NOTE: no specification governs this (§11.3 covers only an answered lookup):
    // our own design, a cross-reference that could not answer fails the query
    // 424 under all-or-nothing; under best-effort it stays reported.
    if resolution_failed && completion == Completion::AllOrNothing && status == StatusCode::OK {
        return StatusCode::FAILED_DEPENDENCY;
    }
    status
}

/// Runs the fan-out of `plan` inside the `fan_out` span, which names how many
/// endpoints were asked and the status of the answer.
pub(in crate::facade) async fn fanned_out(
    federation: &Federation,
    plan: Plan,
    budget: Budget,
    (started, conveyance, outbound): (Instant, &Conveyance, OutboundId),
) -> Result<FederatedAnswer, FanOutError> {
    let span = tracing::info_span!(
        "fan_out",
        endpoints = plan.dispatched().count(),
        http.response.status_code = Empty,
    );
    let answer = fan_out_within(
        federation.clients(),
        federation.snapshot(),
        plan,
        budget,
        started,
        (conveyance, Some(outbound)),
    )
    .instrument(span.clone())
    .await?;
    span.record("http.response.status_code", answer.status().as_u16());
    Ok(answer)
}
