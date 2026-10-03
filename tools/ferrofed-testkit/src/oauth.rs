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
//! [`es384_pem`] and [`p256_pem`] generate a synthetic private key in PKCS#8
//! PEM at run time, so no key is ever committed. No specification governs the
//! device: our own design.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P384_SHA384_FIXED_SIGNING, EcdsaKeyPair,
    EcdsaSigningAlgorithm,
};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

/// The path the token endpoint answers on.
pub const TOKEN_PATH: &str = "/oauth2/token";

/// The `client_assertion_type` of a JWT client assertion (RFC 7523 §2.2).
pub const JWT_BEARER: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

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

/// What the endpoint made of one token request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A token was issued.
    Issued,
    /// The request was refused, for this reason.
    Refused(String),
}

/// A refusal the endpoint answers every request with, set by
/// [`TokenEndpoint::refuse`].
#[derive(Debug, Clone)]
struct Refusal {
    status: u16,
    error: String,
    description: String,
}

#[derive(Debug, Default)]
struct State {
    jwks: JwkSet,
    expires_in: Option<u64>,
    scope: Option<String>,
    refusal: Option<Refusal>,
    delay: Option<Duration>,
    accepted: BTreeSet<String>,
    jti: BTreeSet<String>,
    assertions: Vec<String>,
    verdicts: Vec<Verdict>,
    forms: Vec<Vec<(String, String)>>,
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

    /// A matcher for a request carrying `Authorization: Bearer` with a token
    /// this endpoint issued and still accepts.
    #[must_use]
    pub fn bearer(&self) -> IssuedBearer {
        IssuedBearer(Arc::clone(&self.shared))
    }
}

/// The wiremock matcher of [`TokenEndpoint::bearer`].
#[derive(Debug)]
pub struct IssuedBearer(Arc<Shared>);

impl Match for IssuedBearer {
    fn matches(&self, request: &Request) -> bool {
        let Some(value) = request.headers.get(http::header::AUTHORIZATION) else {
            return false;
        };
        let Some(token) = value.to_str().ok().and_then(|v| v.strip_prefix("Bearer ")) else {
            return false;
        };
        self.0.lock().accepted.contains(token)
    }
}

struct Responder(Arc<Shared>);

#[derive(Serialize)]
struct Issued<'a> {
    access_token: &'a str,
    token_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
}

#[derive(Serialize)]
struct Refused<'a> {
    error: &'a str,
    error_description: &'a str,
}

impl Respond for Responder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let form: Vec<(String, String)> = url::form_urlencoded::parse(&request.body)
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let mut state = self.0.lock();
        state.forms.push(form.clone());
        let delay = state.delay;
        let answer = self.answer(&mut state, &form);
        match delay {
            Some(delay) => answer.set_delay(delay),
            None => answer,
        }
    }
}

impl Responder {
    /// The answer to a request carrying `form`, recorded in `state`.
    fn answer(&self, state: &mut State, form: &[(String, String)]) -> ResponseTemplate {
        let field = |name: &str| {
            form.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        if let Some(refusal) = state.refusal.clone() {
            state.verdicts.push(Verdict::Refused(refusal.error.clone()));
            return refused(refusal.status, &refusal.error, &refusal.description);
        }
        let outcome = (|| {
            if field("grant_type").as_deref() != Some("client_credentials") {
                return Err(("unsupported_grant_type", "grant_type".to_owned()));
            }
            if field("client_assertion_type").as_deref() != Some(JWT_BEARER) {
                return Err(("invalid_client", "client_assertion_type".to_owned()));
            }
            if field("client_secret").is_some() {
                return Err(("invalid_request", "a client secret was sent".to_owned()));
            }
            if let Some(expected) = &state.scope
                && field("scope").as_deref() != Some(expected.as_str())
            {
                return Err(("invalid_scope", "scope".to_owned()));
            }
            let assertion = field("client_assertion")
                .ok_or(("invalid_client", "no client_assertion".to_owned()))?;
            state.assertions.push(assertion.clone());
            let claims = verify(
                &assertion,
                &state.jwks,
                &self.0.client_id,
                &self.0.token_url,
            )
            .map_err(|reason| ("invalid_client", reason))?;
            if !state.jti.insert(claims.jti) {
                return Err(("invalid_client", "the jti was used before".to_owned()));
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                let token = uuid::Uuid::new_v4().simple().to_string();
                state.accepted.insert(token.clone());
                state.verdicts.push(Verdict::Issued);
                let body = Issued {
                    access_token: &token,
                    token_type: "Bearer",
                    expires_in: state.expires_in,
                };
                ResponseTemplate::new(200).set_body_json(body)
            }
            Err((error, reason)) => {
                state.verdicts.push(Verdict::Refused(reason.clone()));
                refused(400, error, &reason)
            }
        }
    }
}

fn refused(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(Refused {
        error,
        error_description: description,
    })
}

/// The token URL of the endpoint `server` runs.
fn token_url(server: &Server) -> String {
    format!("{}{TOKEN_PATH}", server.uri())
}
