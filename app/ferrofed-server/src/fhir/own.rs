// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory, discharge and imaging documents of the document list a
//! member serves itself (Regulation (EU) 2025/327 Art 14(1)(d) to (f), Annex II
//! 2.1).
//!
//! A member that serves the documents of a category itself has each of its
//! own `DocumentReference`s listed as it holds them, its attachment URLs
//! readdressed to `Binary/{token}` URLs the gateway signs, since the
//! member's own URL is one the caller cannot reach and one that would pass
//! the gateway's access check and access record by. The read asks that
//! member alone for its `Binary`, by its `ehr_id` alone (§5.4.1, N33), and
//! answers it as the member sent it. Readdressing the URLs is our own
//! design: no specification governs it.

use std::sync::Arc;

use axum::response::Response;
use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientLookup};
use ferrofed_eehrxf::list::{self, Match};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_search::{By, SearchAsked, Searched};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use super::admission::Admitted;
use super::written::Listed;
use super::{attach, outcome};
use crate::access::{Accessed, Asked};
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions;
use crate::facade::request::Arrived;
use crate::facade::summary::Gathered;
use crate::facade::summary::own::{Kind, binary};
use crate::federation::Federation;

/// The operation an access record names for the read.
const OPERATION: &str = "Binary";

/// The entries the list names for the documents the members of `gathered`
/// that serve them themselves answered with, on the face `fhir`, for
/// `caller`: each member's `DocumentReference` as it holds it, its
/// attachment URLs readdressed to URLs the gateway signs, each by its
/// `fullUrl`, and an `OperationOutcome` for each member that did not answer
/// or answered what the list cannot pass through; `None` when a URL cannot
/// be signed.
pub(super) fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
) -> Option<Listed> {
    let snapshot = federation.snapshot();
    let minted = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let mut matches = Vec::new();
    let mut outcomes = Vec::new();
    for own in &gathered.own {
        let endpoint = own.chosen.native.as_str();
        match &own.held {
            prescriptions::Held::Found(pages) => {
                let base = snapshot.native_endpoint(&own.chosen.native)?.url();
                let signer = federation.signer()?;
                for document in pages.iter().flat_map(|page| &page.prescriptions) {
                    // NOTE: no specification governs this: our own design; the attachment URL alone
                    // changes, so the document is read through the gateway's access check and record.
                    let passed = list::readdressed(&document.kept.resource, |url| {
                        let binary = binary(base, url)?;
                        let token = super::binary::mint_own(
                            signer,
                            fhir,
                            caller,
                            (&own.chosen, own.kind, &binary),
                        )
                        .ok()?;
                        Some(format!("Binary/{token}"))
                    })?;
                    matches.push((minted(), Match::Passed(passed)));
                }
            }
            prescriptions::Held::Unusable { reason, .. } => {
                outcomes.push((
                    minted(),
                    list::unread(endpoint, own.kind.documents(), &reason.to_string()),
                ));
            }
            prescriptions::Held::Silent(searched) => {
                outcomes.push((minted(), list::silent(endpoint, searched.status().as_str())));
            }
            prescriptions::Held::Unanswered { status, .. } => {
                outcomes.push((minted(), list::silent(endpoint, status.as_str())));
            }
        }
    }
    Some((matches, outcomes))
}

/// `GET {fhir-base}/Binary/{token}` of a document of the kind `kind` that
/// `member` serves itself: its `Binary` of the logical id `binary`, as the
/// member sent it.
pub(super) async fn read(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, kind, binary): (&Chosen, Kind, &str),
) -> Response {
    let federation = &admitted.federation;
    let snapshot = federation.snapshot();
    let (Some(client), Some(native)) = (
        federation.clients().get(&member.via),
        snapshot.native_endpoint(&member.native),
    ) else {
        return outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document at this URL; list the documents again",
        );
    };
    // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond what it
    // reads of every request, and the member is located by its ehr_id alone.
    let (options, until) = match native::options(federation, arrived, Arc::new(Withheld::default()))
    {
        Ok(options) => options,
        Err(clock) => {
            tracing::error!(
                error = %crate::chain(&clock),
                request_id = %admitted.outbound,
                category = kind.code(),
                "the budget of a member's document read could not be held"
            );
            return crate::error::fixed(crate::error::Code::Internal, admitted.request_id);
        }
    };
    let asked = SearchAsked {
        native,
        ehr_id: &member.ehr_id,
        resource: "Binary",
        by: By::Read(binary),
        next: None,
    };
    let searched = client.native_search(asked, &options, until).await;
    let left = searched.contact().left();
    let (mut response, status) = match &searched {
        Searched::Page(forwarded) => (super::binary::passed(forwarded), EndpointStatus::Active),
        // NOTE: FHIR R4 `http.html` §3.1.0.2: a read of a resource the member holds no longer is
        // `404` or `410`, an answer about the document that leaves the member active.
        Searched::Erred(code) if *code == StatusCode::NOT_FOUND || *code == StatusCode::GONE => (
            outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "the member holds this document no longer; list the documents again",
            ),
            EndpointStatus::Active,
        ),
        silent => (unanswered(&member.native, silent.status()), silent.status()),
    };
    let delivered = response.status() == StatusCode::OK;
    attach(
        &mut response,
        accessed(
            federation,
            (member, &member.native),
            (kind.category(), status.as_str(), left, delivered),
        ),
    );
    response
}

/// The answer to a read whose member, at `endpoint`, gave no answer, with
/// its §11.1 status `status`: `504` for one in `time-out` or `offline` and
/// `424` otherwise (§11.3, §11.4).
pub(super) fn unanswered(endpoint: &EndpointId, status: EndpointStatus) -> Response {
    let (answer, code) = match status {
        EndpointStatus::TimeOut | EndpointStatus::Offline => {
            (StatusCode::GATEWAY_TIMEOUT, "timeout")
        }
        _ => (StatusCode::FAILED_DEPENDENCY, "incomplete"),
    };
    outcome(
        answer,
        code,
        &format!(
            "the document could not be read: no answer from {endpoint} ({}) (§11.3)",
            status.as_str()
        ),
    )
}

/// The access record's facts of the read of a document of `category` at
/// `member`, asked at its `endpoint`, whose answer showed the §11.1 status
/// `status`, a request having left when `left`, the document delivered when
/// `delivered` (Annex II 3.2); `None` when nothing left or no access log is
/// kept.
pub(super) fn accessed(
    federation: &Federation,
    (member, endpoint): (&Chosen, &EndpointId),
    (category, status, left, delivered): (Category, &str, bool, bool),
) -> Option<Accessed> {
    let log = federation.access_log()?;
    if !left {
        return None;
    }
    let evidence = || Evidence::reached(Basis::Returned, Vec::new()).constructed(category.clone());
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Read,
        operation: OPERATION,
        resource: None,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: None,
            ehrs: vec![EhrAt {
                endpoint: endpoint.as_str().to_owned(),
                ehr_id: member.ehr_id.as_str().to_owned(),
                patient: PatientLookup::RequestNamed,
            }],
            population: None,
        },
        evidence: evidence(),
        delivered: Some(usize::from(delivered)),
        origins: vec![Asked {
            endpoint: endpoint.as_str().to_owned(),
            node: Some(member.node.as_str().to_owned()),
            system_id: federation
                .snapshot()
                .node(&member.node)
                .map(|node| node.system_id().as_str().to_owned()),
            status: status.to_owned(),
            rows: Some(usize::from(delivered)),
            version: None,
            created_ehr: None,
            contributed: delivered,
            evidence: delivered.then(evidence),
        }],
        consent_set_aside: std::collections::BTreeSet::new(),
        inserted: None,
        transformations: Vec::new(),
    })
}
