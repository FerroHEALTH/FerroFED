// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A console sign-in completed at a test OpenID Provider of its own.
//!
//! `ferrohealth-smart`'s relying party exchanges a code for the access token
//! a test names and an ID Token this provider signs (OpenID Connect Core 1.0
//! §3.1.3), so the session a console test begins holds what a real sign-in
//! leaves. No specification governs the harness: our own design.

use std::error::Error;
use std::time::Duration;

use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_session::config::OidcSection;
use ferrohealth_session::record::SignedIn;
use ferrohealth_smart::oidc::{Callback, PendingSignIn};
use jsonwebtoken::{Algorithm, Header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::issuer::{Issuer, JWKS_PATH};
use crate::mock::Server;

/// The console's client id at the test provider.
pub const CLIENT_ID: &str = "ferrofed-console";

/// What a sign-in at the test provider is answered with.
#[derive(Debug, Clone, Copy)]
pub struct Answer<'a> {
    /// The access token.
    pub access_token: &'a str,
    /// The operator's name the ID Token carries.
    pub name: Option<&'a str>,
    /// The access token's lifetime.
    pub expires_in: Option<Duration>,
    /// The refresh token.
    pub refresh_token: Option<&'a str>,
}

/// What a sign-in answered with `answer` leaves a session, with the ID Token
/// it was completed with.
///
/// # Errors
///
/// Returns the error of a test provider that cannot start, or of a sign-in
/// the relying party refuses.
pub async fn signed_in(answer: Answer<'_>) -> Result<(SignedIn, String), Box<dyn Error>> {
    let server = Server::start().await;
    let uri = server.uri();
    let issuer = Issuer::new(uri.clone())?;
    issuer.publish(&server).await?;
    let section = OidcSection {
        issuer: uri.clone(),
        authorization_endpoint: Some(SecretUrl::new(format!("{uri}/auth"))),
        token_endpoint: Some(SecretUrl::new(format!("{uri}/token"))),
        jwks_uri: Some(SecretUrl::new(format!("{uri}{JWKS_PATH}"))),
        client_id: String::from(CLIENT_ID),
        redirect_uri: String::from("http://127.0.0.1:3000/auth/callback"),
        ..OidcSection::default()
    };
    let party = section.check()?.relying_party().await?;
    let pending = PendingSignIn::mint()?;
    let now = jiff::Timestamp::now().as_second();
    let claims = serde_json::json!({
        "iss": issuer.name(),
        "aud": CLIENT_ID,
        "sub": "synthetic-operator",
        "iat": now,
        "exp": now + 300,
        "nonce": pending.nonce(),
        "name": answer.name,
    });
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    let id_token = issuer.sign(&header, &claims.to_string())?;
    let mut body = serde_json::Map::new();
    body.insert(String::from("access_token"), answer.access_token.into());
    body.insert(String::from("token_type"), "Bearer".into());
    body.insert(String::from("id_token"), id_token.clone().into());
    if let Some(lifetime) = answer.expires_in {
        body.insert(String::from("expires_in"), lifetime.as_secs().into());
    }
    if let Some(refresh) = answer.refresh_token {
        body.insert(String::from("refresh_token"), refresh.into());
    }
    let body = serde_json::Value::Object(body);
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(body.to_string()),
        )
        .mount(&server)
        .await;
    let signed = party
        .exchange(
            &pending,
            &Callback::granted("synthetic-code", pending.state()),
        )
        .await?;
    Ok((SignedIn::from_oidc(signed), id_token))
}
