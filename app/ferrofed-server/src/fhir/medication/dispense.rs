// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The receipt of a dispensation on the FHIR face.

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;
use eehrxf::category::Category;
use eehrxf::dataset::DatasetError;
use eehrxf::medication::Mpd;
use ferrofed_eehrxf::dispensations::{self, Unread};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use http::StatusCode;

use crate::auth::caller::Caller;
use crate::federation::Federation;
use crate::state::AppState;

use crate::fhir::{admission, fhir_json, refusal};

use super::{not_served, serves};

/// `POST {fhir-base}/MedicationDispense`: a dispensation, written to the
/// one member that owns the prescription it names
/// ([`crate::facade::dispensation`]).
///
/// The body is one `MedicationDispense` in FHIR JSON (FHIR R4 `http.html`
/// §3.1.0.8), held to the HL7 Europe MPD 1.0.0 profiles before any member
/// is asked: a text that is no `MedicationDispense` is `400`, and one that
/// breaks its profiles `422` with one issue per finding. A written
/// dispensation answers `201` with a `Location` naming it by the face's id.
pub(in crate::fhir) async fn dispense(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    headers: http::HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Dispensation) {
        return not_served(Category::Dispensation);
    }
    if !crate::fhir::fhir_json_body(&headers) {
        return refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a dispensation is sent as application/fhir+json",
        );
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the dispensation is not UTF-8 text (RFC 8259 §8.1)",
        );
    };
    let mpd = match &*MPD {
        Ok(mpd) => mpd,
        Err(failure) => {
            tracing::error!(error = %crate::chain(failure), "the HL7 Europe MPD package this build carries could not be read");
            return refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the dispensation profiles could not be read",
            );
        }
    };
    let received = match dispensations::read(text, mpd) {
        Ok(received) => received,
        Err(unread) => return unread_answer(&unread),
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to write a dispensation",
            )
        },
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let targeted = match targeted(&admitted.federation, &headers) {
        Ok(targeted) => targeted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    crate::facade::dispensation::dispense(
        &admitted.federation,
        admitted.fhir,
        &arrived,
        (&received, &body),
        &targeted,
    )
    .await
}

/// The HL7 Europe MPD 1.0.0 profiles a received dispensation is held to,
/// read from the package this build carries.
static MPD: LazyLock<Result<Mpd, DatasetError>> = LazyLock::new(|| {
    Mpd::read(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz"
    )))
});

/// The endpoints the `openEHR-federation-endpoint` header names, each one
/// the registry of `federation` holds (§8.4).
///
/// # Errors
///
/// The `400` answer of a header that is no list of endpoint ids, or that
/// names one the registry does not hold, with nothing sent.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
fn targeted(
    federation: &Federation,
    headers: &http::HeaderMap,
) -> Result<BTreeSet<EndpointId>, Response> {
    let malformed = || {
        refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "openEHR-federation-endpoint names no endpoint of the registry (§8.4)",
        )
    };
    let snapshot = federation.snapshot();
    let mut named = BTreeSet::new();
    for value in headers.get_all(openehr_federation::headers::ENDPOINT) {
        let value = value.to_str().map_err(|_unread| malformed())?;
        for id in value.split(',').map(str::trim) {
            let id = EndpointId::new(id).map_err(|_unparsed| malformed())?;
            if snapshot.endpoint(&id).is_none() && snapshot.native_endpoint(&id).is_none() {
                return Err(malformed());
            }
            named.insert(id);
        }
    }
    Ok(named)
}

/// The answer to a dispensation refused before any member is asked.
fn unread_answer(unread: &Unread) -> Response {
    match unread {
        Unread::NonConformant { findings } => {
            let mut response = fhir_json(
                StatusCode::UNPROCESSABLE_ENTITY,
                &ferrofed_eehrxf::receive::nonconformant(findings),
            );
            response.extensions_mut().insert(crate::access::NoAccess);
            response
        }
        Unread::Check(_) => {
            tracing::error!(error = %crate::chain(unread), "a dispensation's profiles could not be checked");
            refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the dispensation profiles could not be checked",
            )
        }
        _ if unread.is_form() => refusal(StatusCode::BAD_REQUEST, "invalid", &crate::chain(unread)),
        _ => refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            "processing",
            &crate::chain(unread),
        ),
    }
}
