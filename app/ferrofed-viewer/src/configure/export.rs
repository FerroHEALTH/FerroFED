// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The download of a configuration revision as TOML, served by the
//! console's server at [`EXPORT`].
//!
//! The route checks the operator's session itself, reads the revision from
//! the first replica's admin listener with the operator's own access token,
//! and answers it as an attachment that no cache keeps. A replica's refusal
//! is answered with its status and stable code, never an empty file. No
//! specification governs this: our own design.

use axum::Extension;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use ferrofed_registry::revision::RevisionId;
use http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, LOCATION};
use http::{HeaderMap, HeaderValue, StatusCode};

use crate::gateway::AccessToken;
use crate::server::ViewerState;

/// The path of a revision's download.
pub const EXPORT: &str = "/configuration/revisions/{id}/export";

/// The path of revision `id`'s download.
#[must_use]
pub fn href(id: RevisionId) -> String {
    format!("/configuration/revisions/{id}/export")
}

/// A plain-text answer of `status` saying `said`, never cached.
fn plain(status: StatusCode, said: String) -> Response {
    (
        status,
        [
            (CACHE_CONTROL, "no-store"),
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
        ],
        said,
    )
        .into_response()
}

/// Serves `GET` [`EXPORT`]: revision `id` as a TOML attachment, read from
/// the first replica as the signed-in operator.
pub async fn export(
    Extension(state): Extension<ViewerState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let sessions = state.sessions();
    let held = crate::oidc::cookie_named(&headers, &sessions.cookie_name(crate::session::COOKIE))
        .map(|session| sessions.access_token(&session));
    let token = match held {
        Some(Ok(Some(token))) => AccessToken::new(token),
        Some(Err(error)) => {
            tracing::error!(%error, "the session store could not be read");
            return plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("The session store could not be read."),
            );
        }
        Some(Ok(None)) | None => {
            let mut response = StatusCode::SEE_OTHER.into_response();
            response
                .headers_mut()
                .insert(LOCATION, HeaderValue::from_static(crate::app::SIGN_IN));
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return response;
        }
    };
    let Ok(id) = id.parse::<RevisionId>() else {
        return plain(
            StatusCode::NOT_FOUND,
            String::from("The path names no revision number."),
        );
    };
    let Some(replica) = state.settings().admin.first() else {
        return plain(
            StatusCode::NOT_FOUND,
            String::from(
                "This console is configured with no admin listener, so it exports no configuration.",
            ),
        );
    };
    match state.gateway().export_revision(replica, &token, id).await {
        Ok(toml) => attachment(id, toml),
        Err(error) => {
            let name = replica.name.as_str();
            if let Some((status, code)) = error.refusal() {
                let code = code.unwrap_or_else(|| String::from("no code"));
                return plain(
                    status,
                    format!(
                        "Replica {name} refused the export: {} ({code}).",
                        status.as_u16()
                    ),
                );
            }
            tracing::warn!(replica = name, error = %error, "a configuration export failed");
            plain(
                StatusCode::BAD_GATEWAY,
                format!("Replica {name} could not be reached, or its answer read, for the export."),
            )
        }
    }
}

/// The `200` of revision `id`'s TOML, an attachment never cached.
fn attachment(id: RevisionId, toml: Vec<u8>) -> Response {
    let mut response = (StatusCode::OK, toml).into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/toml; charset=utf-8"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"ferrofed-revision-{id}.toml\""
    )) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    response
}
