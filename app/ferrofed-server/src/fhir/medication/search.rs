// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search of a patient's prescriptions on the FHIR face.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::State;
use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::prescriptions::SearchRequest;
use ferrofed_eehrxf::prescriptions::page::Page;
use ferrofed_eehrxf::prescriptions::searchset::{Answered, Assembled, Silent, assemble};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_engine::outbound_id::OutboundId;
use http::{StatusCode, Uri};
use openehr_federation::status::EndpointStatus;

use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::native::Chosen;
use crate::facade::prescriptions::{self, Held, Prescribed, Unprescribed};
use crate::facade::summary::organisation;
use crate::federation::Federation;
use crate::state::AppState;

use crate::fhir::locator::Locator;
use crate::fhir::{admission, attach, fhir_json, outcome, refusal};

use super::{PRESCRIPTIONS, not_served, serves};

/// `GET {fhir-base}/MedicationRequest?patient.identifier=system|value`: a
/// patient's prescriptions, at every member that serves them
/// ([`crate::facade::prescriptions`]).
///
/// A patient no member serving prescriptions holds, and one none may say
/// anything about, both answer the same empty `searchset` (FHIR R4 search
/// §3.1.1.3: an unknown subject is no failure; Federation Tier §11.3).
pub(in crate::fhir) async fn search(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    let read = match SearchRequest::from_query(uri.query()) {
        Ok(read) => read,
        Err(refused) => {
            return refusal(StatusCode::BAD_REQUEST, "invalid", &refused.to_string());
        }
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to search prescriptions",
            )
        },
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let none = BTreeSet::new();
    let declared = fhir
        .serving
        .members_of(Category::Prescription)
        .unwrap_or(&none);
    let patient = (read.system(), read.value());
    let searched = prescriptions::search(federation, &arrived, patient, declared).await;
    let (request_id, outbound) = (admitted.request_id, admitted.outbound);
    match searched {
        Ok(prescribed) => listed(federation, fhir, (&read, &prescribed)),
        Err(Unprescribed::NotFound(prescribed)) => {
            let mut response = listed_none(fhir, &read);
            let accessed = prescribed.and_then(|prescribed| {
                prescriptions::accessed(federation, &prescribed, patient, None)
            });
            attach(&mut response, accessed);
            response
        }
        Err(Unprescribed::Incomplete {
            status,
            silent,
            prescribed,
        }) => {
            let named = silent
                .iter()
                .map(|(endpoint, status)| format!("{endpoint} ({status})"))
                .collect::<Vec<_>>()
                .join(", ");
            let code = if status == StatusCode::GATEWAY_TIMEOUT {
                "timeout"
            } else {
                "incomplete"
            };
            let diagnostics = if named.is_empty() {
                "the search is incomplete: the patient could not be resolved at every member (§11.3)"
                    .to_owned()
            } else {
                format!("the search is incomplete: no answer from {named} (§11.3)")
            };
            let mut response = outcome(status, code, &diagnostics);
            attach(
                &mut response,
                prescriptions::accessed(federation, &prescribed, patient, None),
            );
            response
        }
        Err(Unprescribed::Failure(failure)) => failure.respond(request_id, outbound),
        Err(Unprescribed::Internal(what)) => {
            tracing::error!(request_id = %outbound, what, "the prescription search could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
    }
}

/// The page of the `searchset` `read` asks for of what `prescribed` holds,
/// on the face `fhir`, with the access it records.
fn listed(
    federation: &Federation,
    fhir: &FhirSettings,
    (read, prescribed): (&SearchRequest, &Prescribed),
) -> Response {
    let snapshot = federation.snapshot();
    let found: Vec<(&prescriptions::Member, Organisation, Vec<Page>)> = prescribed
        .members
        .iter()
        .filter_map(|member| match &member.held {
            Held::Found(pages) => Some((
                member,
                organisation(snapshot, &member.chosen.node),
                named(fhir, &member.chosen, pages),
            )),
            Held::Unusable { .. } | Held::Silent(_) => None,
        })
        .collect();
    let answered: Vec<Answered<'_>> = found
        .iter()
        .map(|(member, organisation, pages)| Answered {
            endpoint: member.chosen.native.as_str(),
            organisation,
            pages,
        })
        .collect();
    let silent: Vec<Silent> = prescribed
        .members
        .iter()
        .filter(|member| !matches!(member.held, Held::Found(_)))
        .map(|member| Silent {
            endpoint: member.chosen.native.as_str().to_owned(),
            status: member.held.status().as_str().to_owned(),
        })
        .chain(prescribed.unresolved.iter().map(|endpoint| Silent {
            endpoint: endpoint.as_str().to_owned(),
            status: EndpointStatus::NotResolved.as_str().to_owned(),
        }))
        .collect();
    let assembled = written(fhir, read, (&answered, &silent));
    let mut response = fhir_json(StatusCode::OK, &assembled.searchset);
    let accessed = prescriptions::accessed(
        federation,
        prescribed,
        (read.system(), read.value()),
        Some(&assembled),
    );
    attach(&mut response, accessed);
    response
}

/// The pages `pages` of the member `chosen`, each prescription named by the
/// id the face reads it by ([`locator`]), on the face `fhir`.
///
/// A prescription named so has that id and a `fullUrl` of the face's read
/// (FHIR R4 `http.html` §3.1.0.2), and is otherwise as the member sent it;
/// one whose parts give no such id keeps the member's own id and `fullUrl`
/// and is marked unreadable, which the searchset warns of.
fn named(fhir: &FhirSettings, chosen: &Chosen, pages: &[Page]) -> Vec<Page> {
    let base = fhir.absolute.trim_end_matches('/');
    let mut pages = pages.to_vec();
    for prescription in pages
        .iter_mut()
        .flat_map(|page| page.prescriptions.iter_mut())
    {
        let kept = &mut prescription.kept;
        let id = kept.id().and_then(|resource| {
            Locator::new(chosen.native.clone(), chosen.ehr_id.clone(), resource).id()
        });
        let Some(id) = id else {
            // NOTE: no specification governs this: our own design; a prescription whose parts
            // give no id is listed as its member states it, and the searchset warns it cannot be read.
            tracing::warn!(
                endpoint = chosen.native.as_str(),
                "a member's prescription has no id the face can read it by"
            );
            prescription.unreadable = true;
            continue;
        };
        kept.full_url = Some(format!("{base}{PRESCRIPTIONS}/{id}"));
        kept.rename(&id);
    }
    pages
}

/// The empty `searchset` `read` is answered with when no member serving
/// prescriptions holds the patient, or none may say so.
fn listed_none(fhir: &FhirSettings, read: &SearchRequest) -> Response {
    fhir_json(StatusCode::OK, &written(fhir, read, (&[], &[])).searchset)
}

/// The page `read` asks for of what `answered` holds, naming every member
/// of `silent`, on the face `fhir`.
fn written(
    fhir: &FhirSettings,
    read: &SearchRequest,
    (answered, silent): (&[Answered<'_>], &[Silent]),
) -> Assembled {
    let recorded = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let mut mint = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    assemble(
        (read, fhir.absolute.trim_end_matches('/')),
        (answered, silent),
        &recorded,
        &mut mint,
    )
}
