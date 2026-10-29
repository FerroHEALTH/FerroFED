// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The public documents a binding has the gateway serve.
//!
//! One is the DID document of a Nuts grant's holder at its `did:web`
//! location (the did:web Method Specification, Read (Resolve); Nuts RFC021
//! §4.2 item 4). A document is public material, like the JWK Set at
//! `{base}/.well-known/jwks.json`: it is answered to a `GET` or `HEAD` of its
//! exact path before the client authentication gate, with no credential, and
//! it never sits inside the ITS-REST surface (the federation refuses one
//! there at load). Its path is the one its binding names, which a `did:web`
//! DID fixes absolutely, so it is matched against the request path as the
//! client sent it, under the configured base or outside it. The running
//! federation is read per request, so a reload that changes a key serves the
//! new document. No specification governs the mechanism: our own design.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, Method, StatusCode, header};

use crate::state::AppState;

/// Answers a `GET` or `HEAD` of a public document's path with the document,
/// and passes every other request on.
pub async fn serve(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    match answer(&state, &request) {
        Some(document) => document,
        None => next.run(request).await,
    }
}

/// The answer to `request` when it is a `GET` or `HEAD` of a public
/// document's path, served outside the base path too.
#[must_use]
pub fn answer(state: &AppState, request: &Request) -> Option<Response> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return None;
    }
    // NOTE: no specification governs this: our own design; a router nested under the
    // base path sees the path past it, and a document's path is the one the client sent.
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path());
    let (media_type, body) = state.federation().and_then(|federation| {
        federation
            .document(path)
            .map(|document| (document.media_type, document.body.clone()))
    })?;
    let body = if request.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(body)
    };
    Some(
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, HeaderValue::from_static(media_type))],
            body,
        )
            .into_response(),
    )
}
