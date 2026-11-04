// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the FHIR face and the facade write onto the FHIR wire: an answer as
//! FHIR JSON, and the logical id a resource is read back by ([`locator`]).
//! No specification governs the layout: our own design.

pub mod locator;

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use ferrofed_eehrxf::face::FHIR_JSON;
use http::{HeaderValue, StatusCode, header};
use serde::Serialize;

/// `resource` as FHIR JSON with `status`.
pub(crate) fn fhir_json(status: StatusCode, resource: &impl Serialize) -> Response {
    match serde_json::to_vec(resource) {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON))],
            body,
        )
            .into_response(),
        Err(_unencoded) => (StatusCode::INTERNAL_SERVER_ERROR, Body::empty()).into_response(),
    }
}
