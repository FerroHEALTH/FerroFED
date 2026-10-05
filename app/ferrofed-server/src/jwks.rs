// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {base}/.well-known/jwks.json`: the gateway's public signing keys as
//! a JWK Set (RFC 7517 §5), which a node's authorization server verifies
//! the gateway's client assertions against (§13.1, N25).
//!
//! The set holds the current key, the previous one during a rotation's
//! overlap window, and the next one ahead of a rotation, then the ES256
//! client key of each FAPI 2.0 grant, which that grant's authorization
//! server verifies its `private_key_jwt` assertions against (FAPI 2.0
//! Security Profile §5.4.2; Annex B §B.4a.2). It is public: a node fetches
//! it without authenticating, and it holds no private key material. A
//! gateway with no `[signing]` keys answers `404`.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_engine::onward::keys::JWK_SET_MEDIA_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::error::{self, Code};
use crate::request_id;
use crate::state::AppState;

/// The path of the JWK Set under `{base}`.
pub const JWKS_PATH: &str = "/.well-known/jwks.json";

/// `GET {base}/.well-known/jwks.json`: the keys the gateway publishes now.
pub async fn jwks(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotFound, request_id);
    };
    let Some(signing) = federation.signing() else {
        return error::fixed(Code::NotFound, request_id);
    };
    let mut published = signing.keys.published();
    for client_key in federation.client_keys() {
        if !published
            .keys
            .iter()
            .any(|key| key.common.key_id == client_key.common.key_id)
        {
            published.keys.push(client_key.clone());
        }
    }
    match serde_json::to_vec(&published) {
        Ok(body) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(JWK_SET_MEDIA_TYPE),
            )],
            body,
        )
            .into_response(),
        Err(failure) => {
            tracing::error!(error = %failure, "the JWK Set could not be written");
            error::fixed(Code::Internal, request_id)
        }
    }
}
