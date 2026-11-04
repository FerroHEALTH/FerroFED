// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation written at an owner in the map mode (A102 point 2b).
//!
//! The owner is found as for a native member ([`super::lookup`]), among the
//! prescriptions the map mode reads from its medication orders. The
//! dispensation must name one prescription. It is mapped into one
//! composition of the owner's template by the MPD project of `eehrxf`
//! (`MapMode::dispensation`), its `ACTION` naming the order it carries out
//! by the `OBJECT_VERSION_ID` of the order's composition and the order's
//! archetype path (RM 1.1.0 EHR IM `INSTRUCTION_DETAILS`), the received
//! dispensation kept in its `FEEDER_AUDIT.original_content`. Before it is
//! written, the owner's medication actions are read with
//! `eu.ferrofed.eehrxf::medication-actions`, and one whose kept dispensation
//! carries the received one's business identifier refuses it `409` with
//! nothing written (hazard I-12). It is then created once, as
//! `composition_create`, at the owner by its own `ehr_id` alone (ITS-REST
//! 1.1.0 EHR API; §5.4.1, N33). No specification governs a new object
//! written at the owner of the object it names: our own design.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::dispensations::Received;
use ferrofed_eehrxf::medication::Medication;
use ferrofed_eehrxf::prescriptions::mapped::{ORDER_ACTIVITY, order_in};
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use serde::Deserialize;

use super::answer::{Lookup, Wrote, accessed, attach, refused_after};
use super::lookup;
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::located::Located;
use crate::facade::mapped::{Answered, queried};
use crate::facade::native::Chosen;
use crate::facade::prescriptions::Member;
use crate::facade::request::Arrived;
use crate::federation::Federation;
use crate::metrics::dispensations::Outcome;
use ferrofed_eehrxf::dispensations;
use ferrofed_eehrxf::face;
use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::single_node::ehr::EhrCallError;

/// Writes the dispensation `received`, whose text is `body`, as one
/// composition at the map-mode owner `owner` the lookup `lookup` found, the
/// patient located as `located` holds it.
pub(super) async fn written(
    (federation, fhir, arrived): (&Arc<Federation>, &FhirSettings, &Arrived<'_>),
    (received, body): (&Received, &[u8]),
    (lookup, owner, located): (&Lookup<'_>, &Member, &Located),
    withheld: Arc<Withheld>,
) -> Response {
    let refuse = |status, code, text: &str| {
        Outcome::Failed.record();
        refused_after(federation, lookup, (status, code), text)
    };
    let writing = &owner.chosen;
    let declared = fhir
        .serving
        .members_of(Category::Dispensation)
        .is_some_and(|declared| declared.contains(writing.node.as_str()));
    let Some(mode) = fhir.mapped.get(writing.node.as_str()).filter(|_| declared) else {
        return refuse(
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            "the member that holds the prescription does not receive dispensations on this deployment",
        );
    };
    let Some((page, entry)) = lookup::named_at(&received.named, owner, &fhir.absolute) else {
        return refuse(
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            "a dispensation written in the map mode names one prescription, so nothing was written",
        );
    };
    let order = owner
        .composed
        .as_ref()
        .and_then(|composed| composed.get(page))
        .and_then(|held| order_in(&held.composition, &held.version, entry));
    let Some(order) = order else {
        return refuse(
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            "the prescription names no medication order the dispensation's action can name, so nothing was written",
        );
    };
    let (system, value) = (&received.identifier.0, &received.identifier.1);
    match duplicated(
        federation,
        arrived,
        (located, lookup.patient),
        (writing.via.clone(), (system, value)),
    )
    .await
    {
        Ok(false) => {}
        Ok(true) => {
            return refuse(
                StatusCode::CONFLICT,
                "duplicate",
                "the owner already holds this dispensation under the same identifier, so nothing was written; a dispensation sent again after a time-out is refused, never written twice",
            );
        }
        Err((status, code, text)) => return refuse(status, code, &text),
    }
    let Ok(text) = std::str::from_utf8(body) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the dispensation is no UTF-8 text",
        );
    };
    let now = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let composition = match mode.dispensation(text, (&order, ORDER_ACTIVITY), &now) {
        Ok(composition) => composition,
        Err(refusal) => {
            tracing::warn!(
                request_id = %arrived.outbound,
                error = %crate::chain(&refusal),
                "a dispensation in the map mode does not map"
            );
            return refuse(
                StatusCode::UNPROCESSABLE_ENTITY,
                "processing",
                "the dispensation does not map into a composition of the owner's template, so nothing was written",
            );
        }
    };
    let budget = federation.budget();
    let (Some(overall), Some(client)) = (
        arrived.started.checked_add(budget.overall()),
        federation.clients().get(&writing.via),
    ) else {
        return crate::error::fixed(Code::Internal, arrived.request_id);
    };
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    let options = DispatchOptions::new(per_node, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(withheld);
    let created = client
        .create_composition(writing.ehr_id.as_str(), composition.model(), &options)
        .await;
    composed(federation, (received, lookup), writing, &created)
}

/// Whether the owner behind the Query API endpoint `via` already holds a
/// dispensation of the patient `patient` under the business identifier
/// `system`|`value`, among the dispensations its medication actions keep, or
/// the refusal that says the question went unanswered.
async fn duplicated(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, patient): (&Located, (&str, &str)),
    (via, (system, value)): (EndpointId, (&str, &str)),
) -> Result<bool, (StatusCode, &'static str, String)> {
    let members = BTreeSet::from([via]);
    let unread = |status: &str| {
        format!(
            "the dispensation was not written: the owner did not answer the search for its dispensations of the patient ({status}), so a dispensation already written under the same identifier cannot be ruled out"
        )
    };
    let refused = |_failed| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            unread(EndpointStatus::NodeError.as_str()),
        )
    };
    let name = Medication::Actions.name().map_err(refused)?;
    let answered = queried(
        federation,
        arrived,
        located,
        (patient, (name.as_str(), Medication::Actions.aql())),
        &members,
    )
    .await
    .map_err(|_failed| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            unread(EndpointStatus::NodeError.as_str()),
        )
    })?;
    let Some((_, answered)) = answered.first() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            unread(EndpointStatus::Excluded.as_str()),
        ));
    };
    match answered {
        Answered::Rows(held) => Ok(held
            .iter()
            .any(|held| keeps(&held.composition, (system, value)))),
        Answered::Silent { status, .. } => {
            let answer = match status {
                EndpointStatus::TimeOut | EndpointStatus::Offline => {
                    (StatusCode::GATEWAY_TIMEOUT, "timeout")
                }
                _ => (StatusCode::SERVICE_UNAVAILABLE, "transient"),
            };
            Err((answer.0, answer.1, unread(status.as_str())))
        }
    }
}

/// What a composition is read for: the original content its feeder audit
/// keeps.
#[derive(Deserialize)]
struct Fed {
    /// The `feeder_audit`.
    feeder_audit: Option<Audit>,
}

/// A `FEEDER_AUDIT`.
#[derive(Deserialize)]
struct Audit {
    /// The `original_content`.
    original_content: Option<Content>,
}

/// A `DV_PARSABLE` original content.
#[derive(Deserialize)]
struct Content {
    /// The `value`.
    value: Option<String>,
}

/// A received collection, read for its dispensations' identifiers.
#[derive(Deserialize)]
struct Kept {
    /// Its entries.
    #[serde(default)]
    entry: Vec<Entry>,
}

/// One entry of a kept collection.
#[derive(Deserialize)]
struct Entry {
    /// Its resource.
    resource: Option<Resource>,
}

/// A kept resource's type and identifiers.
#[derive(Deserialize)]
struct Resource {
    /// The `resourceType`.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// The `identifier`s.
    #[serde(default)]
    identifier: Vec<Identifier>,
}

/// A kept identifier.
#[derive(Deserialize)]
struct Identifier {
    /// The system.
    system: Option<String>,
    /// The value.
    value: Option<String>,
}

/// Whether the composition `composition` keeps, in its feeder audit, a
/// received `MedicationDispense` of the identifier `system`|`value`.
fn keeps(composition: &str, (system, value): (&str, &str)) -> bool {
    // NOTE: RM 1.1.0 FEEDER_AUDIT.original_content: a composition the gateway did not write keeps
    // no received dispensation, so its content legitimately reads as none.
    let Some(kept) = serde_json::from_str::<Fed>(composition)
        .ok()
        .and_then(|fed| fed.feeder_audit?.original_content?.value)
        .and_then(|text| serde_json::from_str::<Kept>(&text).ok())
    else {
        return false;
    };
    kept.entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
        .filter(|resource| resource.resource_type.as_deref() == Some("MedicationDispense"))
        .flat_map(|resource| resource.identifier.iter())
        .any(|identifier| {
            identifier.system.as_deref() == Some(system)
                && identifier.value.as_deref() == Some(value)
        })
}

/// The answer to the dispensation `received` written in the map mode at
/// `writing` as one composition, which ended as `created`, with its record.
///
/// A composition the owner created answers `201 Created` with the
/// `OperationOutcome` naming the member and the version uid it created; the
/// face names no id to read it by, so the answer carries no `Location`, and
/// the outcome says so. The owner's refusal is `409`, an owner that did not
/// answer in time `504` saying the outcome is unknown, a request that never
/// left `503`, and any other failure `502`, each naming the member. The
/// member's ITS-REST error body is no `OperationOutcome`, so none of it is
/// quoted. No specification governs the statuses: our own design.
fn composed(
    federation: &Federation,
    (received, lookup): (&Received, &Lookup<'_>),
    writing: &Chosen,
    created: &Result<String, EhrCallError>,
) -> Response {
    let left = match created {
        Ok(_) => true,
        Err(error) => Contact::of_ehr_call_error(error).left(),
    };
    let (status, endpoint_status, outcome) = match created {
        Ok(version) => {
            Outcome::Created.record();
            let unlocated = format!(
                "the member created the composition {version}; in the map mode the face names no id to read the dispensation by, so the answer carries no Location"
            );
            let outcome = dispensations::written(
                received,
                (writing.node.as_str(), writing.native.as_str()),
                Some(&unlocated),
            );
            (StatusCode::CREATED, EndpointStatus::Active, outcome)
        }
        Err(error) => {
            let (status, endpoint_status, code, text) = unmade_composition(error);
            match status {
                StatusCode::CONFLICT => Outcome::Refused,
                StatusCode::GATEWAY_TIMEOUT => Outcome::Unknown,
                _ => Outcome::Failed,
            }
            .record();
            let diagnostics = format!("{} {text}", writing.native);
            (status, endpoint_status, face::outcome(code, &diagnostics))
        }
    };
    let mut response = crate::fhir::fhir_json(status, &outcome);
    let wrote = Wrote {
        writing,
        status: endpoint_status,
        left,
        made: created.is_ok(),
        own: None,
    };
    attach(&mut response, accessed(federation, lookup, Some(wrote)));
    response
}

/// The status, the §11.1 status, the issue type and the sentence after the
/// member's endpoint of a composition the owner did not create, `error`.
fn unmade_composition(error: &EhrCallError) -> (StatusCode, EndpointStatus, &'static str, String) {
    match error {
        EhrCallError::Rejected { status, .. } => (
            StatusCode::CONFLICT,
            EndpointStatus::Active,
            "conflict",
            format!("refused the dispensation ({status}), so nothing was written"),
        ),
        EhrCallError::TimeOut { .. } | EhrCallError::Unreachable { .. } => (
            StatusCode::GATEWAY_TIMEOUT,
            EndpointStatus::TimeOut,
            "timeout",
            String::from(
                "did not answer the write in time; the outcome is unknown, and the gateway never sends it again",
            ),
        ),
        EhrCallError::Withheld { .. }
        | EhrCallError::Expired(_)
        | EhrCallError::Capped(_)
        | EhrCallError::Conveyance { .. } => (
            StatusCode::SERVICE_UNAVAILABLE,
            EndpointStatus::NodeError,
            "transient",
            String::from("was not sent the write, so nothing was written"),
        ),
        _ => (
            StatusCode::BAD_GATEWAY,
            EndpointStatus::NodeError,
            "exception",
            String::from("failed the write, so the dispensation may not be written"),
        ),
    }
}
