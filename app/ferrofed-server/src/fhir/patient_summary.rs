// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer to a patient summary request: the summary gathered, the header
//! the identity binding holds, the document written, and the refusal or the
//! incomplete answer when it cannot be (§11.3, §11.4).

use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use ferrofed_eehrxf::face::{Refusal, SummaryRequest};
use ferrofed_eehrxf::summary::{self, Author};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use http::StatusCode;
use secrecy::SecretString;

use super::{PRODUCT, UNADMITTED, admission, attach, outcome};
use crate::access::NoAccess;
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::demographics::{self, Unheaded};
use crate::facade::summary::laboratory::Laboratory;
use crate::facade::summary::{self as gathered, Scope, Unsummarised};
use crate::fhir::admission::Asked;
use crate::fhir::trc::Service;
use crate::fhir_wire::fhir_json;

/// The answer to a summary request `read` from the request's caller and
/// headers.
pub(super) async fn summarised(
    (asked, started): (&Asked, Instant),
    read: Result<SummaryRequest, Refusal>,
) -> Response {
    let read = match read {
        Ok(read) => read,
        Err(refusal) => {
            return outcome(StatusCode::BAD_REQUEST, "invalid", &refusal.to_string());
        }
    };
    admission::served(
        (asked, started),
        || outcome(StatusCode::FORBIDDEN, "forbidden", UNADMITTED),
        |_| ControlFlow::Continue(()),
        async |admitted, arrived, ()| summary_of(admitted, arrived, &read).await,
    )
    .await
}

/// The summary `read` asks for, of the request `admitted` and `arrived`
/// carry.
pub(super) async fn summary_of(
    admitted: &admission::Admitted<'_>,
    arrived: crate::facade::request::Arrived<'_>,
    read: &SummaryRequest,
) -> Response {
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let (request_id, outbound, logged) = (admitted.request_id, admitted.outbound, &admitted.logged);
    let patient = (read.system(), read.value());
    // NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: a contact point's patient is its TRC's, compared
    // before the demographics supplier or any member is asked.
    if let Err(refused) = super::trc::named(admitted, &arrived, patient, Service::PatientSummary) {
        return refused;
    }
    // NOTE: eHN PS A.1.1, A.1.2; the header comes from the identity binding before any
    // member is asked, so no member is reached for a summary that could name no patient.
    let header = header_of(federation, &arrived, patient, (request_id, logged.as_str())).await;
    let header = match header {
        Ok(header) => header,
        Err(headless) => {
            let mut response = match headless {
                Headless::Refused(refused) => *refused,
                Headless::Unheaded(unheaded) => unheaded_outcome(unheaded, "summary header"),
            };
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    match gathered::gather(
        Arc::clone(federation),
        arrived,
        patient,
        (Scope::Summary, Laboratory::Unlisted),
    )
    .await
    {
        Ok(gathered) => document(federation, fhir, &gathered, (patient, header), logged).await,
        Err(unsummarised) => refused(
            federation,
            (unsummarised, Scope::Summary),
            patient,
            (request_id, outbound),
        ),
    }
}

/// The header the identity binding holds of the patient `system`|`value`
/// for a request that `arrived`, asked once the caller's grant is held to the
/// patient, or the [`Headless`] that says why there is none.
pub(super) async fn header_of(
    federation: &crate::federation::Federation,
    arrived: &crate::facade::request::Arrived<'_>,
    patient: (&str, &str),
    (request_id, logged): (&str, &str),
) -> Result<PatientHeader, Headless> {
    let named = named(patient).map_err(Headless::Unheaded)?;
    // NOTE: Regulation (EU) 2025/327 Art 3(1), Federation Tier §5.2: a grant confined to one
    // patient is held to it before the demographics supplier is asked about anyone.
    let conveyed = (arrived.conveyance, arrived.on_behalf);
    let started = arrived.started;
    if let Some(refused) = crate::facade::other_patient(
        federation,
        conveyed,
        (&named, started),
        (request_id, logged),
    )
    .await
    {
        return Err(Headless::Refused(Box::new(refused)));
    }
    headed(federation, (&named, arrived.on_behalf), started)
        .await
        .map_err(Headless::Unheaded)
}

/// Why a request gets no header, so nothing is sent to a member.
pub(super) enum Headless {
    /// The caller is confined to another patient, or its patient could not
    /// be checked: the answer.
    Refused(Box<Response>),
    /// The identity binding gives no header.
    Unheaded(Unheaded),
}

/// The patient reference of the identifier `system`|`value` a summary
/// request names.
pub(super) fn named((system, value): (&str, &str)) -> Result<PatientRef, Unheaded> {
    // NOTE: the request reader refuses an empty system or value, so a reference that cannot
    // be built names no identifier the binding is asked about.
    IdentifierNamespace::new(system)
        .and_then(|namespace| PatientRef::new(namespace, SecretString::from(value)))
        .map_err(|_empty| Unheaded::NoBinding)
}

/// The summary header the identity binding holds of `patient`, asked on
/// behalf of `on_behalf` within the budget of a request that started at
/// `started`, or the [`Unheaded`] that says why there is none.
pub(super) async fn headed(
    federation: &crate::federation::Federation,
    (patient, on_behalf): (&PatientRef, &OnBehalfOf),
    started: Instant,
) -> Result<PatientHeader, Unheaded> {
    let deadline = started
        .checked_add(federation.budget().overall())
        .unwrap_or(started);
    demographics::header(federation, (patient, on_behalf), deadline).await
}

/// The `OperationOutcome` of a request for `what` whose header `unheaded`
/// says why the identity binding gives none.
pub(super) fn unheaded_outcome(unheaded: Unheaded, what: &str) -> Response {
    match unheaded {
        Unheaded::NoBinding => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-supported",
            &format!(
                "the demographics binding is not asked about identifiers of this system, so no {what} can be written"
            ),
        ),
        // NOTE: Federation Tier §11.3; a patient the binding does not know answers as one
        // no member holds, so the outcome names neither.
        Unheaded::NoMatch => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no member holds a patient summary for this identifier",
        ),
        Unheaded::Ambiguous(reason) => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "multiple-matches",
            &format!("no {what} can be written: {reason}"),
        ),
        Unheaded::Unnamed(_) => outcome(
            StatusCode::UNPROCESSABLE_ENTITY,
            "required",
            "the demographics binding holds no name for this patient, which the HL7 Europe Patient Summary Patient requires (ips-pat-1)",
        ),
        Unheaded::Unavailable { failure, timed_out } => {
            let (status, code) = if timed_out {
                (StatusCode::GATEWAY_TIMEOUT, "timeout")
            } else {
                (StatusCode::BAD_GATEWAY, "exception")
            };
            outcome(status, code, &format!("no {what}: {failure}"))
        }
    }
}

/// The document `gathered` makes about the patient the identity binding
/// describes in `header`, answered with the access it records.
pub(super) async fn document(
    federation: &crate::federation::Federation,
    fhir: &FhirSettings,
    gathered: &gathered::Gathered,
    ((system, value), header): ((&str, &str), PatientHeader),
    logged: &str,
) -> Response {
    let request = summary::Request {
        base: fhir.absolute.clone(),
        identifier: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        patient_id: uuid::Uuid::new_v4().to_string(),
        timestamp: jiff::Timestamp::now()
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        system: system.to_owned(),
        value: value.to_owned(),
        header,
    };
    let author = Author {
        product: String::from(PRODUCT),
        version: String::from(env!("CARGO_PKG_VERSION")),
        operator: fhir.operator.clone(),
    };
    let elsewhere = gathered::elsewhere(federation.snapshot(), gathered);
    let assembled = summary::assemble(
        &request,
        &author,
        (&gathered.origins, &gathered.sections, &elsewhere),
        (&fhir.mappings, &fhir.terminology),
    )
    .await;
    let (mut response, delivered, transformations) = match assembled {
        Ok(assembled) => (
            fhir_json(StatusCode::OK, &assembled.bundle),
            assembled.mapped,
            assembled
                .transformations
                .iter()
                .map(crate::access::transformation::of)
                .collect(),
        ),
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the patient summary could not be written"
            );
            (
                outcome(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "exception",
                    "the patient summary could not be written from what the members answered",
                ),
                BTreeSet::new(),
                Vec::new(),
            )
        }
    };
    let accessed = gathered::access::accessed(
        federation,
        gathered,
        (system, value),
        (Scope::Summary, Some(&delivered)),
    )
    .map(|mut accessed| {
        accessed.transformations = transformations;
        accessed
    });
    attach(&mut response, accessed);
    response
}

/// The answer to a request of `scope` the members did not give,
/// `unsummarised`.
pub(super) fn refused(
    federation: &crate::federation::Federation,
    (unsummarised, scope): (Unsummarised, Scope),
    patient: (&str, &str),
    (request_id, outbound): (&str, OutboundId),
) -> Response {
    // NOTE: no specification governs this: our own design; a refused summary counts what
    // it delivered, none, and a refused list delivered no member's own document.
    let none = BTreeSet::new();
    let delivered = (scope == Scope::Summary).then_some(&none);
    let what = match scope {
        Scope::Summary => "summary",
        Scope::List => "list",
        Scope::Lookup => "lookup",
    };
    match unsummarised {
        Unsummarised::Failure(failure) => failure.respond(request_id, outbound),
        Unsummarised::Internal(what) => {
            tracing::error!(request_id = %outbound, what, "the section queries could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
        // NOTE: Federation Tier §11.3, Regulation (EU) 2025/327 Art 8: an unknown patient and
        // one no member may disclose answer alike, so the outcome names neither.
        Unsummarised::NotFound(gathered) => {
            let mut response = outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "no member holds a patient summary for this identifier",
            );
            let accessed = gathered.and_then(|gathered| {
                gathered::access::accessed(federation, &gathered, patient, (scope, delivered))
            });
            attach(&mut response, accessed);
            response
        }
        Unsummarised::Incomplete {
            status,
            silent,
            gathered,
        } => {
            let mut response = incomplete(what, status, &silent);
            attach(
                &mut response,
                gathered::access::accessed(federation, &gathered, patient, (scope, delivered)),
            );
            response
        }
    }
}

/// The `OperationOutcome` of a `what` that settled incomplete with
/// `status`, naming each member in `silent` that gave no answer with its
/// status (§11.3).
pub(super) fn incomplete(what: &str, status: StatusCode, silent: &[(String, String)]) -> Response {
    let named = gathered::named(silent);
    let code = if status == StatusCode::GATEWAY_TIMEOUT {
        "timeout"
    } else {
        "incomplete"
    };
    let diagnostics = if named.is_empty() {
        format!(
            "the {what} is incomplete: the patient could not be resolved at every member (§11.3)",
        )
    } else {
        format!("the {what} is incomplete: no answer from {named} (§11.3)")
    };
    outcome(status, code, &diagnostics)
}
