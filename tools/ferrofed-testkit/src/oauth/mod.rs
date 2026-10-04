// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness OAuth 2.0 token endpoint, a test device in place of a node's
//! authorization server (Federation Tier with AQL §13.1, N25, CP-17).
//!
//! It authenticates the gateway by its signed JWT client assertion against
//! the JWK Set the gateway publishes.
//!
//! **This is a test device, not an authorization server.** It answers
//! `POST /oauth2/token` with the client-credentials grant of RFC 6749
//! §4.4.2, authenticated by an RFC 7523 §2.2 client assertion. It verifies
//! the assertion against the JWK Set a test hands it ([`TokenEndpoint::trust`],
//! normally the one the gateway serves): the ES384 signature by the key its
//! `kid` names, `iss` and `sub` equal to the client id, `aud` equal to its own
//! token URL, an `exp` no more than 300 seconds after `iat`, and a `jti` it
//! has not seen before. It issues a random access token and answers anything
//! else with an RFC 6749 §5.2 error. [`TokenEndpoint::bearer`] is the wiremock
//! matcher a mock node requires the issued token with.
//!
//! Once a test calls [`TokenEndpoint::accept_exchange`], it also answers RFC
//! 8693 token exchange: the client assertion as above, the `actor_token`
//! verified the same way, and the `subject_token` verified as an access
//! token of the trusted caller issuer, by its signature, `iss` and `exp`. It
//! records each exchange's subject, scope and resource, and
//! [`TokenEndpoint::bearer_for`] matches a token issued for one subject.
//! Once a test calls [`TokenEndpoint::require_dpop`], every token request
//! must carry a `DPoP` proof ([`crate::dpop::verify`]); the token issued is
//! bound to the proof's key, typed `DPoP`, and [`TokenEndpoint::dpop_bound`]
//! is the matcher of a node that requires it with a proof of that key.
//!
//! [`es384_pem`] and [`p256_pem`] generate a synthetic private key in PKCS#8
//! PEM at run time, so no key is ever committed. No specification governs the
//! device: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P384_SHA384_FIXED_SIGNING, EcdsaKeyPair,
    EcdsaSigningAlgorithm,
};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, Request};

use crate::dpop;
use crate::mock::Server;
use crate::oauth::respond::Responder;

mod respond;

/// The path the token endpoint answers on.
pub const TOKEN_PATH: &str = "/oauth2/token";

/// The `client_assertion_type` of a JWT client assertion (RFC 7523 §2.2).
pub const JWT_BEARER: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// The `grant_type` of token exchange (RFC 8693 §2.1).
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The token type identifier of an access token (RFC 8693 §3).
pub const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

/// The token type identifier of a JWT (RFC 8693 §3).
pub const JWT_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:jwt";

/// The longest an assertion may live, `exp` minus `iat`, in seconds.
pub const MAX_LIFETIME_S: i64 = 300;

/// A key could not be generated.
#[derive(Debug, thiserror::Error)]
#[error("a synthetic signing key could not be generated")]
pub struct KeyGenError;

/// A fresh ES384 (P-384) private key in PKCS#8 PEM.
///
/// # Errors
///
/// Returns [`KeyGenError`] when the crypto library cannot generate one.
pub fn es384_pem() -> Result<String, KeyGenError> {
    generated(&ECDSA_P384_SHA384_FIXED_SIGNING)
}

/// A fresh P-256 private key in PKCS#8 PEM, a key ES384 cannot sign with.
///
/// # Errors
///
/// Returns [`KeyGenError`] when the crypto library cannot generate one.
pub fn p256_pem() -> Result<String, KeyGenError> {
    generated(&ECDSA_P256_SHA256_FIXED_SIGNING)
}

fn generated(algorithm: &'static EcdsaSigningAlgorithm) -> Result<String, KeyGenError> {
    let document = EcdsaKeyPair::generate_pkcs8(algorithm, &SystemRandom::new())
        .map_err(|_unspecified| KeyGenError)?;
    Ok(pem::encode(&pem::Pem::new(
        "PRIVATE KEY",
        document.as_ref().to_vec(),
    )))
}

/// The claims a verified assertion carried.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssertionClaims {
    /// The issuer, the client id.
    pub iss: String,
    /// The subject, the client id.
    pub sub: String,
    /// The audience, the token URL.
    pub aud: String,
    /// The expiry, in seconds since the epoch.
    pub exp: i64,
    /// The issue time, in seconds since the epoch.
    pub iat: i64,
    /// The assertion's unique id.
    pub jti: String,
}

/// Verifies `assertion` against `jwks` as a client assertion of `client_id`
/// for the token endpoint `audience` (RFC 7523 §3).
///
/// # Errors
///
/// Returns the reason the assertion is refused: its header names no ES384
/// key of `jwks`, the signature does not verify, or a claim is missing or
/// wrong.
pub fn verify(
    assertion: &str,
    jwks: &JwkSet,
    client_id: &str,
    audience: &str,
) -> Result<AssertionClaims, String> {
    let header =
        jsonwebtoken::decode_header(assertion).map_err(|error| format!("header: {error}"))?;
    if header.alg != Algorithm::ES384 {
        return Err(format!("the assertion is signed with {:?}", header.alg));
    }
    let kid = header.kid.ok_or("the header names no kid")?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| format!("no published key has kid {kid}"))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|error| format!("jwk: {error}"))?;
    let mut validation = Validation::new(Algorithm::ES384);
    validation.set_audience(&[audience]);
    validation.set_issuer(&[client_id]);
    validation.sub = Some(client_id.to_owned());
    validation.set_required_spec_claims(&["exp", "iss", "sub", "aud"]);
    let claims = jsonwebtoken::decode::<AssertionClaims>(assertion, &key, &validation)
        .map_err(|error| format!("verification: {error}"))?
        .claims;
    let lifetime = claims.exp.saturating_sub(claims.iat);
    if lifetime <= 0 || lifetime > MAX_LIFETIME_S {
        return Err(format!("the assertion lives {lifetime} s"));
    }
    if claims.jti.is_empty() {
        return Err("the jti is empty".to_owned());
    }
    Ok(claims)
}

/// The claims of a caller's access token the endpoint reads.
#[derive(Debug, Deserialize)]
struct SubjectClaims {
    sub: String,
}

/// Verifies `token` as an access token `issuer` signed with a key of
/// `jwks`, by its signature, `iss` and `exp`, and returns its `sub`.
fn verify_subject(token: &str, jwks: &JwkSet, issuer: &str) -> Result<String, String> {
    let header = jsonwebtoken::decode_header(token).map_err(|error| format!("header: {error}"))?;
    let kid = header.kid.ok_or("the subject token names no kid")?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| format!("no trusted caller key has kid {kid}"))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|error| format!("jwk: {error}"))?;
    let mut validation = Validation::new(header.alg);
    validation.validate_aud = false;
    validation.set_issuer(&[issuer]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    jsonwebtoken::decode::<SubjectClaims>(token, &key, &validation)
        .map(|data| data.claims.sub)
        .map_err(|error| format!("subject token: {error}"))
}

/// What the endpoint made of one token request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A token was issued.
    Issued,
    /// The request was refused, for this reason.
    Refused(String),
}

/// One token exchange the endpoint answered with a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchanged {
    /// The `sub` of the subject token, the caller.
    pub subject: String,
    /// The `scope` asked for, when one was.
    pub scope: Option<String>,
    /// The `resource` named, when one was.
    pub resource: Option<String>,
}

/// A refusal the endpoint answers every request with, set by
/// [`TokenEndpoint::refuse`].
#[derive(Debug, Clone)]
struct Refusal {
    status: u16,
    error: String,
    description: String,
}

/// The caller issuer whose tokens the endpoint exchanges.
#[derive(Debug, Clone)]
struct Callers {
    jwks: JwkSet,
    issuer: String,
}

#[derive(Debug, Default)]
struct State {
    jwks: JwkSet,
    expires_in: Option<u64>,
    scope: Option<String>,
    resource: Option<String>,
    refusal: Option<Refusal>,
    delay: Option<Duration>,
    callers: Option<Callers>,
    dpop: bool,
    nonce: Option<String>,
    untyped: bool,
    accepted: BTreeSet<String>,
    subjects: BTreeMap<String, String>,
    bound: BTreeMap<String, String>,
    jti: BTreeSet<String>,
    assertions: Vec<String>,
    verdicts: Vec<Verdict>,
    forms: Vec<Vec<(String, String)>>,
    exchanges: Vec<Exchanged>,
}

#[derive(Debug)]
struct Shared {
    client_id: String,
    token_url: String,
    state: Mutex<State>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The harness token endpoint.
#[derive(Debug)]
pub struct TokenEndpoint {
    server: Server,
    shared: Arc<Shared>,
}

impl TokenEndpoint {
    /// Starts an endpoint that authenticates the client `client_id` and
    /// issues tokens that live `expires_in` seconds, stating none when
    /// `expires_in` is `None`.
    pub async fn start(client_id: &str, expires_in: Option<u64>) -> Self {
        let server = Server::start().await;
        let shared = Arc::new(Shared {
            client_id: client_id.to_owned(),
            token_url: token_url(&server),
            state: Mutex::new(State {
                expires_in,
                ..State::default()
            }),
        });
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(Responder(Arc::clone(&shared)))
            .mount(&server)
            .await;
        Self { server, shared }
    }

    /// The token URL, the `aud` every assertion must carry.
    #[must_use]
    pub fn token_url(&self) -> String {
        token_url(&self.server)
    }

    /// Verifies every later assertion against `jwks`.
    pub fn trust(&self, jwks: JwkSet) {
        self.shared.lock().jwks = jwks;
    }

    /// Refuses every later request whose `scope` is not `scope`.
    pub fn expect_scope(&self, scope: &str) {
        self.shared.lock().scope = Some(scope.to_owned());
    }

    /// Refuses every later token exchange whose `resource` is not
    /// `resource`.
    pub fn expect_resource(&self, resource: &str) {
        self.shared.lock().resource = Some(resource.to_owned());
    }

    /// Answers token exchange from now on, verifying each subject token as
    /// an access token `issuer` signed with a key of `jwks`.
    pub fn accept_exchange(&self, jwks: JwkSet, issuer: &str) {
        self.shared.lock().callers = Some(Callers {
            jwks,
            issuer: issuer.to_owned(),
        });
    }

    /// Requires a `DPoP` proof on every later token request, and binds every
    /// token it issues to the proof's key, typed `DPoP` (RFC 9449 §5).
    pub fn require_dpop(&self) {
        self.shared.lock().dpop = true;
    }

    /// Requires every later proof to name `nonce`, answering one that does
    /// not with `400` `use_dpop_nonce` and the nonce (RFC 9449 §8).
    pub fn require_nonce(&self, nonce: &str) {
        self.shared.lock().nonce = Some(nonce.to_owned());
    }

    /// Answers every later token exchange without the `issued_token_type`
    /// RFC 8693 §2.2.1 requires, as a defective server would.
    pub fn omit_issued_token_type(&self) {
        self.shared.lock().untyped = true;
    }

    /// Answers every later request with the RFC 6749 §5.2 error `error`,
    /// `description` and `status`.
    pub fn refuse(&self, status: u16, error: &str, description: &str) {
        self.shared.lock().refusal = Some(Refusal {
            status,
            error: error.to_owned(),
            description: description.to_owned(),
        });
    }

    /// Answers every later request after `delay`.
    pub fn delay(&self, delay: Duration) {
        self.shared.lock().delay = Some(delay);
    }

    /// Stops accepting every token issued so far, as a node whose
    /// authorization server revoked them would.
    pub fn revoke_all(&self) {
        self.shared.lock().accepted.clear();
    }

    /// The verdict of every request, in arrival order.
    #[must_use]
    pub fn verdicts(&self) -> Vec<Verdict> {
        self.shared.lock().verdicts.clone()
    }

    /// The tokens issued so far.
    #[must_use]
    pub fn issued(&self) -> usize {
        self.verdicts()
            .iter()
            .filter(|verdict| **verdict == Verdict::Issued)
            .count()
    }

    /// Every client assertion received, in arrival order.
    #[must_use]
    pub fn assertions(&self) -> Vec<String> {
        self.shared.lock().assertions.clone()
    }

    /// Every form parameter of every request, in arrival order.
    #[must_use]
    pub fn forms(&self) -> Vec<Vec<(String, String)>> {
        self.shared.lock().forms.clone()
    }

    /// Every token exchange answered with a token, in arrival order.
    #[must_use]
    pub fn exchanges(&self) -> Vec<Exchanged> {
        self.shared.lock().exchanges.clone()
    }

    /// A matcher for a request carrying `Authorization: Bearer` with a token
    /// this endpoint issued and still accepts.
    #[must_use]
    pub fn bearer(&self) -> IssuedBearer {
        IssuedBearer {
            shared: Arc::clone(&self.shared),
            subject: None,
        }
    }

    /// A matcher for a request carrying `Authorization: Bearer` with a token
    /// this endpoint exchanged for the caller `subject` and still accepts.
    #[must_use]
    pub fn bearer_for(&self, subject: &str) -> IssuedBearer {
        IssuedBearer {
            shared: Arc::clone(&self.shared),
            subject: Some(subject.to_owned()),
        }
    }

    /// A matcher for a request carrying `Authorization: DPoP` with a token
    /// this endpoint bound and still accepts, and a `DPoP` proof of the
    /// bound key over the request's method, URL and token, naming `nonce`
    /// when a nonce is given (RFC 9449 §7.1).
    #[must_use]
    pub fn dpop_bound(&self, nonce: Option<&str>) -> DpopBound {
        DpopBound {
            shared: Arc::clone(&self.shared),
            nonce: nonce.map(str::to_owned),
        }
    }
}

/// The wiremock matcher of [`TokenEndpoint::bearer`] and
/// [`TokenEndpoint::bearer_for`].
#[derive(Debug)]
pub struct IssuedBearer {
    shared: Arc<Shared>,
    subject: Option<String>,
}

impl Match for IssuedBearer {
    fn matches(&self, request: &Request) -> bool {
        let Some(value) = request.headers.get(http::header::AUTHORIZATION) else {
            return false;
        };
        let Some(token) = value.to_str().ok().and_then(|v| v.strip_prefix("Bearer ")) else {
            return false;
        };
        let state = self.shared.lock();
        state.accepted.contains(token)
            && self
                .subject
                .as_ref()
                .is_none_or(|subject| state.subjects.get(token) == Some(subject))
    }
}

/// The wiremock matcher of [`TokenEndpoint::dpop_bound`].
#[derive(Debug)]
pub struct DpopBound {
    shared: Arc<Shared>,
    nonce: Option<String>,
}

impl Match for DpopBound {
    fn matches(&self, request: &Request) -> bool {
        let Some(token) = request
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("DPoP "))
        else {
            return false;
        };
        let Some(proof) = request
            .headers
            .get(dpop::HEADER)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        // The mock server reads every request over plain HTTP and names its
        // URL by `localhost`, so the URL the client sent is its Host and path.
        let Some(host) = request
            .headers
            .get(http::header::HOST)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let htu = format!("http://{host}{}", request.url.path());
        let Ok(verified) = dpop::verify(proof, request.method.as_str(), &htu, Some(token)) else {
            return false;
        };
        let state = self.shared.lock();
        state.accepted.contains(token)
            && state.bound.get(token) == Some(&verified.jkt)
            && self
                .nonce
                .as_ref()
                .is_none_or(|nonce| verified.proof.nonce.as_ref() == Some(nonce))
    }
}

/// The token URL of the endpoint `server` runs.
fn token_url(server: &Server) -> String {
    format!("{}{TOKEN_PATH}", server.uri())
}
