// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The withdrawal of a dispensation on the FHIR face.

use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::Path;
use axum::response::Response;
use eehrxf::category::Category;
use http::StatusCode;

use crate::facade::dispensation::withdrawal::{self, WITHDRAWN};
use crate::facade::prescription::{self, Readable, Unfound};
use crate::fhir::trc::{self, Service};

use crate::fhir::admission::Asked;
use crate::fhir::{admission, refusal};

use super::dispense::received;
use super::read::unread;

/// `PUT {fhir-base}/MedicationDispense/{id}`: the withdrawal of the
/// dispensation the id names, written as an update at the member that
/// holds it ([`crate::facade::dispensation::withdrawal`]).
///
/// The body is the dispensation in FHIR JSON with `status`
/// `entered-in-error`, held to the HL7 Europe MPD 1.0.0 profiles as a
/// receipt is. Its `id` must be the one the request names, or it is `400`
/// (FHIR R4 `http.html` §3.1.0.4), and any other `status` is `422`
/// `business-rule`, since the face takes an update of a dispensation only as
/// its withdrawal (§3.1.0.4.2). A withdrawal the member made answers `200`
/// with an `OperationOutcome` naming the member.
pub(in crate::fhir) async fn withdraw(
    asked: Asked,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let (received, text) = match received(&asked.state, &asked.headers, &body) {
        Ok(received) => received,
        Err(refused) => return refused,
    };
    // NOTE: FHIR R4 `http.html` §3.1.0.4: a body with no id, or one that disagrees with the
    // [id] in the URL, SHALL be answered 400.
    if received.id.as_deref() != Some(id.as_str()) {
        return refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the dispensation's id is not the id the request names (FHIR R4 http.html §3.1.0.4)",
        );
    }
    // NOTE: no specification governs a discard on the national side: our own design; the face
    // takes an update of a dispensation only as its withdrawal (FHIR R4 `http.html` §3.1.0.4.2).
    if received.status.as_deref() != Some(WITHDRAWN) {
        return refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            "business-rule",
            "the face takes an update of a dispensation only as its withdrawal, with status entered-in-error",
        );
    }
    admission::served(
        (&asked, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to withdraw a dispensation",
            )
        },
        |_| ControlFlow::Continue(()),
        async |admitted, arrived, ()| {
            if let Err(missing) = trc::binding(admitted, &arrived) {
                return missing;
            }
            let federation = &admitted.federation;
            let none = BTreeSet::new();
            let declared = admitted
                .fhir
                .serving
                .members_of(Category::Dispensation)
                .unwrap_or(&none);
            let dispensation = (Readable::Dispensation, id.as_str());
            match prescription::read(federation, &arrived, dispensation, (declared, None)).await {
                Err(Unfound::Unbound(member, member_ehr_id)) => trc::unbound(
                    admitted,
                    &arrived,
                    (&member, &member_ehr_id),
                    Service::Prescription,
                ),
                Ok(read) => {
                    withdrawal::withdraw(federation, &arrived, (&received, text), &read).await
                }
                Err(unfound) => unread(
                    federation,
                    Readable::Dispensation,
                    unfound,
                    (admitted.request_id, admitted.outbound),
                ),
            }
        },
    )
    .await
}
