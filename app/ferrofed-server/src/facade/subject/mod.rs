// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {base}/v1/ehr?subject_id=…&subject_namespace=…`: the EHR of a
//! subject, answered by the one member that holds it (ITS-REST 1.1.0 EHR
//! API, `ehr_get_by_subject`).
//!
//! The two query parameters name the patient, which N33 forbids the gateway
//! to dispatch, so they are resolution input (§5.2, N3). The gateway resolves
//! the subject at the members through the cross-reference service and sends
//! the member that knows it `GET {base}/v1/ehr/{ehr_id}` under its own local
//! `ehr_id`. That request carries no query string, no body and only the
//! headers its operation declares, and the outbound gate holds it to the
//! subject value as well (§5.4.1, N33). The answer passes through as the node
//! sent it, naming the acting endpoint (N31, §9.6).
//!
//! The targeting headers select the one endpoint the subject is resolved at
//! (§8.4, §7a.1); without them, every member with an active endpoint is a
//! candidate, narrowed by the localizer where one is configured: the read
//! names the patient and no node, so it is undirected (N4, §14.1). The
//! consent pre-filter is asked about the candidates first,
//! as a federated query asks it: a member it denies is never resolved or
//! contacted, and one it does not deny is left to its node (N27a, N27). The resolution settles the answer:
//!
//! - one member knows the subject: its EHR, forwarded once;
//! - several members know it: `409` listing their endpoints, because the
//!   gateway never chooses by where the patient resolved (§12.5.2);
//! - the cross-reference could not answer for a member: `424`, because that
//!   member may hold the EHR too (§11.2, §11.5);
//! - the consent pre-filter denied every member that might hold it, and no
//!   other member knows it: `403 consent-denied` naming the denied endpoints
//!   (N27a; no specification governs the answer: our own design);
//! - the localizer could not answer and the deployment fails closed: `424`
//!   with no member asked, because a holder may be among them (§14.1);
//! - the demographics step knows no patient for the subject: `404`; it names
//!   no one master identity or could not answer: `424`, as a localizer
//!   outage when an undirected read fails closed (Annex A §A.2, §14.1);
//! - no member knows it: `404`, the operation's own answer for a subject with
//!   no EHR (ITS-REST 1.1.0 `404_EHR_subject`, §11.2).
//!
//! A deployment that does not disclose consent exclusions resolves a denied
//! member with the others, never contacts it, and never counts it a holder.
//! A subject no member it may ask holds an EHR for is then `404
//! subject-unavailable`, whether no member knows the subject or only a denied
//! one does, so the answer never shows that a restriction exists (Regulation
//! (EU) 2025/327 Art 8; RFC 9110 §15.5.5).
//!
//! Every `{node, ehr_id}` pair the resolution produced teaches the `ehr_id`
//! index (§12.5.1 step 3). No answer and no log line names the subject: an
//! error body names endpoints and query parameter positions only (§5.4.3).

mod resolved;
mod unserved;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use ferrofed_engine::declared::{self, query};
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::forward::{ClientRequest, HeldRequest};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::conveyance::Conveyance;
use ferrofed_identity::binding::SessionKey;
use ferrofed_identity::localizer::OnFailure;
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use http::{HeaderMap, Method};
use openehr_its::rest::client::path_segment;
use openehr_its::rest::generated::ehr::EhrGetBySubjectParams;
use openehr_its::rest::routes::RouteMatch;
use secrecy::SecretString;

use crate::error::{self, Code};
use crate::facade::demographics::{self, Identified};
use crate::facade::localize::{Localized, localize};
use crate::facade::owner;
use crate::facade::provenance::Provenance;
use crate::facade::route::{self, Arrived, Deadlines, Failure};
use crate::facade::security;
use crate::facade::subject::resolved::resolve;
use crate::facade::subject::unserved::Unserved;
use crate::facade::{confined, consent};
use crate::federation::Federation;

/// The ITS-REST operation that reads an EHR by its subject.
const OPERATION: &str = "ehr_get_by_subject";

/// Whether `matched` is `GET {base}/v1/ehr`, the read of an EHR by subject.
pub(crate) fn serves(matched: &RouteMatch) -> bool {
    matched.operation_id == OPERATION
}

/// Answers the read of an EHR by subject: resolves the subject and forwards
/// `GET {base}/v1/ehr/{ehr_id}` to the one member that holds it.
///
/// The query string and the headers are held to what the operation declares
/// before anything is resolved, and the subject reaches the cross-reference
/// service alone (§5.2, §5.4.1, N33).
pub(crate) async fn serve(
    federation: &Federation,
    arrived: Arrived<'_>,
    matched: &RouteMatch,
) -> Response {
    let started = Instant::now();
    let request_id = arrived.request_id;
    // NOTE: §5.4.1, N33: the client's id is free text that may carry an
    // identifier, so every log event names the gateway's own id instead.
    let logged = arrived.outbound.to_string();
    let subject = match Subject::of(matched, arrived.uri.query()) {
        Ok(subject) => subject,
        Err(unserved) => return unserved.respond(request_id, &logged),
    };
    if let Err(refusal) = declared::held(matched, None, arrived.headers, &arrived.body) {
        return route::declared_refused(&refusal, request_id, &logged);
    }
    security::subject_consumed(&logged);
    let Some(budget) = Deadlines::from(federation, started) else {
        return Unserved::Clock.respond(request_id, &logged);
    };
    let snapshot = federation.snapshot();
    let (candidates, directed) = match candidates(snapshot, arrived.headers) {
        Ok(candidates) => candidates,
        Err(unserved) => return unserved.respond(request_id, &logged),
    };
    let named = (&subject.patient, directed, budget.overall());
    let ids = (request_id, logged.as_str());
    let master = match admitted(federation, &arrived.conveyance, named, ids).await {
        Ok(master) => master,
        Err(refused) => return *refused,
    };
    let patient = master.as_ref().unwrap_or(&subject.patient);
    let located = localized(
        federation,
        (patient, budget.overall()),
        candidates,
        directed,
    )
    .await;
    let Some((candidates, members)) = located else {
        return Unserved::Unlocalized.respond(request_id, &logged);
    };
    let consented = consent::prefilter(
        federation,
        (patient, arrived.requester),
        &members,
        budget.overall(),
    )
    .await;
    let unbound = (patient, &consented.denied);
    let resolved = resolve(federation, candidates, unbound, budget.overall()).await;
    let holders = resolved.holders.clone();
    let settled = resolved
        .settled()
        .map_err(|unserved| unserved.disclosed_as(federation.discloses_consent()));
    // NOTE: §5.2, §12.5: a confined grant reads its own patient's EHR alone, and the gateway
    // records nothing of another subject, no session binding and no index entry.
    if confined::is_confined(&arrived.conveyance)
        && !settled.as_ref().is_ok_and(|(endpoint, ehr_id)| {
            confined::admits(&arrived.conveyance, endpoint.id(), ehr_id)
        })
    {
        return confined::refused("subject", request_id, &logged);
    }
    let session = arrived.session.map(|session| (session, &consented.denied));
    learn(federation, &holders, session, started);
    let (endpoint, ehr_id) = match settled {
        Ok(owner) => owner,
        Err(unserved) => return unserved.respond(request_id, &logged),
    };
    tracing::debug!(
        endpoint = %endpoint.id(),
        request_id = logged,
        "routed the read of an EHR by subject to the member that resolved it"
    );
    let withheld = Arc::new(subject.withheld(master.as_ref()));
    let options = DispatchOptions::new(budget.per_node(), arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(withheld)
        .with_composed_ehr_id(ehr_id.clone());
    // NOTE: §5.4.1, N33: the node is located by its own ehr_id alone, so the
    // request is the gateway's own, with none of the client's query or body.
    let request = ClientRequest {
        method: Method::GET,
        path: format!("/ehr/{}", path_segment(&ehr_id.as_str())),
        query: None,
        headers: arrived.headers.clone(),
        body: Vec::new(),
    };
    let provenance = Provenance::of(snapshot, endpoint);
    let forwarded = match HeldRequest::hold(request) {
        Ok(request) => route::send(federation, endpoint, request, &options, &logged).await,
        Err(refused) => Err(Failure::Forward(refused)),
    };
    match forwarded {
        // NOTE: Regulation (EU) 2025/327 Art 8: a withheld refusal answers as no holder would, so
        // it names no acting endpoint (no specification governs this: our own design).
        Ok(forwarded) => route::withheld(federation, endpoint, &forwarded, (request_id, &logged))
            .unwrap_or_else(|| {
                route::learn(federation, (&ehr_id, None), endpoint, &forwarded, &logged);
                provenance.stamp(route::answered(forwarded))
            }),
        Err(Failure::Internal) => error::fixed(Code::Internal, request_id),
        Err(Failure::Forward(failure)) => {
            route::failed(&failure, provenance, (request_id, &logged))
        }
    }
}

/// The `candidates` the localizer names for `patient` before `deadline`, with
/// their members, or every candidate for a `directed` read; `None` when the
/// localizer failed closed.
async fn localized<'a>(
    federation: &Federation,
    (patient, deadline): (&PatientRef, Instant),
    candidates: Vec<&'a Endpoint>,
    directed: bool,
) -> Option<(Vec<&'a Endpoint>, Vec<NodeId>)> {
    // NOTE: N4, §5.2, §14.1: a read that names the patient and no node is undirected,
    // so the localizer narrows which members learn of the patient; a targeted one is not (§8).
    let located = if directed {
        Localized::everyone()
    } else {
        let members: Vec<NodeId> = candidates
            .iter()
            .map(|endpoint| endpoint.node().clone())
            .collect();
        localize(federation, patient, &members, deadline).await
    };
    if located.failed_closed() {
        return None;
    }
    let candidates: Vec<&Endpoint> = candidates
        .into_iter()
        .filter(|endpoint| located.admits(endpoint.node()))
        .collect();
    let members = candidates
        .iter()
        .map(|endpoint| endpoint.node().clone())
        .collect();
    Some((candidates, members))
}

/// The master identity of the subject `patient` before `deadline`, or `None`
/// for the subject as named, once a confined caller is held to its own
/// patient: the refusal otherwise, answered under `request_id` and logged
/// under `logged`.
async fn admitted(
    federation: &Federation,
    conveyance: &Conveyance,
    (patient, directed, deadline): (&PatientRef, bool, Instant),
    (request_id, logged): (&str, &str),
) -> Result<Option<PatientRef>, Box<Response>> {
    let named = (patient, deadline);
    if let Some(refused) = other_patient(federation, conveyance, named, (request_id, logged)).await
    {
        return Err(Box::new(refused));
    }
    identified(federation, patient, directed, deadline)
        .await
        .map_err(|unserved| Box::new(unserved.respond(request_id, logged)))
}

/// The master identity the demographics step finds for `patient` before
/// `deadline`, or `None` when no step applies (Annex A §A.2).
///
/// # Errors
///
/// Returns [`Unserved::Nowhere`] when the service knows no patient for the
/// identifier, and [`Unserved::Unidentified`] when its answer names no one
/// master identity or it could not answer. An outage on an undirected read
/// that fails closed is reported as localization's own outage, as a
/// localizer's is (§14.1, N4); a `directed` read is never localized.
async fn identified(
    federation: &Federation,
    patient: &PatientRef,
    directed: bool,
    deadline: Instant,
) -> Result<Option<PatientRef>, Unserved> {
    match demographics::identify(federation, patient, deadline).await {
        Identified::AsNamed => Ok(None),
        Identified::Master(master) => Ok(Some(master)),
        Identified::NoMatch(_) => {
            Err(Unserved::Nowhere.disclosed_as(federation.discloses_consent()))
        }
        Identified::Ambiguous(error) => Err(Unserved::Unidentified {
            reason: error,
            closed: false,
        }),
        Identified::Unavailable {
            failure,
            audit_failed,
        } => {
            let closed = !directed
                && (audit_failed || federation.localization().on_failure() == OnFailure::Closed);
            Err(Unserved::Unidentified {
                reason: failure,
                closed,
            })
        }
    }
}

/// The refusal of a read by subject whose caller is confined to one patient
/// and names another, or `None` to go on: the subject is resolved at the
/// bound member alone before `deadline`, ahead of any localizer, consent
/// pre-filter or other member (§5.2).
async fn other_patient(
    federation: &Federation,
    conveyance: &Conveyance,
    (patient, deadline): (&PatientRef, Instant),
    (request_id, logged): (&str, &str),
) -> Option<Response> {
    let confinement = conveyance.confinement()?;
    match confined::names_own(federation, confinement, patient, deadline).await {
        Ok(true) => None,
        Ok(false) => Some(confined::refused("subject", request_id, logged)),
        Err(unconfined) => Some(unconfined.respond(request_id, logged)),
    }
}

/// The subject a request names, consumed as resolution input.
///
/// `Debug` shows the namespace and redacts the identifier.
#[derive(Debug)]
struct Subject {
    /// The patient reference the cross-reference service is asked about.
    patient: PatientRef,
    /// The identifier itself, which the outbound gate withholds.
    value: SecretString,
}

impl Subject {
    /// The subject the query string `query` of `matched` names.
    ///
    /// Each parameter must be one the operation declares, and the query
    /// string is decoded by the operation's generated parameters: `subject_id`
    /// and `subject_namespace` each given once (ITS-REST 1.1.0, both
    /// `required`; a `form` scalar is one pair), each percent-decoded to UTF-8
    /// text (RFC 3986 §2.1, so a `+` is a literal plus).
    fn of(matched: &RouteMatch, query: Option<&str>) -> Result<Self, Unserved> {
        if let Some(query) = query {
            query::every_declared(matched, query).map_err(|unlisted| Unserved::Undeclared {
                position: unlisted.position,
            })?;
        }
        // NOTE: no specification governs this: our own design; the headers are held
        // by `declared::held`, so the generated decoder reads the query string alone.
        let params = EhrGetBySubjectParams::from_request(matched, query, &HeaderMap::new())
            .map_err(Unserved::Malformed)?;
        let namespace =
            IdentifierNamespace::new(params.subject_namespace).map_err(Unserved::Patient)?;
        let value = SecretString::from(params.subject_id);
        let patient = PatientRef::new(namespace, value.clone()).map_err(Unserved::Patient)?;
        Ok(Self { patient, value })
    }

    /// What the outbound gate withholds from the request to the node: the
    /// subject's identifier, and the `master` identity the demographics step
    /// found for it, which is as identifying (§5.4.1, N33).
    fn withheld(self, master: Option<&PatientRef>) -> Withheld {
        let mut values = vec![self.value];
        values.extend(master.map(PatientRef::withheld));
        Withheld::new(values)
    }
}

/// The endpoints the subject is resolved at: the one the targeting headers
/// name, or the endpoint each member is asked through (§8.4, §11.1), and
/// whether the headers named it.
///
/// # Errors
///
/// Returns [`Unserved::Target`] when the targeting headers name no one
/// registry endpoint (§8.4.1), and [`Unserved::Suspended`] when they name a
/// suspended one, which §11.1 never contacts.
fn candidates<'a>(
    snapshot: &'a RegistrySnapshot,
    headers: &HeaderMap,
) -> Result<(Vec<&'a Endpoint>, bool), Unserved> {
    if let Some(endpoint) = owner::targeted(snapshot, headers)? {
        if endpoint.status() == EndpointStatus::Suspended {
            return Err(Unserved::Suspended);
        }
        return Ok((vec![endpoint], true));
    }
    let every = snapshot
        .nodes()
        .filter_map(|node| snapshot.asked_through(node.id()))
        .collect();
    Ok((every, false))
}

/// Teaches the `ehr_id` index, and the verified caller's resolution
/// bindings, every `{node, ehr_id}` pair `holders` names, as a federated
/// query's resolution does (§12.5.1 steps 2 and 3).
///
/// With a `session`, the bindings it holds that name a member the consent
/// pre-filter denied are dropped first (N27a), as a query's are.
fn learn(
    federation: &Federation,
    holders: &[(&Endpoint, EhrId)],
    session: Option<(&SessionKey, &BTreeSet<NodeId>)>,
    now: Instant,
) {
    if let Some((session, denied)) = session {
        federation.bindings().forget_denied(session, denied);
        federation.bindings().record(
            session,
            now,
            holders
                .iter()
                .map(|(endpoint, ehr_id)| (endpoint.node(), ehr_id)),
        );
    }
    for (endpoint, ehr_id) in holders {
        owner::learn(federation.index(), ehr_id, endpoint.node());
    }
}
