// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The duplicate check of a received dispensation (hazard I-12 of the
//! clinical safety risk file).
//!
//! A client that sent a dispensation and got no answer, a time-out, cannot
//! tell whether the owner wrote it, and may send it again. Before the
//! gateway writes, it asks the owner's own endpoint for the patient's
//! dispensations, by the member's own `ehr_id` alone, matches the received
//! one's business identifier among them at the gateway, so the identifier
//! never reaches the member in a query (§5.4.1, N33), and refuses a
//! dispensation the owner already holds. It is a read with no lock and no
//! count at the gateway; an owner that does not answer the read refuses the
//! dispensation, never counts as holding none. The check and its answers are our own design: no
//! specification governs them.

use std::time::Instant;

use ferrofed_eehrxf::dispensations::RESOURCE;
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::single_node::native_search::Searched;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use crate::facade::native::Chosen;
use crate::facade::prescriptions::{Held, paged};
use crate::facade::request::Arrived;
use crate::federation::Federation;
use crate::fhir::locator::Locator;

/// What the owner's dispensations of one business identifier showed.
#[derive(Debug)]
pub(super) enum Checked {
    /// The owner holds none: the dispensation is new.
    New,
    /// The owner holds one already, by the face's id when the face can give
    /// it one.
    Held(Option<String>),
    /// The owner's answer cannot tell, with the status and the issue type
    /// the dispensation is refused with and why.
    Untold(StatusCode, &'static str, String),
}

/// Asks `writing`, the owner's own endpoint for dispensations, for the
/// patient's dispensations, and finds among them one of the business
/// identifier `(system, value)`, under `options`, abandoned when the overall
/// budget `until` runs out.
pub(super) async fn check(
    federation: &Federation,
    writing: &Chosen,
    (system, value): (&str, &str),
    (options, until): (&DispatchOptions, tokio::time::Instant),
) -> Checked {
    let held = match paged(federation, (writing, RESOURCE, None), options, until).await {
        Ok(held) => held,
        Err(_unknown) => {
            return Checked::Untold(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                String::from("the owner's endpoint for dispensations could not be asked"),
            );
        }
    };
    match held {
        Held::Found(pages) => {
            let found = pages
                .iter()
                .flat_map(|page| page.prescriptions.iter())
                .find(|held| {
                    held.identifiers.iter().any(|(held_system, held_value)| {
                        held_system == system && held_value == value
                    })
                });
            match found {
                None => Checked::New,
                Some(existing) => Checked::Held(existing.kept.id().and_then(|id| {
                    Locator::new(writing.native.clone(), writing.ehr_id.clone(), id).id()
                })),
            }
        }
        Held::Unusable { status, .. } => Checked::Untold(
            StatusCode::BAD_GATEWAY,
            "processing",
            format!(
                "the dispensation was not written: the owner's answer ({status}) to the search for its dispensations of the patient cannot be read, so a dispensation already written under the same identifier cannot be ruled out"
            ),
        ),
        Held::Silent(searched) => {
            let (answer, code) = match searched.status() {
                EndpointStatus::TimeOut | EndpointStatus::Offline => {
                    (StatusCode::GATEWAY_TIMEOUT, "timeout")
                }
                _ => (StatusCode::SERVICE_UNAVAILABLE, "transient"),
            };
            Checked::Untold(
                answer,
                code,
                format!(
                    "the dispensation was not written: the owner did not answer the search for its dispensations of the patient ({}), so a dispensation already written under the same identifier cannot be ruled out",
                    status_of(&searched)
                ),
            )
        }
    }
}

/// The §11.1 status a member's unanswered search shows.
fn status_of(searched: &Searched) -> &'static str {
    searched.status().as_str()
}

/// The deadline of one request to the owner within the request `arrived`'s
/// overall budget, and the budget's end, or `None` when the clock cannot
/// hold them.
pub(super) fn budgeted(
    federation: &Federation,
    arrived: &Arrived<'_>,
) -> Option<(Instant, tokio::time::Instant)> {
    let budget = federation.budget();
    let overall = arrived.started.checked_add(budget.overall())?;
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    Some((per_node, tokio::time::Instant::from_std(overall)))
}
