// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One token request to a node's OAuth 2.0 token endpoint, and its answer.
//!
//! The request is the client-credentials grant (RFC 6749 §4.4.2) or token
//! exchange (RFC 8693 §2.1), authenticated by a signed JWT client assertion
//! (RFC 7523 §2.2), and the answer a token (RFC 6749 §5.1, RFC 8693 §2.2.1)
//! or a typed refusal (RFC 6749 §5.2).
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
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::dispatch::reported::MESSAGE_LIMIT;
use crate::onward::Grant;
use crate::onward::keys::{ALGORITHM, SigningKey};

/// The `client_assertion_type` of a JWT client assertion (RFC 7523 §2.2).
pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// The `grant_type` of the client-credentials grant (RFC 6749 §4.4.2).
pub const GRANT_TYPE: &str = "client_credentials";

/// The `grant_type` of token exchange (RFC 8693 §2.1).
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The token type identifier of an access token (RFC 8693 §3), the
/// `subject_token_type` of the caller's token and the `issued_token_type`
/// the gateway accepts.
pub const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

/// The token type identifier of a JWT (RFC 8693 §3), the `actor_token_type`
/// of the gateway's assertion.
pub const JWT_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:jwt";

/// A token the endpoint issued: the access token, ready to send as a
/// credential, and its lifetime when the endpoint stated one.
///
/// A `DPoP`-bound token is held as a bearer credential too: the
/// [`DpopTransport`](crate::onward::dpop::DpopTransport) of its endpoint
/// sends it under the `DPoP` scheme with a proof (RFC 9449 §7.1).
#[derive(Debug)]
pub struct Issued {
    /// The access token as the `Authorization` credential.
    pub credentials: Credentials,
    /// The lifetime `expires_in` stated (RFC 6749 §5.1), or `None`.
    pub expires_in: Option<Duration>,
}

/// A token request that produced no token.
///
/// No variant carries an assertion or a token. The text a token endpoint
/// wrote is cut to [`MESSAGE_LIMIT`] characters.
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
    /// The token endpoint issued a token of a type other than the grant
    /// asks for: `Bearer` (RFC 6750), or `DPoP` for a grant whose tokens
    /// are `DPoP`-bound (RFC 9449 §5).
    #[error("the token endpoint issued a token of type {token_type}, not {expected}")]
    TokenType {
        /// The `token_type` it named.
        token_type: String,
        /// The type the grant asks for.
        expected: &'static str,
    },
    /// A token exchange answered with an `issued_token_type` other than an
    /// access token, or with none (RFC 8693 §2.2.1).
    #[error(
        "the token exchange issued {}, not an access token (RFC 8693 §2.2.1)",
        issued_token_type.as_deref().unwrap_or("no issued_token_type")
    )]
    IssuedTokenType {
        /// The `issued_token_type` it named, when it named one.
        issued_token_type: Option<String>,
    },
    /// A token exchange would ask for no scope, which lets the authorization
    /// server choose one, so it was not sent (RFC 8693 §2.1).
    #[error("the token exchange asks for no scope, so it was not sent")]
    Unscoped,
    /// A token exchange would name no resource, so it was not sent (RFC 8707
    /// §2).
    #[error("the token exchange names no resource, so it was not sent")]
    Untargeted,
    /// The issued access token cannot be sent as a credential.
    #[error("the issued access token cannot be sent as a credential")]
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

/// The caller a token exchange asks a token for (RFC 8693 §2.1).
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    /// The caller's verified access token, the `subject_token`.
    pub token: &'a SecretString,
    /// The scope the token is asked for: the caller's granted scopes that
    /// cover the operation (RFC 8693 §2.1 `scope`), never empty.
    pub scope: &'a str,
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

/// An RFC 6749 §5.1 token response, the members the gateway reads, with the
/// `issued_token_type` of RFC 8693 §2.2.1.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    issued_token_type: Option<String>,
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
/// clock. The same form is the `actor_token` of a token exchange, signed
/// separately so its `jti` is its own.
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

/// Requests a token for `grant` with the client-credentials grant over
/// `transport`, authenticating with `assertion`, and waits at most
/// `timeout` for the answer (RFC 6749 §4.4.2).
///
/// # Errors
///
/// Returns a [`TokenError`] for every request that produced no usable
/// token: one that could not be composed or sent, an RFC 6749 §5.2 refusal,
/// another status, a body that is no token response, and a token of
/// another type than the grant asks for or that cannot be sent.
pub async fn request<T: Transport>(
    grant: &Grant,
    assertion: &str,
    transport: &T,
    timeout: Duration,
) -> Result<Issued, TokenError> {
    let token = send(
        grant,
        client_credentials_form(grant, assertion),
        transport,
        timeout,
    )
    .await?;
    issued(grant, token)
}

/// Exchanges `subject`'s token for a token of `grant`'s node over
/// `transport`, authenticating with `assertion`, naming the gateway as the
/// actor with `actor`, and waits at most `timeout` (RFC 8693 §2.1).
///
/// The request always asks for `subject`'s scope and names the node with
/// `resource` (RFC 8707 §2), and with `audience` where the grant has one.
/// An exchange with no scope or no resource is never sent: without `scope`
/// the authorization server may issue the caller's whole grant (RFC 8693
/// §2.1). The answer must issue an access token (RFC 8693 §2.2.1).
///
/// # Errors
///
/// Returns [`TokenError::Unscoped`] for a subject with no scope and
/// [`TokenError::Untargeted`] for a grant with no resource, both with
/// nothing sent, a [`TokenError`] as [`request`] does, and
/// [`TokenError::IssuedTokenType`] for an answer that issues anything else
/// than an access token.
pub async fn exchange<T: Transport>(
    grant: &Grant,
    subject: Subject<'_>,
    (assertion, actor): (&str, &str),
    transport: &T,
    timeout: Duration,
) -> Result<Issued, TokenError> {
    if subject.scope.split_whitespace().next().is_none() {
        return Err(TokenError::Unscoped);
    }
    if grant.resource().is_none() {
        return Err(TokenError::Untargeted);
    }
    let form = exchange_form(grant, subject, (assertion, actor));
    let token = send(grant, form, transport, timeout).await?;
    if token.issued_token_type.as_deref() != Some(ACCESS_TOKEN_TYPE) {
        return Err(TokenError::IssuedTokenType {
            issued_token_type: token.issued_token_type.as_deref().map(bounded),
        });
    }
    issued(grant, token)
}

/// The body of a client-credentials request for `grant`, authenticated by
/// `assertion` (RFC 6749 §4.4.2, RFC 7523 §2.2).
fn client_credentials_form(grant: &Grant, assertion: &str) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", GRANT_TYPE);
    authenticated(&mut form, assertion);
    form.append_pair("scope", grant.scope().as_str());
    targeted(&mut form, grant);
    form.finish()
}

/// The body of a token exchange for `grant` of `subject`'s token,
/// authenticated by `assertion` and naming the gateway with `actor` (RFC
/// 8693 §2.1).
fn exchange_form(grant: &Grant, subject: Subject<'_>, (assertion, actor): (&str, &str)) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", TOKEN_EXCHANGE);
    authenticated(&mut form, assertion);
    form.append_pair("subject_token", subject.token.expose_secret())
        .append_pair("subject_token_type", ACCESS_TOKEN_TYPE)
        .append_pair("actor_token", actor)
        .append_pair("actor_token_type", JWT_TOKEN_TYPE)
        .append_pair("requested_token_type", ACCESS_TOKEN_TYPE);
    form.append_pair("scope", subject.scope);
    targeted(&mut form, grant);
    form.finish()
}

/// Adds the client authentication of RFC 7523 §2.2 to `form`.
fn authenticated(form: &mut form_urlencoded::Serializer<'_, String>, assertion: &str) {
    form.append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE)
        .append_pair("client_assertion", assertion);
}

/// Adds the grant's `resource` (RFC 8707 §2) and `audience` to `form`.
fn targeted(form: &mut form_urlencoded::Serializer<'_, String>, grant: &Grant) {
    if let Some(resource) = grant.resource() {
        form.append_pair("resource", resource.as_str());
    }
    if let Some(audience) = grant.audience() {
        form.append_pair("audience", audience);
    }
}

/// Posts the `application/x-www-form-urlencoded` `body` to `grant`'s token
/// endpoint and reads the token response.
async fn send<T: Transport>(
    grant: &Grant,
    body: String,
    transport: &T,
    timeout: Duration,
) -> Result<TokenResponse, TokenError> {
    let mut built = http::Request::builder()
        .method(Method::POST)
        .uri(grant.token_endpoint().as_str())
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        )
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .body(body.into_bytes())
        .map_err(TokenError::Compose)?;
    built.extensions_mut().insert(RequestTimeout(timeout));
    let answer = transport.send(built).await.map_err(|error| match error {
        TransportError::Timeout { .. } => TokenError::TimeOut(error),
        TransportError::Send { .. } => TokenError::Unreachable(error),
    })?;
    let status = answer.status();
    let body = answer.body();
    if status == StatusCode::OK {
        return serde_json::from_slice(body).map_err(TokenError::Body);
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

/// The credential `token` carries, refused when it is of another type than
/// `grant` asks for or cannot be sent.
fn issued(grant: &Grant, token: TokenResponse) -> Result<Issued, TokenError> {
    // NOTE: RFC 6749 §5.1 makes token_type case-insensitive; RFC 9449 §5 names a
    // DPoP-bound token DPoP, and a grant that asked for one takes no other.
    let expected = if grant.dpop().is_some() {
        "DPoP"
    } else {
        "Bearer"
    };
    if !token.token_type.eq_ignore_ascii_case(expected) {
        return Err(TokenError::TokenType {
            token_type: bounded(&token.token_type),
            expected,
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
