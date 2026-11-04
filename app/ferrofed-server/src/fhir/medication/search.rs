// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search of a patient's prescriptions on the FHIR face.

use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::time::Instant;

use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::prescriptions::SearchRequest;
use ferrofed_eehrxf::prescriptions::page::{Page, Prescription};
use ferrofed_eehrxf::prescriptions::searchset::{Answered, Assembled, Silent, assemble};
use ferrofed_eehrxf::summary::Organisation;
use http::{StatusCode, Uri};
use openehr_federation::status::EndpointStatus;

use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::native::Chosen;
use crate::facade::prescriptions::{self, Held, Prescribed, Unprescribed};
use crate::facade::summary::organisation;
use crate::federation::Federation;

use crate::fhir::admission::Asked;
use crate::fhir::locator::Locator;
use crate::fhir::{admission, attach, fhir_json, refusal};

use super::{not_served, serves};
use crate::face_path::PRESCRIPTIONS;

/// `GET {fhir-base}/MedicationRequest?patient.identifier=system|value`: a
/// patient's prescriptions, at every member that serves them
/// ([`crate::facade::prescriptions`]).
///
/// A patient no member serving prescriptions holds, and one none may say
/// anything about, both answer the same empty `searchset` (FHIR R4 search
/// §3.1.1.3: an unknown subject is no failure; Federation Tier §11.3).
pub(in crate::fhir) async fn search(asked: Asked, uri: Uri) -> Response {
    let started = Instant::now();
    if !serves(&asked.state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    let read = match SearchRequest::from_query(uri.query()) {
        Ok(read) => read,
        Err(refused) => {
            return refusal(StatusCode::BAD_REQUEST, "invalid", &refused.to_string());
        }
    };
    admission::served(
        (&asked, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to search prescriptions",
            )
        },
        |_| ControlFlow::Continue(()),
        async |admitted, arrived, ()| {
        let (federation, fhir) = (&admitted.federation, admitted.fhir);
        let none = BTreeSet::new();
        let declared = fhir
            .serving
            .members_of(Category::Prescription)
            .unwrap_or(&none);
        let patient = (read.system(), read.value());
        let now = jiff::Timestamp::now()
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let searched = prescriptions::search(
            federation,
            &arrived,
            (patient, &now),
            (declared, &fhir.mapped),
        )
        .await;
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
                let mut response = crate::fhir::patient_summary::incomplete("search", status, &silent);
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
        },
    )
    .await
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
                named(fhir, member, pages),
            )),
            Held::Unusable { .. } | Held::Silent(_) | Held::Unanswered { .. } => None,
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

/// The pages `pages` of the member `member`, each prescription named by the
/// id the face reads it by (`fhir::locator`), on the face `fhir`.
///
/// A prescription named so has that id and a `fullUrl` of the face's read
/// (FHIR R4 `http.html` §3.1.0.2), and is otherwise as the member sent it
/// or, in the map mode, as the face mapped it; one whose parts give no such
/// id keeps its own id and `fullUrl` and is marked unreadable, which the
/// searchset warns of. A resource a mapped prescription references is named
/// under the face's base by its type and id, so the reference resolves in the
/// searchset (FHIR R4 Bundle §Resolving references in Bundles).
fn named(fhir: &FhirSettings, member: &prescriptions::Member, pages: &[Page]) -> Vec<Page> {
    let base = fhir.absolute.trim_end_matches('/');
    let chosen = &member.chosen;
    let mut pages = pages.to_vec();
    for (index, page) in pages.iter_mut().enumerate() {
        let origin = member.composed.as_ref().map_or(Origin::Native, |composed| {
            Origin::Mapped(composed.get(index).and_then(|held| held.object))
        });
        if origin != Origin::Native {
            for kept in &mut page.included {
                if let Some(id) = kept.id() {
                    kept.full_url = Some(format!("{base}/{}/{id}", kept.resource_type));
                }
            }
        }
        for (position, prescription) in page.prescriptions.iter_mut().enumerate() {
            name(base, chosen, (origin, position), prescription);
        }
    }
    pages
}

/// Where a page of a member's prescriptions comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// The member's own endpoint.
    Native,
    /// The mapping of one of its compositions, by the UUID of its versioned
    /// object when it is one.
    Mapped(Option<uuid::Uuid>),
}

/// Names `prescription`, at `position` on a page of the member `chosen`
/// that comes from `origin`, by the id the face reads it by, under the face's
/// base `base`.
fn name(
    base: &str,
    chosen: &Chosen,
    (origin, position): (Origin, usize),
    prescription: &mut Prescription,
) {
    let kept = &mut prescription.kept;
    let id = match origin {
        Origin::Native => kept.id().and_then(|resource| {
            Locator::new(chosen.native.clone(), chosen.ehr_id.clone(), resource).id()
        }),
        Origin::Mapped(composition) => {
            composition
                .zip(u8::try_from(position).ok())
                .and_then(|(composition, entry)| {
                    Locator::mapped(
                        chosen.via.clone(),
                        chosen.ehr_id.clone(),
                        composition,
                        entry,
                    )
                    .id()
                })
        }
    };
    let Some(id) = id else {
        // NOTE: no specification governs this: our own design; a prescription whose parts
        // give no id is listed as its member states it, and the searchset warns it cannot be read.
        tracing::warn!(
            endpoint = chosen.native.as_str(),
            "a member's prescription has no id the face can read it by"
        );
        prescription.unreadable = true;
        return;
    };
    kept.full_url = Some(format!("{base}{PRESCRIPTIONS}/{id}"));
    kept.rename(&id);
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
