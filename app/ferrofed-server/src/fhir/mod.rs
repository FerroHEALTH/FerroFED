// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 face of the European exchange format, served on its own base,
//! `{fhir-base}` (Regulation (EU) 2025/327 Annex II 2.1).
//!
//! `GET {fhir-base}/metadata` answers the face's `CapabilityStatement`, and
//! `GET` or `POST {fhir-base}/Patient/$summary` the patient summary as an
//! HL7 Europe Patient Summary document `Bundle`, by the International Patient
//! Summary 2.0.0 `$summary` operation. `POST {fhir-base}/Bundle` receives a
//! document in the exchange format, at the `Bundle` end-point FHIR R4
//! documents §3.3.4 names (Annex II 2.2 and 2.3), and
//! `GET {fhir-base}/DocumentReference` lists the summaries that exist for a
//! patient ([`ferrofed_eehrxf::list`]), and `GET {fhir-base}/Patient` looks a
//! patient up by identifier ([`ferrofed_eehrxf::lookup`]). `MedicationRequest` and
//! `MedicationDispense` answer a category no member is declared for as not
//! served ([`medication`]). Every other path under the base is a
//! `404`. A summary runs the gateway's own section queries over
//! the members (`facade::summary`), maps each composition through the
//! deployment's FHIRconnect mappings and writes the document
//! ([`ferrofed_eehrxf::summary`]); a received document is written to the
//! one member its category is declared for (`facade::receive`). The face
//! sits beside `{base}`, so no ITS-REST path changes (Federation Tier §4.1,
//! N28, N1).
//!
//! Every request is authenticated at the same gate as the ITS-REST face
//! (§13.1, N25), and every summary, list or document that reached a member
//! is recorded in the access log, naming the verified caller (Annex II 3.2).
//! Every error the face answers is an `OperationOutcome` ([`errors::outcomes`]): the gateway's own
//! refusals keep their status and their headers, and the error body is
//! rewritten as the outcome. No specification governs the mapping of the
//! gateway's errors to issue types: our own design, over the FHIR R4
//! `issue-type` codes.

use std::ops::ControlFlow;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;
use axum::routing::{MethodRouter, get, post};
use ferrofed_eehrxf::face::{self, FHIR_JSON, SummaryRequest};
use ferrofed_eehrxf::receive::Receivers;
use ferrofed_eehrxf::serving::Serving;
use http::{StatusCode, Uri, header};

use crate::access::NoAccess;
use crate::face_path::{BUNDLE, DOCUMENTS, METADATA, PATIENTS, SUMMARY};
use crate::fhir::admission::Asked;
use crate::fhir_wire::fhir_json;
use crate::state::AppState;

mod admission;
mod binary;
pub mod errors;
mod laboratory;
mod list;
mod lookup;
pub mod medication;
mod own;
mod patient_summary;
mod trc;

/// The product name the document's author `Device` and the capability
/// statement carry.
const PRODUCT: &str = "FerroFED";

/// The text of the `404` every other path answers, naming each path
/// [`table`] routes.
static UNSERVED: LazyLock<String> = LazyLock::new(|| {
    let paths: Vec<&str> = table()
        .iter()
        .map(|(path, _)| path.trim_start_matches('/'))
        .collect();
    format!("the FHIR face serves {} alone", paths.join(", "))
});

/// Every path of the face with its handlers, the one list the router is
/// built from and the `404` of any other path names.
fn table() -> [(&'static str, MethodRouter<Arc<AppState>>); 12] {
    [
        (METADATA, get(metadata)),
        (PATIENTS, get(lookup::patients)),
        (SUMMARY, get(summary_get).post(summary_post)),
        (BUNDLE, post(bundle_post)),
        (DOCUMENTS, get(list::documents)),
        (binary::ROUTE, get(binary::read)),
        (
            crate::face_path::PRESCRIPTIONS,
            get(medication::search::search),
        ),
        (crate::face_path::PRESCRIPTION, get(medication::read::read)),
        (
            crate::face_path::PRESCRIPTION_VERSION,
            get(medication::read::vread),
        ),
        (
            crate::face_path::DISPENSATIONS,
            post(medication::dispense::dispense),
        ),
        (
            crate::face_path::DISPENSATION,
            get(medication::read::read_dispensation).put(medication::withdraw::withdraw),
        ),
        (
            crate::face_path::DISPENSATION_VERSION,
            get(medication::read::vread_dispensation),
        ),
    ]
}

/// The routes of the face, which the router nests under `{fhir-base}`.
pub fn routes() -> Router<Arc<AppState>> {
    table()
        .into_iter()
        .fold(Router::new(), |router, (path, handlers)| {
            router.route(path, handlers)
        })
        // NOTE: no specification governs this: our own design; every other path under
        // `{fhir-base}` is a `404` naming the paths the face serves, never the path asked for.
        .fallback(|| async { outcome(StatusCode::NOT_FOUND, "not-found", &UNSERVED) })
}

/// `GET {fhir-base}/metadata`: the face's `CapabilityStatement`, with the
/// categories the deployment receives documents of and those it serves.
async fn metadata(State(state): State<Arc<AppState>>) -> Response {
    let date = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
    let (no_receivers, no_serving) = (Receivers::default(), Serving::default());
    let receivers = state
        .fhir()
        .map_or(&no_receivers, |fhir| fhir.receivers.as_ref());
    let serving = state
        .fhir()
        .map_or(&no_serving, |fhir| fhir.serving.as_ref());
    fhir_json(
        StatusCode::OK,
        &face::capability(
            PRODUCT,
            env!("CARGO_PKG_VERSION"),
            &date,
            (receivers, serving),
        ),
    )
}

/// `GET {fhir-base}/Patient/$summary`.
async fn summary_get(asked: Asked, uri: Uri) -> Response {
    let started = Instant::now();
    let read = SummaryRequest::from_query(uri.query());
    patient_summary::summarised((&asked, started), read).await
}

/// `POST {fhir-base}/Patient/$summary` with a `Parameters` body.
async fn summary_post(asked: Asked, body: Bytes) -> Response {
    let started = Instant::now();
    if !fhir_json_body(&asked.headers) {
        return outcome(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a Parameters body is sent as application/fhir+json",
        );
    }
    let read = SummaryRequest::from_parameters(&body);
    patient_summary::summarised((&asked, started), read).await
}

/// `POST {fhir-base}/Bundle` with a document in the exchange format: the
/// document written to the member that receives its category
/// (`facade::receive`).
async fn bundle_post(asked: Asked, body: Bytes) -> Response {
    let started = Instant::now();
    if !fhir_json_body(&asked.headers) {
        return refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a document is sent as application/fhir+json",
        );
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the document is not UTF-8 text (RFC 8259 §8.1)",
        );
    };
    admission::served(
        (&asked, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to create a composition",
            )
        },
        |_| ControlFlow::Continue(()),
        async |admitted, arrived, ()| {
            crate::facade::receive::receive(
                &admitted.federation,
                admitted.fhir,
                (arrived, admitted.caller),
                text,
            )
            .await
        },
    )
    .await
}

/// Whether `headers` declare a FHIR JSON body.
fn fhir_json_body(headers: &http::HeaderMap) -> bool {
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    matches!(media, Some(FHIR_JSON | "application/json"))
}

/// The text of the `OperationOutcome` a caller the gate did not admit to the
/// section queries a summary, a list or a listed document runs over is
/// answered `403` with.
const UNADMITTED: &str = "the caller was not admitted to the section queries";

/// The `OperationOutcome` of one error, answered with `status`, stating that
/// no member was sent anything.
fn refusal(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    let mut response = outcome(status, code, diagnostics);
    response.extensions_mut().insert(NoAccess);
    response
}

/// The answer to a search of the face `asked`, begun at `started`, whose
/// query read as `read`.
///
/// A query the face cannot read is `400`, stating that no member was sent
/// anything; a caller the gate did not admit is `403` with `unadmitted` as
/// its text; a search `held` breaks off is answered with its answer before
/// anything is conveyed; every other is `serve`'s.
async fn searched<R, E: std::fmt::Display>(
    (asked, started): (&Asked, Instant),
    read: Result<R, E>,
    unadmitted: &str,
    held: impl FnOnce(&R) -> ControlFlow<Response>,
    serve: impl AsyncFnOnce(
        &R,
        &admission::Admitted<'_>,
        crate::facade::request::Arrived<'_>,
    ) -> Response,
) -> Response {
    let read = match read {
        Ok(read) => read,
        Err(refusal) => {
            let mut response = outcome(StatusCode::BAD_REQUEST, "invalid", &refusal.to_string());
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    admission::served(
        (asked, started),
        || outcome(StatusCode::FORBIDDEN, "forbidden", unadmitted),
        |_| held(&read),
        async |admitted, arrived, ()| serve(&read, admitted, arrived).await,
    )
    .await
}

/// Attaches `accessed` to `response` for the access log, or the statement
/// that no member was sent anything.
fn attach(response: &mut Response, accessed: Option<crate::access::Accessed>) {
    if let Some(accessed) = accessed {
        response.extensions_mut().insert(accessed);
    } else {
        response.extensions_mut().insert(NoAccess);
    }
}

/// The `OperationOutcome` of one error of the FHIR R4 issue type `code`,
/// with `status`.
fn outcome(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    fhir_json(status, &face::outcome(code, diagnostics))
}

#[cfg(test)]
mod tests {
    use super::UNSERVED;

    #[test]
    fn the_404_names_every_path_the_router_serves() {
        assert_eq!(
            UNSERVED.as_str(),
            "the FHIR face serves metadata, Patient, Patient/$summary, Bundle, DocumentReference, \
             Binary/{token}, MedicationRequest, MedicationRequest/{id}, \
             MedicationRequest/{id}/_history/{vid}, MedicationDispense, MedicationDispense/{id}, \
             MedicationDispense/{id}/_history/{vid} alone"
        );
    }
}
