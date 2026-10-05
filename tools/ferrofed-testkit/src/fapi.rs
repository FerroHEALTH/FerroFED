// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness FAPI 2.0 authorization server, a test device in place of a
//! node's authorization server on the BgZ/eOverdracht track of Annex B
//! §B.4a.
//!
//! **This is a test device, not an authorization server.** It is the
//! harness token endpoint ([`crate::oauth::TokenEndpoint`]) held to what the
//! FAPI 2.0 Security Profile asks of a server, with its metadata beside it on
//! the same origin:
//!
//! - its issuer identifier is the server's base URL, and its metadata is
//!   served at the RFC 8414 §3.1 well-known URL, `application/json`, naming
//!   that issuer, its token endpoint, `private_key_jwt` with `ES256`, the
//!   client-credentials and token-exchange grants, `ES256` for `DPoP`, and
//!   the authorization details type [`DETAILS_TYPE`]
//!   ([`AuthorizationServer::metadata`]); a test replaces the document with
//!   [`AuthorizationServer::publish`] or [`AuthorizationServer::publish_raw`];
//! - every client assertion is verified as `ES256`, with the issuer as its
//!   one-string `aud` (FAPI 2.0 §5.3.2.1, §5.4.1);
//! - every token request must carry a `DPoP` proof, and every token is bound
//!   to its key (§5.3.2.1, RFC 9449 §5).
//!
//! A test that configures `authorization_details` requires them with
//! [`crate::oauth::TokenEndpoint::require_authorization_details`] on
//! [`AuthorizationServer::endpoint`]. [`AuthorizationServer::mutual_tls`]
//! starts a server for the profile's other choice (§5.3.2.1): it
//! authenticates the client by its TLS certificate and requires no `DPoP`
//! proof (RFC 8705 §2), behind a [`crate::tls::MutualTls`] front the test
//! starts over [`AuthorizationServer::issuer`], with the metadata the test
//! publishes. No specification governs the device: our own design.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use jsonwebtoken::Algorithm;
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::oauth::{TOKEN_EXCHANGE, TokenEndpoint};

/// The path of the metadata of an issuer with no path (RFC 8414 §3.1).
pub const METADATA_PATH: &str = "/.well-known/oauth-authorization-server";

/// The authorization details type of the Annex B §B.4a.3 example.
pub const DETAILS_TYPE: &str = "nl-gis-v1";

/// The members of a metadata document (RFC 8414 §2, RFC 9449 §5.1, RFC 9396
/// §10), each left out when `None`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Metadata {
    /// `issuer`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// `token_endpoint`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,
    /// `grant_types_supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types_supported: Option<Vec<String>>,
    /// `token_endpoint_auth_methods_supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
    /// `token_endpoint_auth_signing_alg_values_supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_signing_alg_values_supported: Option<Vec<String>>,
    /// `dpop_signing_alg_values_supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpop_signing_alg_values_supported: Option<Vec<String>>,
    /// `authorization_details_types_supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_details_types_supported: Option<Vec<String>>,
    /// `tls_client_certificate_bound_access_tokens` (RFC 8705 §3.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_client_certificate_bound_access_tokens: Option<bool>,
    /// `mtls_endpoint_aliases` (RFC 8705 §5), by endpoint name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtls_endpoint_aliases: Option<BTreeMap<String, String>>,
}

/// The document the metadata URL answers, and its media type.
#[derive(Debug, Clone)]
struct Published {
    body: String,
    media: String,
}

/// The harness FAPI 2.0 authorization server.
#[derive(Debug)]
pub struct AuthorizationServer {
    endpoint: TokenEndpoint,
    published: Arc<Mutex<Published>>,
}

impl AuthorizationServer {
    /// Starts a server that authenticates the client `client_id` and issues
    /// tokens that live `expires_in` seconds, stating none when `expires_in`
    /// is `None`.
    pub async fn start(client_id: &str, expires_in: Option<u64>) -> Self {
        let endpoint = TokenEndpoint::start(client_id, expires_in).await;
        let issuer = endpoint.server().uri();
        endpoint.expect_assertion(Algorithm::ES256, &issuer);
        endpoint.require_dpop();
        Self::serving(endpoint).await
    }

    /// Starts a server that authenticates the client `client_id` by its TLS
    /// certificate and requires no `DPoP` proof (RFC 8705 §2), issuing
    /// tokens as [`AuthorizationServer::start`] does; a test binds them to
    /// a certificate with
    /// [`crate::oauth::TokenEndpoint::bind_to_certificate`].
    pub async fn mutual_tls(client_id: &str, expires_in: Option<u64>) -> Self {
        let endpoint = TokenEndpoint::start(client_id, expires_in).await;
        endpoint.accept_tls_client_auth();
        Self::serving(endpoint).await
    }

    /// The server around `endpoint`, publishing its default metadata.
    async fn serving(endpoint: TokenEndpoint) -> Self {
        let published = Arc::new(Mutex::new(Published {
            body: String::new(),
            media: String::from("application/json"),
        }));
        Mock::given(method("GET"))
            .and(path(METADATA_PATH))
            .respond_with(Document(Arc::clone(&published)))
            .mount(endpoint.server())
            .await;
        let server = Self {
            endpoint,
            published,
        };
        server.publish(&server.metadata());
        server
    }

    /// The issuer identifier: the server's base URL.
    #[must_use]
    pub fn issuer(&self) -> String {
        self.endpoint.server().uri()
    }

    /// The token endpoint, whose matchers a mock node requires a token with.
    #[must_use]
    pub fn endpoint(&self) -> &TokenEndpoint {
        &self.endpoint
    }

    /// The metadata the server publishes when it starts: everything a FAPI
    /// 2.0 client of it needs.
    #[must_use]
    pub fn metadata(&self) -> Metadata {
        let list = |items: &[&str]| Some(items.iter().map(|item| (*item).to_owned()).collect());
        Metadata {
            issuer: Some(self.issuer()),
            token_endpoint: Some(self.endpoint.token_url()),
            grant_types_supported: list(&["client_credentials", TOKEN_EXCHANGE]),
            token_endpoint_auth_methods_supported: list(&["private_key_jwt"]),
            token_endpoint_auth_signing_alg_values_supported: list(&["ES256", "PS256"]),
            dpop_signing_alg_values_supported: list(&["ES256"]),
            authorization_details_types_supported: list(&[DETAILS_TYPE]),
            tls_client_certificate_bound_access_tokens: None,
            mtls_endpoint_aliases: None,
        }
    }

    /// Publishes `metadata` in place of the document served so far.
    ///
    /// # Panics
    ///
    /// Panics when the document cannot be written, which a [`Metadata`]
    /// cannot cause.
    #[expect(
        clippy::expect_used,
        reason = "a Metadata of strings and lists of strings always serializes"
    )]
    pub fn publish(&self, metadata: &Metadata) {
        let body = serde_json::to_string(metadata).expect("the metadata should serialize");
        self.publish_raw(&body, "application/json");
    }

    /// Publishes `body`, as written, under the media type `media`.
    pub fn publish_raw(&self, body: &str, media: &str) {
        let mut published = self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        body.clone_into(&mut published.body);
        media.clone_into(&mut published.media);
    }
}

/// The responder of the metadata URL.
struct Document(Arc<Mutex<Published>>);

impl Respond for Document {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let published = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        ResponseTemplate::new(200).set_body_raw(published.body.into_bytes(), &published.media)
    }
}
