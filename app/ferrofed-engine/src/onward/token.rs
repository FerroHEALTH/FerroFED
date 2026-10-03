// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One token request to a node's OAuth 2.0 token endpoint, and its answer.
//!
//! The request is the client-credentials grant (RFC 6749 §4.4.2)
//! authenticated by a signed JWT client assertion (RFC 7523 §2.2), and the
//! answer a token (RFC 6749 §5.1) or a typed refusal (§5.2).
//!
//! The request is composed here and sent through the same HTTP engine as
//! the node requests. The token endpoint is no ITS-REST resource, so no
//! `openehr-its` operation describes it.

use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};
use jsonwebtoken::Header;
use openehr_its::rest::client::{
    Credentials, InvalidCredentials, RequestTimeout, Transport, TransportError,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::dispatch::reported::MESSAGE_LIMIT;
use crate::onward::Grant;
use crate::onward::keys::{ALGORITHM, SigningKey};

/// The `client_assertion_type` of a JWT client assertion (RFC 7523 §2.2).
pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// The `grant_type` of the client-credentials grant (RFC 6749 §4.4.2).
pub const GRANT_TYPE: &str = "client_credentials";

/// A token the endpoint issued: the access token, ready to send as a
/// bearer credential, and its lifetime when the endpoint stated one.
#[derive(Debug)]
pub struct Issued {
    /// The access token as the `Authorization` credential.
    pub credentials: Credentials,
    /// The lifetime `expires_in` stated (RFC 6749 §5.1), or `None`.
    pub expires_in: Option<Duration>,
}

/// A token request that produced no token.
///
/// No variant carries the client assertion or a token. The text a token
/// endpoint wrote is cut to [`MESSAGE_LIMIT`] characters.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TokenError {
    /// The client assertion could not be signed.
    #[error("the client assertion could not be signed")]
    Sign(#[source] jsonwebtoken::errors::Error),
    /// The token request could not be composed.
    #[error("the token request could not be composed")]
    Compose(#[source] http::Error),
    /// The token endpoint did not answer before the timeout.
    #[error("the token endpoint did not answer in time")]
    TimeOut(#[source] TransportError),
    /// The token endpoint could not be reached.
    #[error("the token endpoint could not be reached")]
    Unreachable(#[source] TransportError),
    /// The token endpoint refused the request with an RFC 6749 §5.2 error.
    #[error(
        "the token endpoint refused the request with {status}: {error}{}",
        description.as_deref().map(|text| format!(" ({text})")).unwrap_or_default()
    )]
    Refused {
        /// The status it answered.
        status: StatusCode,
        /// The `error` code (RFC 6749 §5.2).
        error: String,
        /// The `error_description`, when it sent one.
        description: Option<String>,
    },
    /// The token endpoint answered with a status that is neither a token
    /// nor an RFC 6749 §5.2 error.
    #[error("the token endpoint answered {status}")]
    Status {
        /// The status it answered.
        status: StatusCode,
    },
    /// The token endpoint answered `200` with a body that is not an RFC
    /// 6749 §5.1 token response.
    #[error("the token endpoint answered 200 with a body that is not an RFC 6749 §5.1 token")]
    Body(#[source] serde_json::Error),
    /// The token endpoint issued a token of a type other than `Bearer`
    /// (RFC 6750).
    #[error("the token endpoint issued a token of type {token_type}, not Bearer (RFC 6750)")]
    TokenType {
        /// The `token_type` it named.
        token_type: String,
    },
    /// The issued access token cannot be sent as a bearer credential.
    #[error("the issued access token cannot be sent as a bearer credential")]
    Token(#[source] InvalidCredentials),
}

/// An `error` code RFC 6749 §5.2 registers for a token endpoint's refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// `invalid_request`.
    InvalidRequest,
    /// `invalid_client`.
    InvalidClient,
    /// `invalid_grant`.
    InvalidGrant,
    /// `unauthorized_client`.
    UnauthorizedClient,
    /// `unsupported_grant_type`.
    UnsupportedGrantType,
    /// `invalid_scope`.
    InvalidScope,
}

impl ErrorCode {
    /// The code `text` names, or `None` for a code RFC 6749 §5.2 does not
    /// register.
    #[must_use]
    pub fn registered(text: &str) -> Option<Self> {
        match text {
            "invalid_request" => Some(Self::InvalidRequest),
            "invalid_client" => Some(Self::InvalidClient),
            "invalid_grant" => Some(Self::InvalidGrant),
            "unauthorized_client" => Some(Self::UnauthorizedClient),
            "unsupported_grant_type" => Some(Self::UnsupportedGrantType),
            "invalid_scope" => Some(Self::InvalidScope),
            _ => None,
        }
    }

    /// The code as RFC 6749 §5.2 writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidClient => "invalid_client",
            Self::InvalidGrant => "invalid_grant",
            Self::UnauthorizedClient => "unauthorized_client",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::InvalidScope => "invalid_scope",
        }
    }
}

impl TokenError {
    /// The registered RFC 6749 §5.2 `error` code the token endpoint refused
    /// with, when it refused with one.
    #[must_use]
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Refused { error, .. } => ErrorCode::registered(error),
            _ => None,
        }
    }
}

/// The claims of a client assertion (RFC 7523 §3).
#[derive(Debug, Serialize)]
struct Claims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
    jti: String,
}

/// An RFC 6749 §5.1 token response, the members the gateway reads.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
}

/// An RFC 6749 §5.2 error response, the members the gateway reads.
#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
    error_description: Option<String>,
}

/// The client assertion for `grant`, signed with `key`, valid for
/// `lifetime` from now (RFC 7523 §3).
///
/// `iss` and `sub` are the `client_id`, `aud` the token endpoint, `jti` a
/// fresh version 4 UUID, and `iat` and `exp` whole seconds of the wall
/// clock.
///
/// # Errors
///
/// Returns [`TokenError::Sign`] when the key cannot sign.
pub fn assertion(
    grant: &Grant,
    key: &SigningKey,
    lifetime: Duration,
) -> Result<String, TokenError> {
    let iat = jiff::Timestamp::now().as_second();
    let lifetime = i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX);
    let claims = Claims {
        iss: grant.client_id(),
        sub: grant.client_id(),
        aud: grant.token_endpoint().as_str(),
        exp: iat.saturating_add(lifetime),
        iat,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    let mut header = Header::new(ALGORITHM);
    header.kid = Some(key.kid().to_owned());
    jsonwebtoken::encode(&header, &claims, key.private()).map_err(TokenError::Sign)
}

/// Requests a token for `grant` over `transport`, authenticating with
/// `assertion`, and waits at most `timeout` for the answer.
///
/// # Errors
///
/// Returns a [`TokenError`] for every request that produced no bearer
/// token: one that could not be composed or sent, an RFC 6749 §5.2 refusal,
/// another status, a body that is no token response, and a token that is
/// not a bearer token or cannot be sent as one.
pub async fn request<T: Transport>(
    grant: &Grant,
    assertion: &str,
    transport: &T,
    timeout: Duration,
) -> Result<Issued, TokenError> {
    let mut built = http::Request::builder()
        .method(Method::POST)
        .uri(grant.token_endpoint().as_str())
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        )
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .body(form(grant, assertion).into_bytes())
        .map_err(TokenError::Compose)?;
    built.extensions_mut().insert(RequestTimeout(timeout));
    let answer = transport.send(built).await.map_err(|error| match error {
        TransportError::Timeout { .. } => TokenError::TimeOut(error),
        TransportError::Send { .. } => TokenError::Unreachable(error),
    })?;
    let status = answer.status();
    let body = answer.body();
    if status == StatusCode::OK {
        let token: TokenResponse = serde_json::from_slice(body).map_err(TokenError::Body)?;
        return issued(token);
    }
    // NOTE: RFC 6749 §5.2 answers 400, or 401 for a failed client authentication,
    // with a JSON error; any other body is answered as its bare status.
    if (status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED)
        && let Ok(refused) = serde_json::from_slice::<ErrorResponse>(body)
    {
        return Err(TokenError::Refused {
            status,
            error: bounded(&refused.error),
            description: refused.error_description.as_deref().map(bounded),
        });
    }
    Err(TokenError::Status { status })
}

/// The `application/x-www-form-urlencoded` body of a token request for
/// `grant`, authenticated by `assertion` (RFC 6749 §4.4.2, RFC 7523 §2.2).
fn form(grant: &Grant, assertion: &str) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", GRANT_TYPE)
        .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE)
        .append_pair("client_assertion", assertion)
        .append_pair("scope", grant.scope().as_str());
    if let Some(resource) = grant.resource() {
        form.append_pair("resource", resource.as_str());
    }
    if let Some(audience) = grant.audience() {
        form.append_pair("audience", audience);
    }
    form.finish()
}

/// The bearer credential `token` carries, refused when it is of another
/// type or cannot be sent.
fn issued(token: TokenResponse) -> Result<Issued, TokenError> {
    // NOTE: RFC 6749 §5.1 makes token_type case-insensitive, and RFC 6750 is
    // the one type the node client sends.
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err(TokenError::TokenType {
            token_type: bounded(&token.token_type),
        });
    }
    let credentials = Credentials::bearer(SecretString::from(token.access_token));
    credentials.header_value().map_err(TokenError::Token)?;
    Ok(Issued {
        credentials,
        expires_in: token.expires_in.map(Duration::from_secs),
    })
}

/// At most [`MESSAGE_LIMIT`] characters of `text`.
fn bounded(text: &str) -> String {
    text.chars().take(MESSAGE_LIMIT).collect()
}
