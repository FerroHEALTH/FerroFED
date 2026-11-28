// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {base}/.well-known/jwks.json`: the gateway's public signing keys as
//! a JWK Set (RFC 7517 §5), which a node's authorization server verifies
//! the gateway's client assertions against (§13.1, N25).
//!
//! The set holds the current key, the previous one during a rotation's
//! overlap window, and the next one ahead of a rotation, then the ML-DSA key
//! of `[signing]` (RFC 9964), then the client key of each FAPI 2.0 grant,
//! which that grant's authorization server verifies its `private_key_jwt`
//! assertions against (FAPI 2.0 Security Profile §5.4.2; Annex B §B.4a.2),
//! and the assertion key of each `oauth` client credentials grant (RFC 7523
//! §2.2), an ML-DSA key among them. Each key appears once by its `kid`. It
//! is public: a node fetches it without authenticating, and it holds no
//! private key material. A gateway with no key to publish answers `404`.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrohealth_smart::jws::jwk::Jwk;
use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::{Deserialize, Serialize};

use crate::error::{self, Code};
use crate::request_id;
use crate::state::AppState;

/// The media type of a JWK Set (RFC 7517 §8.5.1).
const JWK_SET_MEDIA_TYPE: &str = "application/jwk-set+json";

/// A JWK Set (RFC 7517 §5) of public keys, each once by its `kid`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwkSet {
    /// The keys, in the order they were added.
    pub keys: Vec<Jwk>,
}

impl JwkSet {
    /// Adds `key` unless a key of the set has its `kid`.
    pub fn add(&mut self, key: Jwk) {
        if !self.keys.iter().any(|known| known.kid() == key.kid()) {
            self.keys.push(key);
        }
    }

    /// Adds every key of `set` ([`JwkSet::add`]).
    pub fn extend(&mut self, set: Self) {
        for key in set.keys {
            self.add(key);
        }
    }
}

/// `GET {base}/.well-known/jwks.json`: the keys the gateway publishes now.
pub async fn jwks(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    let Some(federation) = state.federation() else {
        return error::fixed(Code::NotFound, request_id);
    };
    let mut published = JwkSet::default();
    if let Some(signing) = federation.signing() {
        let ring = serde_json::from_value::<JwkSet>(signing.keys.published());
        let ml_dsa = signing
            .ml_dsa
            .as_ref()
            .map(|key| serde_json::from_value::<Jwk>(key.public_jwk()))
            .transpose();
        match (ring, ml_dsa) {
            (Ok(ring), Ok(ml_dsa)) => {
                published.extend(ring);
                published.keys.extend(ml_dsa);
            }
            (Err(failure), _) | (_, Err(failure)) => {
                tracing::error!(error = %failure, "the signing keys' JWK Set could not be read");
                return error::fixed(Code::Internal, request_id);
            }
        }
    }
    for client_key in federation.client_keys() {
        published.add(client_key.clone());
    }
    if published.keys.is_empty() {
        return error::fixed(Code::NotFound, request_id);
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
