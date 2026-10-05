// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One token request to an OAuth 2.0 token endpoint, a node's or an
//! identity service's, and its answer.
//!
//! The request is the client-credentials grant (RFC 6749 §4.4.2) or token
//! exchange (RFC 8693 §2.1), authenticated by a signed JWT client assertion
//! (RFC 7523 §2.2), by a client secret in the Basic scheme or the request
//! body (RFC 6749 §2.3.1), or by the TLS client certificate of the
//! connection, with the `client_id` in the request (RFC 8705 §2), with the
//! grant's `authorization_details` where it has some (RFC 9396 §6), and the
//! answer a token (RFC 6749 §5.1, RFC 8693 §2.2.1, RFC 9396 §7) or a typed
//! refusal (RFC 6749 §5.2). A grant whose tokens are certificate-bound takes
//! only a token whose stated binding names its certificate (RFC 8705 §3).
//!
//! The request is composed here and sent through the same HTTP engine as
//! the node requests. The token endpoint is no ITS-REST resource, so no
//! `openehr-its` operation describes it. A grant whose tokens are bound with
//! `DPoP` proves each token request with its [`Prover`], and answers a
//! demanded nonce by sending the request once more (RFC 9449 §8).

use std::time::{Duration, Instant};

use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};
use jsonwebtoken::Header;
use openehr_its::rest::client::{
    Credentials, InvalidCredentials, RequestTimeout, Transport, TransportError,
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use url::form_urlencoded;

use crate::dispatch::reported::MESSAGE_LIMIT;
use crate::onward::authorization_details::{self, AuthorizationDetailsError};
use crate::onward::dpop::{self, Prover, Role};
use crate::onward::keys::SigningKey;
use crate::onward::mtls::{self, Binding, Confirmation};
use crate::onward::{ClientAuthentication, Grant, SecretMethod};

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

/// How long before the end of its stated lifetime a cached token is
/// replaced.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(30);

/// The longest lifetime a client assertion may have.
// NOTE: no specification governs this bound: our own design, the five
// minutes SMART Backend Services sets for the same RFC 7523 assertion.
pub const MAX_ASSERTION_LIFETIME: Duration = Duration::from_secs(300);

/// A token the endpoint issued: the access token, ready to send as a
/// credential, and its lifetime when the endpoint stated one.
///
/// A `DPoP`-bound token is a `Credentials::Dpop`, which the node's client
/// sends under the `DPoP` scheme with a proof (RFC 9449 §7.1), and never
/// under `Bearer`.
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
    /// The request needs an assertion of the gateway, and the grant holds
    /// no key to sign one; nothing was sent.
    #[error("the token request needs an assertion of the gateway, and no key signs one")]
    Unsigned,
    /// The client id and secret of a grant that sends them in the Basic
    /// scheme form no `Authorization` value (RFC 6749 §2.3.1, RFC 7617 §2);
    /// nothing was sent, and the source names no part of the secret.
    #[error("the client id and secret cannot be sent in the Basic scheme")]
    ClientSecret(#[source] InvalidCredentials),
    /// The grant authenticates with a client secret and holds none, so
    /// nothing was sent (RFC 6749 §2.3.1).
    #[error("the token request needs the client secret, and the grant holds none")]
    NoSecret,
    /// The token request could not be composed.
    #[error("the token request could not be composed")]
    Compose(#[source] http::Error),
    /// The `DPoP` proof of the token request could not be signed (RFC 9449
    /// §4.2); nothing was sent.
    #[error("the DPoP proof of the token request could not be signed")]
    Proof(#[source] jsonwebtoken::errors::Error),
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
    /// The grant asked for `authorization_details`, and the token response
    /// states none it granted (RFC 9396 §7).
    #[error("the token response states no authorization_details, which RFC 9396 §7 requires")]
    AuthorizationDetails,
    /// The `authorization_details` the token response states it granted do
    /// not have the shape of RFC 9396 §2.
    #[error("the authorization_details the token response granted are not of RFC 9396 §2 shape")]
    GrantedDetails(#[source] AuthorizationDetailsError),
    /// The token is bound to another certificate than the one the grant's
    /// connections present, or confirmed by another method, so it was
    /// neither kept nor sent (RFC 8705 §3).
    #[error(
        "the issued token is not bound to the certificate the gateway presents (RFC 8705 §3), so it was not used"
    )]
    CertificateMismatch,
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
    /// `invalid_authorization_details`, which RFC 9396 §5 and §6 add for
    /// details the server refuses (§14.6).
    InvalidAuthorizationDetails,
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
            "invalid_authorization_details" => Some(Self::InvalidAuthorizationDetails),
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
            Self::InvalidAuthorizationDetails => "invalid_authorization_details",
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
/// `issued_token_type` of RFC 8693 §2.2.1 and a `cnf` in the shape RFC 8705
/// §3.2 gives an introspection response.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    issued_token_type: Option<String>,
    authorization_details: Option<Box<RawValue>>,
    cnf: Option<Confirmation>,
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
/// `iss` and `sub` are the `client_id`, `aud` the token endpoint or the
/// issuer identifier as [`Grant::assertion_aud`] writes it, always one
/// string and never an array (FAPI 2.0 Security Profile §5.3.3.1), `jti` a
/// fresh version 4 UUID, and `iat` and `exp` whole seconds of the wall
/// clock. The header names the key's algorithm and `kid`. The same form is
/// the `actor_token` of a token exchange, signed separately so its `jti` is
/// its own.
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
        aud: grant.assertion_aud(),
        exp: iat.saturating_add(lifetime),
        iat,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    let mut header = Header::new(key.algorithm());
    header.kid = Some(key.kid().to_owned());
    jsonwebtoken::encode(&header, &claims, key.private()).map_err(TokenError::Sign)
}

/// Requests a token for `grant` with the client-credentials grant (RFC
/// 6749 §4.4.2).
///
/// The request goes over `transport`, authenticated as the grant's
/// [`ClientAuthentication`] says: with an assertion signed with the key of
/// `signer` and valid for its lifetime, or by the TLS client certificate
/// `transport` presents, with the `client_id` (RFC 8705 §2). It waits at
/// most `timeout` for the answer. Every send carries an assertion of its
/// own, so the one more send that answers a demanded `DPoP` nonce never
/// repeats a `jti` (RFC 7523 §3).
///
/// # Errors
///
/// Returns a [`TokenError`] for every request that produced no usable
/// token: an assertion that could not be signed or has no key to sign it, a
/// secret the grant authenticates with and does not hold, a
/// request that could not be composed or sent, an RFC 6749 §5.2 refusal,
/// another status, a body that is no token response, a token of another
/// type than the grant asks for or that cannot be sent, and a token bound
/// to another certificate than the grant's.
pub async fn request<T: Transport>(
    grant: &Grant,
    signer: Option<(&SigningKey, Duration)>,
    transport: &T,
    timeout: Duration,
) -> Result<Issued, TokenError> {
    let compose = || -> Result<String, TokenError> {
        let client = client_assertion(grant, signer)?;
        Ok(client_credentials_form(grant, client.as_deref()))
    };
    let token = send(grant, compose, transport, timeout).await?;
    issued(grant, token)
}

/// Exchanges `subject`'s token for a token of `grant`'s node (RFC 8693
/// §2.1).
///
/// The request goes over `transport`, authenticated as [`request`] is,
/// names the gateway as the actor with an assertion signed with `key` and
/// valid for `lifetime`, and waits at most `timeout`. It always asks for
/// `subject`'s scope and names the node with
/// `resource` (RFC 8707 §2), and with `audience` where the grant has one.
/// An exchange with no scope or no resource is never sent: without `scope`
/// the authorization server may issue the caller's whole grant (RFC 8693
/// §2.1). The answer must issue an access token (RFC 8693 §2.2.1). Every
/// send carries a client assertion and an actor token of its own, each with
/// its own `jti` (RFC 7523 §3).
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
    (key, lifetime): (&SigningKey, Duration),
    transport: &T,
    timeout: Duration,
) -> Result<Issued, TokenError> {
    if subject.scope.split_whitespace().next().is_none() {
        return Err(TokenError::Unscoped);
    }
    if grant.resource().is_none() {
        return Err(TokenError::Untargeted);
    }
    let compose = || -> Result<String, TokenError> {
        let client = client_assertion(grant, Some((key, lifetime)))?;
        let actor = assertion(grant, key, lifetime)?;
        Ok(exchange_form(grant, subject, (client.as_deref(), &actor)))
    };
    let token = send(grant, compose, transport, timeout).await?;
    if token.issued_token_type.as_deref() != Some(ACCESS_TOKEN_TYPE) {
        return Err(TokenError::IssuedTokenType {
            issued_token_type: token.issued_token_type.as_deref().map(bounded),
        });
    }
    issued(grant, token)
}

/// The client assertion `grant`'s client authentication sends, signed with
/// the key of `signer`, or `None` for a grant authenticated by its TLS
/// client certificate (RFC 8705 §2) or its client secret (RFC 6749 §2.3.1).
fn client_assertion(
    grant: &Grant,
    signer: Option<(&SigningKey, Duration)>,
) -> Result<Option<String>, TokenError> {
    match grant.client_authentication() {
        ClientAuthentication::Tls(_) => Ok(None),
        // NOTE: RFC 6749 §2.3.1, a client authenticated by a secret sends it, so a
        // grant that names the method and holds no secret sends nothing at all.
        ClientAuthentication::ClientSecret(_) => grant
            .client_secret()
            .map(|_| None)
            .ok_or(TokenError::NoSecret),
        ClientAuthentication::PrivateKeyJwt => {
            let (key, lifetime) = signer.ok_or(TokenError::Unsigned)?;
            assertion(grant, key, lifetime).map(Some)
        }
    }
}

/// The body of a client-credentials request for `grant`, authenticated by
/// `assertion`, or by the connection's certificate when there is none (RFC
/// 6749 §4.4.2, RFC 7523 §2.2, RFC 8705 §2).
fn client_credentials_form(grant: &Grant, assertion: Option<&str>) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", GRANT_TYPE);
    authenticated(&mut form, grant, assertion);
    if let Some(scope) = grant.scope() {
        form.append_pair("scope", scope.as_str());
    }
    targeted(&mut form, grant);
    form.finish()
}

/// The body of a token exchange for `grant` of `subject`'s token,
/// authenticated as [`client_credentials_form`] is and naming the gateway
/// with `actor` (RFC 8693 §2.1).
fn exchange_form(
    grant: &Grant,
    subject: Subject<'_>,
    (assertion, actor): (Option<&str>, &str),
) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", TOKEN_EXCHANGE);
    authenticated(&mut form, grant, assertion);
    form.append_pair("subject_token", subject.token.expose_secret())
        .append_pair("subject_token_type", ACCESS_TOKEN_TYPE)
        .append_pair("actor_token", actor)
        .append_pair("actor_token_type", JWT_TOKEN_TYPE)
        .append_pair("requested_token_type", ACCESS_TOKEN_TYPE);
    form.append_pair("scope", subject.scope);
    targeted(&mut form, grant);
    form.finish()
}

/// Adds the client authentication to `form`: the assertion of RFC 7523
/// §2.2, the `client_id` and `client_secret` of a client that posts its
/// secret (RFC 6749 §2.3.1), nothing for a client that sends its secret in
/// the Basic scheme ([`post`] adds it), or the `client_id` alone that RFC
/// 8705 §2 requires of a client the TLS handshake authenticates.
fn authenticated(
    form: &mut form_urlencoded::Serializer<'_, String>,
    grant: &Grant,
    assertion: Option<&str>,
) {
    if let Some(assertion) = assertion {
        form.append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE)
            .append_pair("client_assertion", assertion);
        return;
    }
    match (grant.client_authentication(), grant.client_secret()) {
        (ClientAuthentication::ClientSecret(SecretMethod::Basic), _) => {}
        (ClientAuthentication::ClientSecret(SecretMethod::Post), Some(secret)) => {
            form.append_pair("client_id", grant.client_id())
                .append_pair("client_secret", secret.expose_secret());
        }
        _ => {
            form.append_pair("client_id", grant.client_id());
        }
    }
}

/// The `Authorization` value of a grant that sends its client secret in the
/// HTTP Basic scheme, the client id and the secret each form-urlencoded
/// first (RFC 6749 §2.3.1), or `None` for any other grant.
fn basic_authorization(grant: &Grant) -> Result<Option<HeaderValue>, TokenError> {
    let (ClientAuthentication::ClientSecret(SecretMethod::Basic), Some(secret)) =
        (grant.client_authentication(), grant.client_secret())
    else {
        return Ok(None);
    };
    let encoded = |text: &str| form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
    let password = SecretString::from(encoded(secret.expose_secret()));
    let mut value = Credentials::basic(encoded(grant.client_id()), password)
        .header_value()
        .map_err(TokenError::ClientSecret)?;
    value.set_sensitive(true);
    Ok(Some(value))
}

/// Adds the grant's `resource` (RFC 8707 §2), `audience`, and
/// `authorization_details` (RFC 9396 §6) to `form`.
fn targeted(form: &mut form_urlencoded::Serializer<'_, String>, grant: &Grant) {
    if let Some(resource) = grant.resource() {
        form.append_pair("resource", resource.as_str());
    }
    if let Some(audience) = grant.audience() {
        form.append_pair("audience", audience);
    }
    if let Some(details) = grant.authorization_details() {
        form.append_pair("authorization_details", details.as_str());
    }
}

/// Posts the `application/x-www-form-urlencoded` body `compose` writes to
/// `grant`'s token endpoint and reads the token response.
///
/// Under a `DPoP` grant each send carries a proof, a nonce the endpoint
/// sends is kept for the next proof, and a `400` [`dpop::USE_NONCE`] that
/// carries one is answered by sending the request once more, within the
/// time `timeout` left (RFC 9449 §8). Each send has a body `compose` writes
/// anew, so its assertions are signed anew (RFC 7523 §3).
async fn send<T: Transport>(
    grant: &Grant,
    compose: impl Fn() -> Result<String, TokenError>,
    transport: &T,
    timeout: Duration,
) -> Result<TokenResponse, TokenError> {
    let started = Instant::now();
    let mut resend = grant.dpop().is_some();
    let answer = loop {
        let body = compose()?.into_bytes();
        let left = timeout.saturating_sub(started.elapsed());
        let answer = post(grant, &body, transport, left).await?;
        let Some((prover, nonce)) = grant.dpop().zip(nonce_of(&answer)) else {
            break answer;
        };
        prover.remember(Role::Authorization, grant.token_endpoint(), &nonce);
        if !(resend && demands_nonce(&answer)) {
            break answer;
        }
        resend = false;
    };
    read(&answer)
}

/// Sends `body` to `grant`'s token endpoint once, with a `DPoP` proof under
/// a `DPoP` grant, waiting at most `timeout`.
async fn post<T: Transport>(
    grant: &Grant,
    body: &[u8],
    transport: &T,
    timeout: Duration,
) -> Result<http::Response<Vec<u8>>, TokenError> {
    let mut builder = http::Request::builder()
        .method(Method::POST)
        .uri(grant.token_endpoint().as_str())
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        )
        .header(ACCEPT, HeaderValue::from_static("application/json"));
    if let Some(prover) = grant.dpop() {
        builder = builder.header(dpop::HEADER, proof(prover, grant)?);
    }
    if let Some(basic) = basic_authorization(grant)? {
        builder = builder.header(AUTHORIZATION, basic);
    }
    let mut built = builder.body(body.to_vec()).map_err(TokenError::Compose)?;
    built.extensions_mut().insert(RequestTimeout(timeout));
    transport.send(built).await.map_err(|error| match error {
        TransportError::Timeout { .. } => TokenError::TimeOut(error),
        TransportError::Send { .. } => TokenError::Unreachable(error),
    })
}

/// The `DPoP` proof of a token request to `grant`'s token endpoint, which
/// carries no access token, so no `ath` (RFC 9449 §4.2).
fn proof(prover: &Prover, grant: &Grant) -> Result<String, TokenError> {
    prover
        .prove(
            (&Method::POST, grant.token_endpoint()),
            Role::Authorization,
            None,
        )
        .map_err(TokenError::Proof)
}

/// The nonce `answer` carries in [`dpop::NONCE_HEADER`], when it carries
/// one.
fn nonce_of(answer: &http::Response<Vec<u8>>) -> Option<String> {
    // NOTE: RFC 9449 §8, a nonce is visible ASCII; one that is not is
    // legitimately unusable, and the endpoint is answered as if it sent none.
    answer
        .headers()
        .get(dpop::NONCE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Whether `answer` is the token endpoint's demand for a nonce: a `400`
/// whose RFC 6749 §5.2 error is [`dpop::USE_NONCE`] (RFC 9449 §8).
fn demands_nonce(answer: &http::Response<Vec<u8>>) -> bool {
    answer.status() == StatusCode::BAD_REQUEST
        && serde_json::from_slice::<ErrorResponse>(answer.body())
            .is_ok_and(|refused| refused.error == dpop::USE_NONCE)
}

/// The token response `answer` carries, or the refusal it states.
fn read(answer: &http::Response<Vec<u8>>) -> Result<TokenResponse, TokenError> {
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
    // NOTE: RFC 9396 §7, the server MUST return the authorization_details it
    // granted, so an answer to a grant that asked for some and states none is refused.
    if grant.authorization_details().is_some() {
        let granted = token
            .authorization_details
            .as_deref()
            .ok_or(TokenError::AuthorizationDetails)?;
        authorization_details::types_of(granted.get()).map_err(TokenError::GrantedDetails)?;
    }
    // NOTE: RFC 8705 §3, a certificate-bound token travels only over a connection
    // presenting that certificate, so one whose stated binding names another is refused.
    if let Some(expected) = grant.certificate()
        && mtls::binding(&token.access_token, token.cnf.as_ref(), expected) == Binding::Mismatch
    {
        return Err(TokenError::CertificateMismatch);
    }
    let access = SecretString::from(token.access_token);
    let credentials = if grant.dpop().is_some() {
        Credentials::dpop(access)
    } else {
        Credentials::bearer(access)
    };
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
