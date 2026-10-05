// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-93 route: one Mobile Patient Identity Feed message, applied to the
//! resolution bindings when it is authenticated and holds to the PMIR
//! profiles (PMIR 1.6.0 §2:3.93.4).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_identity::ihe::pmir;
use http::header::{CONTENT_TYPE, WWW_AUTHENTICATE};
use http::{HeaderMap, HeaderValue, StatusCode};
use ihe_iti::balp::AuditError;
use ihe_iti::pmir::error::FeedError;
use ihe_iti::pmir::feed::{EventKind, Feed, ResponseId, refusal};

use crate::binding::ihe::metrics::FeedResult;
use crate::facade::security::TARGET;
use crate::state::AppState;

/// The media type of every FHIR answer the route gives (ITI TF-2 Appendix
/// Z.6).
const FHIR_JSON: &str = "application/fhir+json";

/// The `realm` of the route's `WWW-Authenticate` challenge (RFC 6750 §3).
const REALM: &str = "ferrofed-pmir";

/// `POST {base}{pmir.path}`: one ITI-93 message, applied to the resolution
/// bindings when it is authenticated and holds to the PMIR profiles.
pub async fn feed(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(identity_feed) = state.identity_feed() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !identity_feed.authenticated(&headers) {
        tracing::warn!(
            target: TARGET,
            event = "identity-feed-refused",
            reason = "unauthenticated",
            "an ITI-93 message without the feed token was refused, and nothing was applied"
        );
        state.metrics().identity_feed(FeedResult::Unauthenticated);
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        if let Ok(challenge) = HeaderValue::from_str(&format!("Bearer realm=\"{REALM}\"")) {
            response.headers_mut().insert(WWW_AUTHENTICATE, challenge);
        }
        return response;
    }
    let media = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    // NOTE: PMIR §2:3.93.5.1: the Consumer records each message; one whose record
    // is refused is neither applied nor answered, so the Supplier sends it again.
    let message = match Feed::read(media, &body) {
        Ok(message) => message,
        Err(error) => {
            if let Err(failure) = identity_feed.record(None).await {
                return unrecorded(&state, &failure);
            }
            return refused(&state, &error);
        }
    };
    if let Err(failure) = identity_feed.record(Some(&message)).await {
        return unrecorded(&state, &failure);
    }
    let Some(federation) = state.federation() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let change = pmir::change_of(&message, identity_feed.domains());
    let dropped = change
        .as_ref()
        .map_or(0, |change| federation.identity_changed(change));
    tracing::info!(
        creates = message.count(EventKind::Create),
        updates = message.count(EventKind::Update),
        deletes = message.count(EventKind::Delete),
        merges = message.count(EventKind::Merge),
        changed = change.is_some(),
        dropped,
        "an ITI-93 message was applied to the resolution bindings"
    );
    state.metrics().identity_feed(FeedResult::Applied);
    let id = uuid::Uuid::new_v4().hyphenated().to_string();
    let written = ResponseId::new(&id)
        .ok()
        .and_then(|id| message.acknowledgement(&id, identity_feed.source()).ok());
    match written {
        Some(bytes) => fhir(StatusCode::OK, bytes),
        // NOTE: PMIR §2:3.93.4.2.2: the message was processed, so a response that
        // cannot be written leaves a bare 2xx, never a failure the Supplier retries.
        None => StatusCode::OK.into_response(),
    }
}

/// The answer to a message that does not hold to the PMIR profiles: `415`
/// for another media type, `400` otherwise, with the refusal's
/// `OperationOutcome` (§2:3.93.4.2.2).
fn refused(state: &AppState, error: &FeedError) -> Response {
    tracing::warn!(
        target: TARGET,
        event = "identity-feed-refused",
        reason = "malformed",
        error = %error,
        "an ITI-93 message that does not hold to PMIR was refused, and nothing was applied"
    );
    state.metrics().identity_feed(FeedResult::Refused);
    let status = match error {
        FeedError::NotFhirJson => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        _ => StatusCode::BAD_REQUEST,
    };
    match refusal(error) {
        Ok(bytes) => fhir(status, bytes),
        Err(_unwritable) => status.into_response(),
    }
}

/// The answer to a message whose audit record could not be stored: `503`,
/// with nothing applied, so the Supplier sends it again.
fn unrecorded(state: &AppState, failure: &AuditError) -> Response {
    tracing::error!(
        target: TARGET,
        event = "identity-feed-refused",
        reason = "audit-failed",
        error = crate::chain(failure),
        "the audit record of an ITI-93 message could not be stored, and nothing was applied"
    );
    state.metrics().identity_feed(FeedResult::AuditFailed);
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

/// `bytes` as a FHIR JSON answer of `status`.
fn fhir(status: StatusCode, bytes: Vec<u8>) -> Response {
    (status, [(CONTENT_TYPE, FHIR_JSON)], bytes).into_response()
}
