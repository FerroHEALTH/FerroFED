// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document received in the exchange format, written as one composition to
//! the member the deployment declares for its category (Regulation (EU)
//! 2025/327 Annex II 2.2 and 2.3; Federation Tier §2.3, §12.4, N23).
//!
//! The component prepares the document ([`ferrofed_eehrxf::receive`]): it
//! reads it, names its category, holds it to the category's profiles and
//! maps it into one composition that keeps the original. Nothing has left
//! the gateway then. The patient is resolved next, from every identifier of
//! the document's `Patient` in a namespace `fhir.receive_namespaces` names,
//! through the demographics step and the resolver, at the category's member
//! alone (§5.2, N32): every identifier must name the same `ehr_id` there, so
//! a document whose identifiers name two people, or a patient the member
//! holds no EHR for, is refused with nothing sent to any member. A caller
//! whose grant is confined to one patient writes only into that patient's
//! EHR. The composition is then committed with ITS-REST `composition_create`
//! through the generated client, located by the member's own `ehr_id`: the
//! outbound gate holds the path and every header to the document's patient
//! identifiers (§5.4.1, N33). The body keeps the received document whole,
//! its identifiers included, as the clinical content the client supplied
//! (§5.4 scope note).
//!
//! Every answer is an `OperationOutcome`. One whose request left for the
//! member carries its access record, of the document's Art 14(1) category by
//! construction (Annex II 3.2); every other one states that no member was
//! sent anything.

use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use ehds_logging::classify::{Basis, Evidence, RootObject};
use ehds_logging::record::{
    Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup, WrittenVersion,
};
use ferrofed_eehrxf::receive::{self, Identifier, Prepared, Written};
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::{Acting, Principal};
use ferrofed_engine::dispatch::{Contact, DispatchOptions};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::Resolution;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::Endpoint;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use secrecy::{ExposeSecret, SecretString};

use crate::access::{self, Accessed, Asked};
use crate::config::fhir::FhirSettings;
use crate::facade::demographics::{self, Identified};
use crate::facade::receive::refusal::{Unresolved, attach, failed, refused, unprepared};
use crate::facade::request::Arrived;
use crate::facade::route::Deadlines;
use crate::facade::{confined, security};
use crate::federation::Federation;

mod refusal;

/// The operation an access record names for a received document: the FHIR
/// `create` interaction on `Bundle` (FHIR R4 documents §3.3.4).
pub(crate) const OPERATION: &str = "create Bundle";

/// Receives `text`, a document in the exchange format, for the caller
/// `arrived` names, and answers with what happened.
pub(crate) async fn receive(
    federation: &Federation,
    fhir: &FhirSettings,
    arrived: Arrived<'_>,
    text: &str,
) -> Response {
    let logged = arrived.outbound.to_string();
    let now = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    // NOTE: no specification governs this: our own design; the gateway's face is the
    // system the composition's FEEDER_AUDIT names as having transformed the document.
    let prepared = match fhir
        .receivers
        .receive(text, (&fhir.absolute, &now), &fhir.terminology)
        .await
    {
        Ok(prepared) => prepared,
        Err(unreceived) => return unprepared(&unreceived, &logged),
    };
    let Some(deadlines) = Deadlines::from(federation, arrived.started) else {
        return refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            "exception",
            "the request's deadline cannot be represented",
        );
    };
    let snapshot = federation.snapshot();
    let Some(endpoint) = NodeId::new(prepared.member.as_str())
        .ok()
        .and_then(|node| snapshot.asked_through(&node))
    else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            &format!(
                "member {} of the category {} has no endpoint it is reached through",
                prepared.member,
                prepared.category.code()
            ),
        );
    };
    security::document_subject_consumed(&logged);
    let resolved = match resolve(
        (federation, fhir),
        &arrived,
        (&prepared, endpoint),
        &deadlines,
    )
    .await
    {
        Ok(resolved) => resolved,
        Err(unresolved) => return unresolved.respond(&prepared, (arrived.request_id, &logged)),
    };
    // NOTE: SMART on openEHR master07 §Context Selection, Federation Tier §5.2: a grant
    // confined to one patient writes into that patient's own EHR at the member alone.
    if !confined::admits(arrived.conveyance, endpoint.id(), &resolved.ehr_id) {
        return confined::refused("receive", arrived.request_id, &logged);
    }
    let mut prepared = prepared;
    let inserted = in_person(arrived.conveyance);
    // NOTE: Regulation (EU) 2025/327 Art 5, RM Release 1.1.0 ENTRY.provider: what a person
    // inserts under their own grant names the record's subject as each entry's provider.
    if inserted {
        prepared.composition = match prepared.composition.inserted_by_subject() {
            Ok(marked) => marked,
            Err(unmarked) => return refusal::unmarked(&unmarked, &logged),
        };
    }
    write(
        federation,
        &arrived,
        (&prepared, endpoint),
        (resolved, inserted),
        &deadlines,
    )
    .await
}

/// Whether the caller under `conveyance` is a natural person whose grant is
/// confined to their own record: the grant a person inserts information in
/// their own EHR with (Regulation (EU) 2025/327 Art 5).
fn in_person(conveyance: &Conveyance) -> bool {
    conveyance.confinement().is_some()
        && matches!(
            conveyance.principal(),
            Principal::Caller(caller) if caller.acting == Acting::Person
        )
}

/// The patient of a prepared document, resolved at its member.
struct Resolved {
    /// The member's own `ehr_id` for the patient.
    ehr_id: EhrId,
    /// The identifier the patient was named by, for the access record.
    named: PatientIdentifier,
    /// Every value the outbound gate withholds: each identifier of the
    /// document's patient, and each master identity found for one.
    withheld: Vec<SecretString>,
}

/// Resolves the patient of `prepared` at `endpoint`'s member, in the
/// namespaces `fhir` names, on behalf of the caller `arrived` names, before
/// the overall deadline of `deadlines`.
async fn resolve(
    (federation, fhir): (&Federation, &FhirSettings),
    arrived: &Arrived<'_>,
    (prepared, endpoint): (&Prepared, &Endpoint),
    deadlines: &Deadlines,
) -> Result<Resolved, Unresolved> {
    let mut withheld: Vec<SecretString> = prepared
        .subject
        .iter()
        .map(|identifier| SecretString::from(identifier.value.clone()))
        .collect();
    let named: Vec<(PatientRef, &Identifier)> = prepared
        .subject
        .iter()
        .filter(|identifier| {
            fhir.receive_namespaces
                .iter()
                .any(|namespace| namespace.as_str() == identifier.system)
        })
        .filter_map(|identifier| {
            // NOTE: Federation Tier §5.2; an identifier no PatientRef forms names no
            // patient a resolver can be asked about, so it is no subject key.
            let namespace = IdentifierNamespace::new(identifier.system.clone()).ok()?;
            let patient =
                PatientRef::new(namespace, SecretString::from(identifier.value.clone())).ok()?;
            Some((patient, identifier))
        })
        .collect();
    let Some((_, first)) = named.first() else {
        return Err(Unresolved::NoIdentifier);
    };
    let first = PatientIdentifier {
        namespace: first.system.clone(),
        value: SecretString::from(first.value.clone()),
    };
    let resolver = federation.resolver().ok_or(Unresolved::NoResolver)?;
    if let Some(confinement) = arrived.conveyance.confinement() {
        // NOTE: Federation Tier §5.2: a grant confined to one patient is held to it at the
        // bound member first, so no other member is asked about another patient.
        for (patient, _) in &named {
            let own = confined::names_own(
                federation,
                confinement,
                patient,
                arrived.on_behalf,
                deadlines.overall(),
            )
            .await
            .map_err(Unresolved::Unconfined)?;
            if !own {
                return Err(Unresolved::OtherPatient);
            }
        }
    }
    let member = endpoint.node().clone();
    let mut found: Option<EhrId> = None;
    let mut unknown = false;
    for (patient, _) in &named {
        let master = match demographics::identify(
            federation,
            (patient, arrived.on_behalf),
            deadlines.overall(),
        )
        .await
        {
            Identified::AsNamed => None,
            Identified::Master(master) => Some(master),
            Identified::NoMatch(_) => {
                unknown = true;
                continue;
            }
            Identified::Ambiguous(_) => return Err(Unresolved::Disagree),
            Identified::Unavailable { .. } => return Err(Unresolved::Unavailable),
        };
        withheld.extend(master.as_ref().map(PatientRef::withheld));
        let asked = master.as_ref().unwrap_or(patient);
        let mut answers = resolver
            .resolve(
                asked,
                std::slice::from_ref(&member),
                arrived.on_behalf,
                deadlines.overall(),
            )
            .await;
        match answers.remove(&member) {
            Some(Resolution::Resolved(ehr_id)) => {
                if found.as_ref().is_some_and(|held| *held != ehr_id) {
                    return Err(Unresolved::Disagree);
                }
                found = Some(ehr_id);
            }
            Some(Resolution::Unknown) => unknown = true,
            Some(Resolution::Unavailable(_)) | None => return Err(Unresolved::Unavailable),
        }
    }
    match (found, unknown) {
        (Some(ehr_id), false) => Ok(Resolved {
            ehr_id,
            named: first,
            withheld,
        }),
        // NOTE: Regulation (EU) 2025/327 Art 13(3): an identifier the member does not know
        // beside one it does may name another person, so the write is refused.
        (Some(_), true) => Err(Unresolved::Disagree),
        (None, _) => Err(Unresolved::NoEhr),
    }
}

/// Commits the composition of `prepared` into `resolved`'s EHR at
/// `endpoint`, within the per-node deadline of `deadlines`; `inserted` says
/// the composition is marked as inserted by its subject.
async fn write(
    federation: &Federation,
    arrived: &Arrived<'_>,
    (prepared, endpoint): (&Prepared, &Endpoint),
    (resolved, inserted): (Resolved, bool),
    deadlines: &Deadlines,
) -> Response {
    let logged = arrived.outbound.to_string();
    let Some(client) = federation.clients().get(endpoint.id()) else {
        tracing::error!(
            endpoint = %endpoint.id(),
            request_id = logged,
            "a registry endpoint has no node client"
        );
        return refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            "exception",
            "the member's endpoint has no client",
        );
    };
    let Resolved {
        ehr_id,
        named,
        withheld,
    } = resolved;
    let options = DispatchOptions::new(deadlines.per_node(), arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(Arc::new(Withheld::new(withheld)))
        .with_composed_ehr_id(ehr_id.clone());
    let started = Instant::now();
    let created = client
        .create_composition(ehr_id.as_str(), prepared.composition.model(), &options)
        .await;
    let contact = match &created {
        Ok(_) => Contact::Answered(StatusCode::CREATED),
        Err(error) => Contact::of_ehr_call_error(error),
    };
    federation.dependencies().contacted(endpoint.id(), contact);
    tracing::debug!(
        endpoint = %endpoint.id(),
        request_id = logged,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "wrote a received document's composition to its member"
    );
    let at = Origin {
        endpoint,
        ehr_id: &ehr_id,
        named,
        inserted,
    };
    match created {
        Ok(version) => {
            let written = Written {
                member: &prepared.member,
                endpoint: endpoint.id().as_str(),
                version: &version,
            };
            let mut response =
                crate::fhir::fhir_json(StatusCode::OK, &receive::written(prepared, written));
            attach(
                &mut response,
                accessed(
                    federation,
                    prepared,
                    &at,
                    (
                        Some(&access::version::named(&version)),
                        EndpointStatus::Active,
                    ),
                ),
            );
            response
        }
        Err(error) => failed(federation, prepared, &at, &error, &logged),
    }
}

/// Where a write was sent: the endpoint, the `ehr_id` and the identifier the
/// patient was named by.
struct Origin<'a> {
    /// The member's endpoint.
    endpoint: &'a Endpoint,
    /// The member's own `ehr_id`.
    ehr_id: &'a EhrId,
    /// The identifier the patient was named by.
    named: PatientIdentifier,
    /// That the composition is marked as inserted by its subject.
    inserted: bool,
}

/// The access record's facts of a write of `prepared` sent to `at`, which
/// left `written` when the member took it and whose member answered as the
/// §11.1 `status` names, or `None` when the federation keeps no access log
/// (Annex II 3.2).
///
/// The origin's status and the version are named as a routed write's are,
/// so both kinds of write read alike in the log.
fn accessed(
    federation: &Federation,
    prepared: &Prepared,
    at: &Origin<'_>,
    (written, status): (Option<&WrittenVersion>, EndpointStatus),
) -> Option<Accessed> {
    let log = federation.access_log()?;
    let model = prepared.composition.model();
    let object = RootObject {
        template_id: Some(prepared.composition.template_id().to_owned()),
        archetype_id: Some(model.archetype_node_id.clone()),
        version_uid: written.and_then(WrittenVersion::named).map(str::to_owned),
    };
    let category = ehds_logging::category::Category::priority(prepared.category.code());
    let mut evidence = Evidence::reached(Basis::Written, vec![object]);
    if let Some(category) = category {
        evidence = evidence.constructed(category);
    }
    let endpoint = at.endpoint.id().as_str().to_owned();
    let snapshot = federation.snapshot();
    let system_id = snapshot
        .node(at.endpoint.node())
        .map(|node| node.system_id().as_str().to_owned());
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Create,
        operation: OPERATION,
        resource: None,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: Some(PatientIdentifier {
                namespace: at.named.namespace.clone(),
                value: SecretString::from(at.named.value.expose_secret().to_owned()),
            }),
            ehrs: vec![EhrAt {
                endpoint: endpoint.clone(),
                ehr_id: at.ehr_id.as_str().to_owned(),
                patient: PatientLookup::RequestNamed,
            }],
            population: None,
        },
        evidence: evidence.clone(),
        delivered: Some(usize::from(written.is_some())),
        origins: vec![Asked {
            endpoint,
            node: Some(at.endpoint.node().as_str().to_owned()),
            system_id,
            status: status.as_str().to_owned(),
            rows: Some(usize::from(written.is_some())),
            version: written.cloned(),
            created_ehr: None,
            contributed: written.is_some(),
            evidence: written.is_some().then_some(evidence),
        }],
        consent_set_aside: std::collections::BTreeSet::new(),
        subject_inserted: at.inserted && written.is_some(),
        transformations: vec![access::transformation::of(&prepared.transformation)],
    })
}
