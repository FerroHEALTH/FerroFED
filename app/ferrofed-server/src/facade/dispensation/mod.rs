// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation received on the FHIR face, written to the one member that
//! owns the prescription it names (Regulation (EU) 2025/327 Art 14(1)(c);
//! Federation Tier §2.3, §12.4, N23).
//!
//! The patient is resolved from every identifier of the dispensation's
//! `subject` in a namespace `fhir.receive_namespaces` names, as the face
//! resolves any patient ([`super::located`]), and every identifier must name
//! the same `{node, ehr_id}` set. Every member the deployment declares for
//! prescriptions that holds the patient is then asked for the patient's
//! prescriptions at its own endpoint, by its own `ehr_id` alone (§5.4.1,
//! N33), and the prescriptions the dispensation names are found among the
//! answers at the gateway ([`lookup`]), so no prescription identifier reaches
//! a member. Every such member must answer, since a silent one could be the
//! owner. The dispensation is then created at the owner's own endpoint, once
//! ([`ferrofed_engine::single_node::native_create`]), its body as the client
//! sent it (§5.4 scope note, N22). The member decides whether its state still
//! admits the dispensation; the gateway holds no lock and never sends a
//! write again.
//!
//! An `openEHR-federation-endpoint` header is a check only: one that names an
//! endpoint of another member than the owner refuses the dispensation. The
//! derivation of the owner, the answers of no owner and of two, and the check
//! are our own design: no specification governs a new object written at the
//! owner of the object it names (§12.4, N23 name an explicitly chosen node).

mod answer;
mod lookup;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::dispensations::{RESOURCE, Received};
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_create::CreateAsked;
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::answer::Failure;
use crate::facade::located::{Located, located};
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions;
use crate::facade::request::Arrived;
use crate::facade::summary::Unsummarised;
use crate::facade::{confined, security};
use crate::federation::Federation;

use answer::{Lookup, refused, refused_after};
use lookup::Owned;

/// The operation an access record names for a received dispensation: the
/// FHIR R4 `create` interaction on `MedicationDispense`.
pub(crate) const OPERATION: &str = "create MedicationDispense";

/// The one text every dispensation with no owner is answered with, whether
/// no member holds its patient, none may say so, or none holds the
/// prescription (RFC 9110 §15.5.5; Federation Tier §11.3).
pub(crate) const UNOWNED: &str =
    "no member holds the prescription this dispensation names for its patient";

/// Writes the dispensation `received`, whose text is `body`, as `arrived`
/// asks on the face `fhir`, checking the owner against the endpoints
/// `targeted` names when the request names any.
pub(crate) async fn dispense(
    federation: &Arc<Federation>,
    fhir: &FhirSettings,
    arrived: &Arrived<'_>,
    (received, body): (&Received, &[u8]),
    targeted: &BTreeSet<EndpointId>,
) -> Response {
    let logged = arrived.outbound.to_string();
    let named: Vec<(&str, &str)> = received
        .patient
        .iter()
        .filter(|(system, _)| {
            fhir.receive_namespaces
                .iter()
                .any(|namespace| namespace.as_str() == system)
        })
        .map(|(system, value)| (system.as_str(), value.as_str()))
        .collect();
    let Some(&first) = named.first() else {
        return refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "required",
            "the dispensation's subject carries no identifier in a namespace the gateway resolves",
        );
    };
    security::document_subject_consumed(&logged);
    let none = BTreeSet::new();
    let prescribing = fhir
        .serving
        .members_of(Category::Prescription)
        .unwrap_or(&none);
    let located = match resolved(federation, arrived, &named, prescribing).await {
        Ok(located) => located,
        Err(response) => return response,
    };
    let Some((located, withheld)) = located else {
        tracing::info!(
            request_id = logged,
            "a dispensation's patient is held by no member"
        );
        return refused(StatusCode::NOT_FOUND, "not-found", UNOWNED);
    };
    let snapshot = federation.snapshot();
    let unresolved: Vec<&EndpointId> = located
        .targets
        .unresolved
        .iter()
        .filter(|endpoint| {
            snapshot
                .endpoint(endpoint)
                .is_some_and(|held| prescribing.contains(held.node().as_str()))
        })
        .collect();
    if !unresolved.is_empty() {
        let named: Vec<&str> = unresolved
            .iter()
            .map(|endpoint| endpoint.as_str())
            .collect();
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            &format!(
                "the dispensation was not written: the patient could not be resolved at {}, which could hold the prescription (§11.3)",
                named.join(", ")
            ),
        );
    }
    let chosen: Vec<Chosen> =
        native::chosen_for(snapshot, &located.targets, Category::Prescription.code())
            .into_iter()
            .filter(|member| prescribing.contains(member.node.as_str()))
            .collect();
    if chosen.is_empty() {
        tracing::info!(
            request_id = logged,
            "a dispensation's patient is held by no member serving prescriptions"
        );
        return refused(StatusCode::NOT_FOUND, "not-found", UNOWNED);
    }
    let members = match prescriptions::asked(federation, arrived, chosen, Arc::clone(&withheld))
        .await
    {
        Ok(members) => members,
        Err(prescriptions::Unprescribed::Failure(failure)) => {
            return failure.respond(arrived.request_id, arrived.outbound);
        }
        Err(other) => {
            tracing::error!(request_id = logged, error = %other, "the prescription lookup could not be run");
            return crate::error::fixed(Code::Internal, arrived.request_id);
        }
    };
    let lookup = Lookup {
        patient: first,
        members: &members,
        set_aside: &located.targets.set_aside,
    };
    owned(
        (federation, fhir, arrived),
        (received, body),
        (&lookup, targeted),
        withheld,
    )
    .await
}

/// Writes the dispensation `received`, whose text is `body`, at the owner
/// `lookup` finds, once every member it asked answered, checking the owner
/// against `targeted`.
async fn owned(
    (federation, fhir, arrived): (&Arc<Federation>, &FhirSettings, &Arrived<'_>),
    (received, body): (&Received, &[u8]),
    (lookup, targeted): (&Lookup<'_>, &BTreeSet<EndpointId>),
    withheld: Arc<Withheld>,
) -> Response {
    let logged = arrived.outbound.to_string();
    let members = lookup.members;
    if let Some(silent) = members
        .iter()
        .find(|member| member.held.status() != EndpointStatus::Active)
    {
        let status = silent.held.status();
        let (answer, code) = match status {
            EndpointStatus::TimeOut | EndpointStatus::Offline => {
                (StatusCode::GATEWAY_TIMEOUT, "timeout")
            }
            _ => (StatusCode::SERVICE_UNAVAILABLE, "transient"),
        };
        return refused_after(
            federation,
            lookup,
            (answer, code),
            &format!(
                "the dispensation was not written: the prescription lookup had no answer from {} ({}), which could hold the prescription (§11.3)",
                silent.chosen.native,
                status.as_str()
            ),
        );
    }
    let owner = match lookup::owner(&received.named, members, &fhir.absolute) {
        Owned::At(owner) => owner,
        unowned => return answer::unowned(federation, lookup, unowned, &logged),
    };
    let Some(member) = members.get(owner) else {
        return crate::error::fixed(Code::Internal, arrived.request_id);
    };
    let Some(writing) = writing(federation, fhir, &member.chosen) else {
        return refused_after(
            federation,
            lookup,
            (StatusCode::UNPROCESSABLE_ENTITY, "business-rule"),
            "the member that holds the prescription does not receive dispensations on this deployment",
        );
    };
    if let Some(other) = targeted
        .iter()
        .find(|endpoint| !of_node(federation, endpoint, &writing.node))
    {
        return refused_after(
            federation,
            lookup,
            (StatusCode::BAD_REQUEST, "business-rule"),
            &format!(
                "openEHR-federation-endpoint names {other}, which is no endpoint of the member that owns the prescription, so nothing was written (§8.4)"
            ),
        );
    }
    if !confined::admits(arrived.conveyance, &writing.via, &writing.ehr_id) {
        return confined::refused("dispensation", arrived.request_id, &logged);
    }
    let created = write(federation, arrived, (&writing, body), withheld).await;
    answer::written(
        federation,
        (received, lookup),
        (&writing, &created),
        fhir,
        &logged,
    )
}

/// The patient the identifiers `named` name, located once by the first and
/// held to name the same `{node, ehr_id}` set among `prescribing` by every
/// other, with every value the outbound gate withholds; `None` when no
/// member holds the patient or none may say so.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
async fn resolved(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    named: &[(&str, &str)],
    prescribing: &BTreeSet<String>,
) -> Result<Option<(Located, Arc<Withheld>)>, Response> {
    let mut first: Option<(Located, BTreeMap<NodeId, EhrId>, Withheld)> = None;
    let mut unknown = false;
    for &patient in named {
        let held = match located(federation, arrived, patient).await {
            Ok(held) => Some(held),
            Err(Unsummarised::Failure(Failure::NoDestination)) => None,
            Err(Unsummarised::Failure(failure)) => {
                return Err(failure.respond(arrived.request_id, arrived.outbound));
            }
            Err(Unsummarised::Internal(what)) => {
                tracing::error!(request_id = %arrived.outbound, what, "a dispensation's patient could not be located");
                return Err(crate::error::fixed(Code::Internal, arrived.request_id));
            }
            Err(Unsummarised::NotFound(_) | Unsummarised::Incomplete { .. }) => {
                return Err(crate::error::fixed(Code::Internal, arrived.request_id));
            }
        };
        let Some(held) = held else {
            unknown = true;
            continue;
        };
        let pairs: BTreeMap<NodeId, EhrId> = held
            .targets
            .resolved
            .iter()
            .filter(|(node, _)| prescribing.contains(node.as_str()))
            .cloned()
            .collect();
        let withheld = held.targets.plan.withheld();
        first = match first {
            None => Some((held, pairs, (*withheld).clone())),
            Some((kept, kept_pairs, kept_withheld)) => {
                if kept_pairs != pairs {
                    return Err(disagree());
                }
                let joined = kept_withheld.joined(&withheld);
                Some((kept, kept_pairs, joined))
            }
        };
    }
    match (first, unknown) {
        (None, _) => Ok(None),
        // NOTE: Regulation (EU) 2025/327 Art 13(3): an identifier no member knows beside one a
        // member does may name another person, so nothing is written.
        (Some(_), true) => Err(disagree()),
        (Some((located, _, withheld)), false) => Ok(Some((located, Arc::new(withheld)))),
    }
}

/// The refusal of a dispensation whose patient identifiers name different
/// patients.
fn disagree() -> Response {
    refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        "business-rule",
        "the identifiers of the dispensation's subject do not name one patient at every member",
    )
}

/// The owner `chosen`, as the dispensation reaches it: its own endpoint for
/// dispensations, when the deployment declares it for them and the registry
/// holds one.
fn writing(federation: &Federation, fhir: &FhirSettings, chosen: &Chosen) -> Option<Chosen> {
    let declared = fhir.serving.members_of(Category::Dispensation)?;
    if !declared.contains(chosen.node.as_str()) {
        return None;
    }
    let native = federation
        .snapshot()
        .native_for(&chosen.node, Category::Dispensation.code())?;
    Some(Chosen {
        via: chosen.via.clone(),
        node: chosen.node.clone(),
        native: native.id().clone(),
        ehr_id: chosen.ehr_id.clone(),
    })
}

/// Whether `endpoint` is a Query API or an own endpoint of `node`.
fn of_node(federation: &Federation, endpoint: &EndpointId, node: &NodeId) -> bool {
    let snapshot = federation.snapshot();
    snapshot
        .endpoint(endpoint)
        .map(|held| held.node() == node)
        .or_else(|| {
            snapshot
                .native_endpoint(endpoint)
                .map(|held| held.node() == node)
        })
        .unwrap_or(false)
}

/// Creates the dispensation `body` at `writing`, once, under the budget of
/// the request `arrived`, refusing to send anything carrying one of
/// `withheld` outside the body.
async fn write(
    federation: &Federation,
    arrived: &Arrived<'_>,
    (writing, body): (&Chosen, &[u8]),
    withheld: Arc<Withheld>,
) -> answer::Written {
    let budget = federation.budget();
    let Some(overall) = arrived.started.checked_add(budget.overall()) else {
        return answer::Written::Unsent;
    };
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    let options = DispatchOptions::new(per_node, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(withheld);
    let snapshot = federation.snapshot();
    let (Some(client), Some(native)) = (
        federation.clients().get(&writing.via),
        snapshot.native_endpoint(&writing.native),
    ) else {
        return answer::Written::Unsent;
    };
    let asked = CreateAsked {
        native,
        ehr_id: &writing.ehr_id,
        resource: RESOURCE,
        body,
    };
    let until = tokio::time::Instant::from_std(overall);
    answer::Written::Sent(client.native_create(asked, &options, until).await)
}
