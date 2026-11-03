// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers to a received dispensation, each with the access record of
//! what reached a member.
//!
//! A dispensation the owner created answers `201 Created` with a `Location`
//! naming it by the face's own id, `{fhir-base}/MedicationDispense/{id}`,
//! and its version when the member named one (FHIR R4 `http.html`
//! §3.1.0.8); the body is the `OperationOutcome` naming the member, as the
//! face's other receipt answers, with every constraint the check could not
//! evaluate. The owner's refusal is `409`, and an owner that did not answer
//! in time is `504` saying the outcome is unknown, since the gateway never
//! sends the write again. Every answer after the lookup carries the record,
//! of the dispensation category by construction and of the prescription
//! category for the lookup (Annex II 3.2).

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::response::Response;
use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{
    Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup, WrittenVersion,
};
use ferrofed_eehrxf::dispensations::{self, RESOURCE, Received};
use ferrofed_eehrxf::face;
use ferrofed_engine::single_node::forward::{ForwardError, Forwarded};
use ferrofed_engine::single_node::native_create::{Answered, Created};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::incident::Incident;
use ferrofed_registry::snapshot::RegistrySnapshot;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use http::{HeaderValue, StatusCode, header};
use secrecy::SecretString;

use super::lookup::Owned;
use super::{OPERATION, UNOWNED};
use crate::access::{Accessed, Asked, NoAccess};
use crate::config::fhir::FhirSettings;
use crate::facade::native::Chosen;
use crate::facade::prescriptions::Member;
use crate::facade::provenance::Provenance;
use crate::facade::security;
use crate::federation::Federation;
use crate::fhir::locator::{Locator, is_fhir_id};
use crate::metrics::dispensations::Outcome;

/// What the lookup reached: the patient it was named by, each member asked
/// and the members whose consent denial was set aside.
pub(super) struct Lookup<'a> {
    /// The identifier the patient was named by.
    pub(super) patient: (&'a str, &'a str),
    /// Each member asked, with what it answered.
    pub(super) members: &'a [Member],
    /// The members whose pre-filter denial was set aside.
    pub(super) set_aside: &'a BTreeSet<NodeId>,
}

/// How the write ended.
pub(super) enum Written {
    /// The request was never composed: the budget or the member's client
    /// could not be had.
    Unsent,
    /// The request was handed to the member's client, and this is how it
    /// ended.
    Sent(Created),
}

/// An `OperationOutcome` error with `status`, stating that no member was
/// asked.
pub(super) fn refused(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    let mut response = crate::fhir::fhir_json(status, &face::outcome(code, diagnostics));
    response.extensions_mut().insert(NoAccess);
    response
}

/// An `OperationOutcome` error with `status` and the issue type `code`,
/// after the lookup `lookup`, with its record; nothing was written.
pub(super) fn refused_after(
    federation: &Federation,
    lookup: &Lookup<'_>,
    (status, code): (StatusCode, &str),
    diagnostics: &str,
) -> Response {
    let mut response = crate::fhir::fhir_json(status, &face::outcome(code, diagnostics));
    attach(&mut response, accessed(federation, lookup, None));
    response
}

/// The answer to a lookup that found no one owner, `owned`.
pub(super) fn unowned(
    federation: &Federation,
    lookup: &Lookup<'_>,
    owned: Owned,
    logged: &str,
) -> Response {
    let listed = |endpoints: &[ferrofed_registry::id::EndpointId]| {
        endpoints
            .iter()
            .map(ferrofed_registry::id::EndpointId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match &owned {
        Owned::Claimed(_) => Outcome::Claimed,
        Owned::Several(_) => Outcome::Several,
        Owned::Unowned | Owned::At(_) => Outcome::Unowned,
        Owned::Foreign | Owned::Disagree | Owned::Spanning => Outcome::Unroutable,
    }
    .record();
    let (status, code, diagnostics) = match owned {
        Owned::Foreign => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            String::from(
                "an authorizingPrescription references no prescription of this face, {fhir-base}/MedicationRequest/{id}",
            ),
        ),
        Owned::Disagree => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            String::from(
                "the reference and the identifier of an authorizingPrescription do not name the same prescription, so nothing was written",
            ),
        ),
        Owned::Spanning => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            String::from(
                "the prescriptions the dispensation names are         if let Some(held) = origins
            .iter_mut()
            .find(|held| held.endpoint == origin.endpoint)
        {
            *held = origin;
        } else {
            ehrs.push(ehr_at(writing));
            origins.push(origin);
        }
 by different members, and a dispensation is written at one, so nothing was written (§2.3)",
            ),
        ),
        Owned::Claimed(claimants) => {
            Incident::PrescriptionIdentifierCollision {
                claimants: claimants.clone(),
            }
            .emit();
            (
                StatusCode::CONFLICT,
                "conflict",
                format!(
                    "the prescription the dispensation names is held by the member endpoints {}, and the gateway never chooses between them, so nothing was written",
                    listed(&claimants)
                ),
            )
        }
        Owned::Several(endpoint) => (
            StatusCode::CONFLICT,
            "conflict",
            format!(
                "{endpoint} holds more than one prescription the dispensation names, and the gateway never chooses between them, so nothing was written"
            ),
        ),
        Owned::Unowned | Owned::At(_) => {
            // NOTE: RFC 9110 §15.5.5, Federation Tier §11.3: no owner answers as an unknown or
            // restricted patient does, logged under the request id with no value.
            tracing::info!(
                request_id = logged,
                "a dispensation names a prescription no member holds"
            );
            (StatusCode::NOT_FOUND, "not-found", String::from(UNOWNED))
        }
    };
    refused_after(federation, lookup, (status, code), &diagnostics)
}

/// What a write is called in the answers to it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Deed {
    /// What was written, as a subject: "the dispensation".
    pub(super) object: &'static str,
    /// What the member did when it wrote it: "wrote the dispensation".
    pub(super) did: &'static str,
    /// How the write is said not to have been made: "written to".
    pub(super) done: &'static str,
}

/// The receipt of a dispensation, a create.
pub(super) const RECEIPT: Deed = Deed {
    object: "the dispensation",
    did: "wrote the dispensation",
    done: "written to",
};

/// How a write ended.
pub(super) enum Ended<'a> {
    /// The member made it, and this is its answer.
    Made(&'a Forwarded),
    /// The member made nothing.
    Unmade(Unmade<'a>),
}

/// How a write ended when the member made nothing.
pub(super) enum Unmade<'a> {
    /// The member refused or erred: the face's `status` and issue type
    /// `code`, followed by the member's own answer, which it `verb`.
    Relayed {
        /// The face's status.
        status: StatusCode,
        /// The face's issue type.
        code: &'static str,
        /// The member's answer.
        answered: &'a Answered,
        /// What the member did with the write.
        verb: &'static str,
    },
    /// Any other end, with the face's one issue.
    Plain(StatusCode, &'static str, String),
}

/// How the write `deed` to `writing` that ended as `created` is answered.
pub(super) fn ended<'a>(
    (writing, created): (&Chosen, &'a Written),
    deed: Deed,
    logged: &str,
) -> Ended<'a> {
    let endpoint = &writing.native;
    let member = &writing.node;
    let Deed { object, did, done } = deed;
    Ended::Unmade(match created {
        Written::Sent(Created::Made(made)) => return Ended::Made(made),
        Written::Sent(Created::Refused(answered)) => Unmade::Relayed {
            status: StatusCode::CONFLICT,
            code: "conflict",
            answered,
            verb: "refused",
        },
        Written::Sent(Created::Erred(answered)) => Unmade::Relayed {
            status: StatusCode::BAD_GATEWAY,
            code: "exception",
            answered,
            verb: "answered",
        },
        Written::Sent(Created::Abandoned | Created::Failed(ForwardError::TimeOut { .. })) => {
            Unmade::Plain(
                StatusCode::GATEWAY_TIMEOUT,
                "timeout",
                format!(
                    "member {member} did not answer at endpoint {endpoint} before the deadline: whether it {did} is unknown, and the gateway did not send it again"
                ),
            )
        }
        Written::Sent(Created::Failed(ForwardError::Unreachable { .. })) => Unmade::Plain(
            StatusCode::BAD_GATEWAY,
            "exception",
            format!(
                "member {member} could not be reached at endpoint {endpoint}: whether it {did} is unknown, and the gateway did not send it again"
            ),
        ),
        Written::Sent(Created::Failed(ForwardError::Withheld { endpoint, part })) => {
            security::gate_stopped(endpoint, part, logged);
            Unmade::Plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "security",
                String::from(
                    "the write would have carried a patient identifier outside its body, so it was not sent (§5.4.1, N33)",
                ),
            )
        }
        Written::Sent(Created::Failed(ForwardError::Capped(_) | ForwardError::Expired(_)))
        | Written::Unsent => Unmade::Plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            format!(
                "{object} could not be sent to member {member} before the deadline, so nothing was sent"
            ),
        ),
        Written::Sent(Created::Failed(failure)) => {
            tracing::warn!(error = %crate::chain(failure), request_id = logged, "a dispensation write was not made");
            Unmade::Plain(
                StatusCode::BAD_GATEWAY,
                "exception",
                format!("{object} could not be {done} member {member} at endpoint {endpoint}"),
            )
        }
    })
}

/// The status and the `OperationOutcome` of the write `deed` to `writing`
/// that ended as `unmade`.
pub(super) fn unmade_outcome(
    writing: &Chosen,
    deed: Deed,
    unmade: &Unmade<'_>,
) -> (StatusCode, OperationOutcome) {
    match unmade {
        Unmade::Relayed {
            status,
            code,
            answered,
            verb,
        } => (*status, relayed(writing, deed, (code, answered, verb))),
        Unmade::Plain(status, code, diagnostics) => (*status, face::outcome(code, diagnostics)),
    }
}

/// The answer to the write to `writing` that ended as `created`, of the
/// dispensation `received` after `lookup`, on the face `fhir`.
pub(super) fn written(
    federation: &Federation,
    (received, lookup): (&Received, &Lookup<'_>),
    (writing, created): (&Chosen, &Written),
    fhir: &FhirSettings,
    logged: &str,
) -> Response {
    match created {
        Written::Sent(Created::Made(_)) => Outcome::Created,
        Written::Sent(Created::Refused(_)) => Outcome::Refused,
        Written::Sent(
            Created::Abandoned
            | Created::Failed(ForwardError::TimeOut { .. } | ForwardError::Unreachable { .. }),
        ) => Outcome::Unknown,
        Written::Sent(_) | Written::Unsent => Outcome::Failed,
    }
    .record();
    let unmade = match ended((writing, created), RECEIPT, logged) {
        Ended::Made(made) => {
            return made_answer(
                federation,
                (received, lookup),
                (writing, made, created),
                fhir,
            );
        }
        Ended::Unmade(unmade) => unmade,
    };
    let (status, outcome) = unmade_outcome(writing, RECEIPT, &unmade);
    let mut response = crate::fhir::fhir_json(status, &outcome);
    attach(
        &mut response,
        accessed(federation, lookup, Some((writing, created, None))),
    );
    response
}

/// The outcome of a member's refusal or error `answered` of the write
/// `deed` to `writing`: the gateway's issue of `code`, naming the member
/// and its status, and after it the member's own issues when its body is a
/// FHIR `OperationOutcome`.
// NOTE: MyHealth@EU Requirements Catalogue v10.0.0 §1.25: the national answer travels with the
// business exception, so the member's issues follow the gateway's, unaltered.
fn relayed(
    writing: &Chosen,
    deed: Deed,
    (code, answered, verb): (&str, &Answered, &str),
) -> OperationOutcome {
    let (member, endpoint) = (&writing.node, &writing.native);
    let own = member_issues(answered.body());
    let follows = if own.is_empty() {
        ""
    } else {
        "; the member's own issues follow"
    };
    let diagnostics = format!(
        "member {member} {verb} {} at endpoint {endpoint} with {}{follows}",
        deed.object,
        answered.status().as_u16()
    );
    let mut outcome = face::outcome(code, &diagnostics);
    outcome.issue.extend(own);
    outcome
}

/// What a member's answer body is read for: whether it is an
/// `OperationOutcome`, and its issues.
#[derive(serde::Deserialize)]
struct MemberOutcome {
    /// The resource type the body names.
    #[serde(rename = "resourceType")]
    resource_type: String,
    /// The issues, when it is an `OperationOutcome`.
    #[serde(default)]
    issue: Vec<OperationOutcomeIssue>,
}

/// The issues of `body` when it is a FHIR R4 `OperationOutcome`, and none
/// otherwise.
fn member_issues(body: &[u8]) -> Vec<OperationOutcomeIssue> {
    // NOTE: a body that is no OperationOutcome is legitimately absent here: the member's status
    // alone already carries the verdict, so an unreadable body is left out, never an error.
    match serde_json::from_slice::<MemberOutcome>(body) {
        Ok(read) if read.resource_type == "OperationOutcome" => read.issue,
        Ok(_) | Err(_) => Vec::new(),
    }
}

/// The `201 Created` of the dispensation `made` at `writing`.
fn made_answer(
    federation: &Federation,
    (received, lookup): (&Received, &Lookup<'_>),
    (writing, made, created): (&Chosen, &Forwarded, &Written),
    fhir: &FhirSettings,
) -> Response {
    let (own, version) = created_id(made);
    let located = own
        .as_deref()
        .and_then(|own| Locator::new(writing.native.clone(), writing.ehr_id.clone(), own).id());
    // NOTE: FHIR R4 `http.html` §3.1.0.8 asks a Location of the created resource; a member that
    // names no id the face can locate leaves none, and the outcome says so.
    let unlocated = located.is_none().then_some(
        "the member named no id the face can locate the dispensation by, so the answer carries no Location",
    );
    let outcome = dispensations::written(
        received,
        (writing.node.as_str(), writing.native.as_str()),
        unlocated,
    );
    let mut response = crate::fhir::fhir_json(StatusCode::CREATED, &outcome);
    if let Some(id) = &located {
        let base = fhir.absolute.trim_end_matches('/');
        // NOTE: FHIR R4 `http.html` §3.1.0.3: a vid has the form of an `id`, and only that form
        // is one the face's vread takes, so any other version leaves the Location unversioned.
        let location = match version.as_deref().filter(|version| is_fhir_id(version)) {
            Some(version) => format!("{base}/{RESOURCE}/{id}/_history/{version}"),
            None => format!("{base}/{RESOURCE}/{id}"),
        };
        if let Ok(value) = HeaderValue::from_str(&location) {
            response.headers_mut().insert(header::LOCATION, value);
        }
    }
    let mut response = from_member(federation, writing, made, response);
    attach(
        &mut response,
        accessed(federation, lookup, Some((writing, created, own.as_deref()))),
    );
    response
}

/// `response` to a write the member `writing` made with the answer `made`,
/// carrying the member's `ETag` and `Last-Modified` (FHIR R4 `http.html`
/// §3.1.0.8, §3.1.0.4) and naming the member.
pub(super) fn from_member(
    federation: &Federation,
    writing: &Chosen,
    made: &Forwarded,
    mut response: Response,
) -> Response {
    for name in [header::ETAG, header::LAST_MODIFIED] {
        if let Some(value) = made.headers().get(&name) {
            response.headers_mut().insert(name, value.clone());
        }
    }
    Provenance::member(federation.snapshot(), &writing.native, &writing.node).stamp(response)
}

/// What a created resource's body is read for: its id.
#[derive(serde::Deserialize)]
struct Head {
    /// The resource's logical id.
    id: Option<String>,
}

/// The member's own id of the resource `made` created, and its version,
/// read from `Location` (FHIR R4 `http.html` §3.1.0.8,
/// `[base]/[type]/[id]/_history/[vid]`) or else from the body's `id`.
fn created_id(made: &Forwarded) -> (Option<String>, Option<String>) {
    let location = made
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok());
    if let Some(location) = location {
        let path = location.split(['?', '#']).next().unwrap_or_default();
        let marker = format!("{RESOURCE}/");
        if let Some(at) = path.rfind(marker.as_str()) {
            let rest = path.get(at + marker.len()..).unwrap_or_default();
            let mut parts = rest.split('/');
            let id = parts.next().filter(|id| !id.is_empty()).map(str::to_owned);
            let version = match (parts.next(), parts.next()) {
                (Some("_history"), Some(version)) if !version.is_empty() => {
                    Some(version.to_owned())
                }
                _ => None,
            };
            if id.is_some() {
                return (id, version);
            }
        }
    }
    let id = serde_json::from_slice::<Head>(made.body())
        .ok()
        .and_then(|head| head.id);
    (id, None)
}

/// Attaches `accessed` to `response`, or the statement that no member was
/// asked.
pub(super) fn attach(response: &mut Response, accessed: Option<Accessed>) {
    match accessed {
        Some(accessed) => {
            response.extensions_mut().insert(accessed);
        }
        None => {
            response.extensions_mut().insert(NoAccess);
        }
    }
}

/// The access record's facts of a dispensation whose lookup reached
/// `lookup` and whose write, when one was made, went to the member of
/// `write` and ended as its [`Written`] says, the member's own id of the
/// created resource with it; `None` when no request left the gateway or the
/// federation keeps no access log (Annex II 3.2).
fn accessed(
    federation: &Federation,
    lookup: &Lookup<'_>,
    write: Option<(&Chosen, &Written, Option<&str>)>,
) -> Option<Accessed> {
    let log = federation.access_log()?;
    let snapshot = federation.snapshot();
    let system_id = |node: &NodeId| {
        snapshot
            .node(node)
            .map(|node| node.system_id().as_str().to_owned())
    };
    let made = matches!(write, Some((_, Written::Sent(Created::Made(_)), _)));
    let evidence = |basis| {
        Evidence::reached(basis, Vec::new())
            .constructed(Category::ElectronicDispensation)
            .constructed(Category::ElectronicPrescription)
    };
    let (mut origins, mut ehrs) = reached(snapshot, lookup);
    let mut resource = None;
    if let Some((writing, Written::Sent(created), own)) = write
        && created.contact().left()
    {
        let origin = Asked {
            endpoint: writing.native.as_str().to_owned(),
            node: Some(writing.node.as_str().to_owned()),
            system_id: system_id(&writing.node),
            status: created.status().as_str().to_owned(),
            rows: Some(usize::from(made)),
            version: made.then_some(WrittenVersion::Unnamed),
            created_ehr: None,
            contributed: made,
            evidence: made.then(|| evidence(Basis::Written)),
        };
        if let Some(held) = origins
            .iter_mut()
            .find(|held| held.endpoint == origin.endpoint)
        {
            *held = origin;
        } else {
            ehrs.push(ehr_at(writing));
            origins.push(origin);
        }
        resource = own.map(|own| format!("{RESOURCE}/{own}"));
    }
    if origins.is_empty() && lookup.set_aside.is_empty() {
        return None;
    }
    let (system, value) = lookup.patient;
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Create,
        operation: OPERATION,
        resource,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: Some(PatientIdentifier {
                namespace: Some(system.to_owned()),
                value: SecretString::from(value.to_owned()),
            }),
            ehrs,
            population: None,
        },
        evidence: evidence(if made {
            Basis::Written
        } else {
            Basis::Returned
        }),
        delivered: Some(usize::from(made)),
        origins,
        consent_set_aside: lookup
            .set_aside
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect(),
        inserted: None,
        transformations: Vec::new(),
    })
}

/// The origin of every member `lookup` reached, of `snapshot`, with the
/// `ehr_id` it was asked by.
fn reached(snapshot: &RegistrySnapshot, lookup: &Lookup<'_>) -> (Vec<Asked>, Vec<EhrAt>) {
    lookup
        .members
        .iter()
        .filter(|member| member.held.left())
        .map(|member| {
            let origin = Asked {
                endpoint: member.chosen.native.as_str().to_owned(),
                node: Some(member.chosen.node.as_str().to_owned()),
                system_id: snapshot
                    .node(&member.chosen.node)
                    .map(|node| node.system_id().as_str().to_owned()),
                status: member.held.status().as_str().to_owned(),
                rows: Some(member.held.prescriptions()),
                version: None,
                created_ehr: None,
                contributed: false,
                evidence: None,
            };
            (origin, ehr_at(&member.chosen))
        })
        .unzip()
}

/// The `ehr_id` `chosen` was asked by, at its own endpoint.
fn ehr_at(chosen: &Chosen) -> EhrAt {
    EhrAt {
        endpoint: chosen.native.as_str().to_owned(),
        ehr_id: chosen.ehr_id.as_str().to_owned(),
        patient: PatientLookup::RequestNamed,
    }
}
