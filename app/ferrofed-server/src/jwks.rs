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
//! Security Profile §5.4.2; Annex B §B.4a.2), and the assertion key of each
//! `oauth` client credentials grant (RFC 7523 §2.2). It is public: a node
//! fetches it without authenticating, and it holds no private key material.
//! A gateway with no key to publish answers `404`.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use jsonwebtoken::jwk::JwkSet;

use crate::error::{self, Code};
use crate::request_id;
use crate::state::AppState;

/// The media type of a JWK Set (RFC 7517 §8.5.1).
const JWK_SET_MEDIA_TYPE: &str = "application/jwk-set+json";

/// `GET {base}/.well-known/jwks.json`: the keys the gateway publishes now.
pub async fn jwks(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotFound, request_id);
    };
    let mut published = match federation.signing() {
        Some(signing) => match serde_json::from_value::<JwkSet>(signing.keys.published()) {
            Ok(published) => published,
            Err(failure) => {
                tracing::error!(error = %failure, "the signing keys' JWK Set could not be read");
                return error::fixed(Code::Internal, request_id);
            }
        },
        None if federation.client_keys().is_empty() => {
            return error::fixed(Code::NotFound, request_id);
        }
        None => JwkSet { keys: Vec::new() },
    };
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
