// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The authorization code exchange at the provider's token endpoint and the
//! check of the ID Token it answers with.
//!
//! The code goes to the token endpoint with the PKCE verifier (RFC 6749
//! §4.1.3, RFC 7636 §4.5), with the client secret as HTTP Basic when the
//! console is a confidential client (RFC 6749 §2.3.1). The answer must carry
//! a bearer access token and an ID Token, whose signature verifies against
//! the provider's JWK Set and whose `iss`, `aud`, `azp`, `exp` and `nonce`
//! are those the sign-in expects (OpenID Connect Core 1.0 §3.1.3.7). No
//! token and no code is ever logged or rendered.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::StatusCode;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

use crate::config::settings::OidcSettings;
use crate::session::{PendingSignIn, SignedIn};

/// The signature algorithms an ID Token may use: the asymmetric ones, so
/// only the provider's own key verifies it.
pub const ALGORITHMS: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

/// How much clock skew the ID Token's times are allowed.
const LEEWAY: Duration = Duration::from_secs(60);

/// Why a sign-in could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// The provider could not be reached, or its answer could not be read.
    #[error("the OpenID Provider could not be reached")]
    Transport {
        /// What the HTTP client reported.
        #[source]
        source: reqwest::Error,
    },
    /// The token endpoint refused the code.
    #[error("the token endpoint answered {status}")]
    Refused {
        /// The status it answered with.
        status: StatusCode,
    },
    /// The token endpoint's answer is not a token response.
    #[error("the token endpoint's answer is not a token response")]
    Body {
        /// What the JSON reader refused.
        #[source]
        source: serde_json::Error,
    },
    /// The access token is not a bearer token, which the gateway takes.
    #[error("the access token is not a bearer token")]
    TokenType,
    /// The answer carries no ID Token.
    #[error("the token endpoint answered no ID Token")]
    NoIdToken,
    /// The provider's JWK Set could not be had or read.
    #[error("the provider's JWK Set could not be read")]
    KeySet,
    /// The ID Token is signed with an algorithm the console refuses, or by a
    /// key the JWK Set does not hold.
    #[error("the ID Token is signed with a key or an algorithm the console does not accept")]
    Key,
    /// The ID Token's signature or claims do not verify.
    #[error("the ID Token does not verify")]
    IdToken {
        /// What the verifier refused.
        #[source]
        source: jsonwebtoken::errors::Error,
    },
    /// The ID Token was issued to another client (`azp`).
    #[error("the ID Token was issued to another client")]
    AuthorizedParty,
    /// The ID Token's `nonce` is not the one the sign-in sent.
    #[error("the ID Token's nonce is not the sign-in's")]
    Nonce,
}

impl ExchangeError {
    /// Whether the failure is the ID Token's, which refuses the operator,
    /// rather than the provider's, which the operator can retry.
    #[must_use]
    pub const fn refuses_the_operator(&self) -> bool {
        matches!(
            self,
            Self::Key | Self::IdToken { .. } | Self::AuthorizedParty | Self::Nonce
        )
    }
}

/// The token endpoint's answer (RFC 6749 §5.1, OpenID Connect Core 1.0
/// §3.1.3.3).
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    id_token: Option<String>,
}

/// The ID Token claims the console checks beyond those the verifier checks.
#[derive(Deserialize)]
struct IdTokenClaims {
    nonce: Option<String>,
    azp: Option<String>,
}

/// Exchanges `code` for the operator's tokens and checks the ID Token
/// against `pending`.
///
/// # Errors
/// Returns the [`ExchangeError`] of the step that failed.
pub async fn exchange(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    pending: &PendingSignIn,
    code: &str,
) -> Result<SignedIn, ExchangeError> {
    let mut request = client
        .post(oidc.token_endpoint.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json");
    if let Some(secret) = &oidc.client_secret {
        // NOTE: RFC 6749 §2.3.1: the id and the secret are each
        // form-urlencoded before they are joined and encoded for Basic.
        let encode =
            |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let credentials = SecretString::from(STANDARD.encode(format!(
            "{}:{}",
            encode(&oidc.client_id),
            encode(secret.expose_secret())
        )));
        request = request.header(
            AUTHORIZATION,
            format!("Basic {}", credentials.expose_secret()),
        );
    }
    let answer = request
        .body(token_request(oidc, pending, code))
        .send()
        .await
        .map_err(|source| ExchangeError::Transport { source })?;
    let status = answer.status();
    if status != StatusCode::OK {
        return Err(ExchangeError::Refused { status });
    }
    let body = answer
        .bytes()
        .await
        .map_err(|source| ExchangeError::Transport { source })?;
    let tokens: TokenResponse =
        serde_json::from_slice(&body).map_err(|source| ExchangeError::Body { source })?;
    if !tokens.token_type.eq_ignore_ascii_case("bearer") {
        return Err(ExchangeError::TokenType);
    }
    let id_token = tokens.id_token.ok_or(ExchangeError::NoIdToken)?;
    verify_id_token(client, oidc, pending, &id_token).await?;
    Ok(SignedIn {
        access_token: SecretString::from(tokens.access_token),
        expires_in: tokens.expires_in.map(Duration::from_secs),
    })
}

/// The form of the token request (RFC 6749 §4.1.3, RFC 7636 §4.5), naming
/// the client in it only for a public client, which sends no secret.
fn token_request(oidc: &OidcSettings, pending: &PendingSignIn, code: &str) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", oidc.redirect_uri.as_str())
        .append_pair("code_verifier", pending.verifier().expose_secret());
    if oidc.client_secret.is_none() {
        form.append_pair("client_id", &oidc.client_id);
    }
    form.finish()
}

/// Checks `id_token` against the provider's keys and `pending`.
async fn verify_id_token(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    pending: &PendingSignIn,
    id_token: &str,
) -> Result<(), ExchangeError> {
    let header = jsonwebtoken::decode_header(id_token).map_err(|_unreadable| ExchangeError::Key)?;
    if !ALGORITHMS.contains(&header.alg) {
        return Err(ExchangeError::Key);
    }
    let keys = key_set(client, oidc).await?;
    let jwk = match &header.kid {
        Some(kid) => keys.find(kid),
        None => match keys.keys.as_slice() {
            [only] => Some(only),
            _ => None,
        },
    }
    .ok_or(ExchangeError::Key)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_unusable| ExchangeError::Key)?;
    let mut validation = Validation::new(header.alg);
    validation.algorithms = vec![header.alg];
    validation.leeway = LEEWAY.as_secs();
    validation.set_issuer(&[oidc.issuer.as_str()]);
    validation.set_audience(&[oidc.client_id.as_str()]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = jsonwebtoken::decode::<IdTokenClaims>(id_token, &key, &validation)
        .map_err(|source| ExchangeError::IdToken { source })?
        .claims;
    // NOTE: OpenID Connect Core 1.0 §3.1.3.7 item 6: an `azp` present names the
    // client the token was issued to, which must be this console.
    if claims
        .azp
        .as_deref()
        .is_some_and(|azp| azp != oidc.client_id)
    {
        return Err(ExchangeError::AuthorizedParty);
    }
    // NOTE: OpenID Connect Core 1.0 §3.1.3.7 item 11: the `nonce` must be the
    // one the authorization request sent, so a replayed ID Token is refused.
    let nonce = claims.nonce.ok_or(ExchangeError::Nonce)?;
    if aws_lc_rs::constant_time::verify_slices_are_equal(
        nonce.as_bytes(),
        pending.nonce().as_bytes(),
    )
    .is_err()
    {
        return Err(ExchangeError::Nonce);
    }
    Ok(())
}

/// Reads the provider's JWK Set.
async fn key_set(client: &reqwest::Client, oidc: &OidcSettings) -> Result<JwkSet, ExchangeError> {
    let answer = client
        .get(oidc.jwks_uri.clone())
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_unreachable| ExchangeError::KeySet)?;
    if answer.status() != StatusCode::OK {
        return Err(ExchangeError::KeySet);
    }
    let body = answer
        .bytes()
        .await
        .map_err(|_unreadable| ExchangeError::KeySet)?;
    serde_json::from_slice(&body).map_err(|_malformed| ExchangeError::KeySet)
}
