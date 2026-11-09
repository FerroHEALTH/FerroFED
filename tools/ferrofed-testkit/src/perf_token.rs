// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The performance gate's token endpoint (#1712), a test device in place of
//! a node's authorization server.
//!
//! The `ferrofed-perf-token` binary serves it as a process of its own, so
//! the gate can send part of its load through a node's onward lane and
//! count the exchanges it causes.
//!
//! **This is a test device, not an authorization server.** It answers
//! `POST /token` for one client, authenticated by `private_key_jwt` (RFC
//! 7523 §2.2, the assertion checked as RFC 7523 §3 asks), with two grants. The client-credentials grant (RFC
//! 6749 §4.4) issues the gateway's own token, from the machines issuer. RFC
//! 8693 token exchange verifies the `subject_token` against the caller
//! issuer's key set (its signature, `iss` and `exp`) and issues a token of
//! the persons issuer for the same `sub`, the client as `client_id`, the
//! requested `scope` or else the subject token's, and the `resource` or
//! `audience` it names as `aud`.
//! Both issued tokens are ES256 JWTs typed `at+jwt` (RFC 9068 §2.1) that
//! live [`Settings::lifetime_s`]. `GET /calls` answers how many requests
//! of each grant it answered and how many it refused, which the gate reads
//! to judge the onward cache. Each issuer's key is generated at start and
//! its key set written to a file for the node that verifies the tokens, and
//! so is the client's P-256 key, written as PKCS#8 PEM for the gateway that
//! signs its assertions with it under the RFC 7638 thumbprint as `kid`. No
//! specification governs the device: our own design.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use http::request::Parts;
use http::{HeaderValue, Method, Response, StatusCode, header};
use http_body_util::Full;
use jsonwebtoken::jwk::{Jwk, JwkSet, ThumbprintHash};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::issuer::{Claims, Issuer, IssuerError};
use crate::oauth::{Curve, JWT_BEARER, KeyGenError};

/// The path the token endpoint answers on.
pub const TOKEN_PATH: &str = "/token";

/// The path the call counts are read on.
pub const CALLS_PATH: &str = "/calls";

/// The `grant_type` of token exchange (RFC 8693 §2.1).
const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The token type identifier of an access token (RFC 8693 §3).
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

/// What the device issues, for whom, and where it writes its key sets.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The address it listens on.
    pub listen: SocketAddr,
    /// The `iss` of an exchanged token, the onward lane's issuer.
    pub persons_issuer: String,
    /// Where the persons issuer's key set is written.
    pub persons_jwks: PathBuf,
    /// The `iss` of the gateway's own token.
    pub machines_issuer: String,
    /// Where the machines issuer's key set is written.
    pub machines_jwks: PathBuf,
    /// The `aud` of a token whose request names no resource or audience.
    pub audience: String,
    /// The one client it authenticates.
    pub client_id: String,
    /// Where that client's private key is written, for the gateway's
    /// `private_key_file`.
    pub client_key: PathBuf,
    /// The `iss` a `subject_token` must carry.
    pub caller_issuer: String,
    /// The key set a `subject_token` is verified against.
    pub caller_jwks: PathBuf,
    /// How long an issued token lives, in seconds.
    pub lifetime_s: i64,
}

/// The device could not start.
#[derive(Debug, thiserror::Error)]
pub enum PerfTokenError {
    /// An issuer's key could not be generated or its key set written.
    #[error("an issuer of the token endpoint could not be set up")]
    Issuer(#[source] IssuerError),
    /// The caller issuer's key set could not be read.
    #[error("the caller issuer's key set {path} could not be read")]
    CallerJwks {
        /// The file that was read.
        path: PathBuf,
        /// Why it could not be.
        #[source]
        source: io::Error,
    },
    /// The caller issuer's key set is no JWK Set.
    #[error("the caller issuer's key set {path} is no JWK Set")]
    CallerJwksJson {
        /// The file that was read.
        path: PathBuf,
        /// Why it is none.
        #[source]
        source: serde_json::Error,
    },
    /// The client's key could not be generated.
    #[error("the client's key could not be generated")]
    ClientKey(#[source] KeyGenError),
    /// The client's key could not be read as a JWK.
    #[error("the client's key could not be read as a JWK")]
    ClientJwk(#[source] jsonwebtoken::errors::Error),
    /// The client's key could not be written.
    #[error("the client's key {path} could not be written")]
    ClientKeyWrite {
        /// The file that was written.
        path: PathBuf,
        /// Why it could not be.
        #[source]
        source: io::Error,
    },
    /// The listen address could not be bound.
    #[error("the token endpoint could not listen")]
    Bind(#[source] io::Error),
}

/// How many requests of each kind the device answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calls {
    /// Client-credentials grants issued.
    pub client_credentials: u64,
    /// Token exchanges issued.
    pub token_exchange: u64,
    /// Token requests refused.
    pub refused: u64,
}

/// A token response (RFC 6749 §5.1, RFC 8693 §2.2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAnswer {
    /// The issued token.
    pub access_token: String,
    /// `Bearer`.
    pub token_type: String,
    /// The token's lifetime in seconds.
    pub expires_in: i64,
    /// An access token's type identifier, on a token exchange alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_token_type: Option<String>,
}

/// An error response (RFC 6749 §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorAnswer {
    /// The error code.
    pub error: String,
}

/// The running device; dropping it stops the listener.
#[derive(Debug)]
pub struct PerfTokenEndpoint {
    address: SocketAddr,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl Drop for PerfTokenEndpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl PerfTokenEndpoint {
    /// Generates both issuers' keys, writes their key sets, generates and
    /// writes the client's key, reads the caller issuer's key set, and
    /// listens on [`Settings::listen`].
    ///
    /// # Errors
    ///
    /// Returns [`PerfTokenError`] naming the step that failed.
    pub async fn start(settings: Settings) -> Result<Self, PerfTokenError> {
        let persons =
            Issuer::new(settings.persons_issuer.clone()).map_err(PerfTokenError::Issuer)?;
        persons
            .write_jwks(&settings.persons_jwks)
            .map_err(PerfTokenError::Issuer)?;
        let machines =
            Issuer::new(settings.machines_issuer.clone()).map_err(PerfTokenError::Issuer)?;
        machines
            .write_jwks(&settings.machines_jwks)
            .map_err(PerfTokenError::Issuer)?;
        let text = std::fs::read_to_string(&settings.caller_jwks).map_err(|source| {
            PerfTokenError::CallerJwks {
                path: settings.caller_jwks.clone(),
                source,
            }
        })?;
        let caller: JwkSet =
            serde_json::from_str(&text).map_err(|source| PerfTokenError::CallerJwksJson {
                path: settings.caller_jwks.clone(),
                source,
            })?;
        let client = client_key(&settings.client_key)?;
        let listener = TcpListener::bind(settings.listen)
            .await
            .map_err(PerfTokenError::Bind)?;
        let address = listener.local_addr().map_err(PerfTokenError::Bind)?;
        let shared = Arc::new(Shared {
            token_url: token_url(address),
            settings,
            persons,
            machines,
            caller,
            client,
            client_credentials: AtomicU64::new(0),
            token_exchange: AtomicU64::new(0),
            refused: AtomicU64::new(0),
        });
        let task = tokio::spawn(crate::connection::accept(
            listener,
            Arc::clone(&shared),
            handle,
        ));
        Ok(Self {
            address,
            shared,
            task,
        })
    }

    /// Returns the address it listens on.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the token endpoint's URL.
    #[must_use]
    pub fn token_url(&self) -> String {
        token_url(self.address)
    }

    /// Returns how many requests of each kind it answered so far.
    #[must_use]
    pub fn calls(&self) -> Calls {
        self.shared.calls()
    }

    /// Serves until the process ends.
    ///
    /// # Errors
    ///
    /// Returns the join error when the listener task ended by a panic.
    pub async fn serve(mut self) -> Result<(), tokio::task::JoinError> {
        (&mut self.task).await
    }
}

/// The token endpoint's URL at `address`, the `aud` of a client assertion.
fn token_url(address: SocketAddr) -> String {
    format!("http://{address}{TOKEN_PATH}")
}

/// Generates the client's P-256 key, writes it to `path` readable by its
/// owner alone, and returns its public half as a key set under its RFC 7638
/// thumbprint.
fn client_key(path: &std::path::Path) -> Result<JwkSet, PerfTokenError> {
    let pem = Curve::P256.pem().map_err(PerfTokenError::ClientKey)?;
    let key = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(PerfTokenError::ClientJwk)?;
    let mut jwk =
        Jwk::from_encoding_key(&key, Algorithm::ES256).map_err(PerfTokenError::ClientJwk)?;
    jwk.common.key_id = Some(
        jwk.thumbprint(ThumbprintHash::SHA256)
            .map_err(PerfTokenError::ClientJwk)?,
    );
    write_private(path, &pem).map_err(|source| PerfTokenError::ClientKeyWrite {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(JwkSet { keys: vec![jwk] })
}

/// Writes `text` to `path`, readable by its owner alone where the platform
/// has modes.
fn write_private(path: &std::path::Path, text: &str) -> io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

/// What every connection shares.
struct Shared {
    token_url: String,
    settings: Settings,
    persons: Issuer,
    machines: Issuer,
    caller: JwkSet,
    client: JwkSet,
    client_credentials: AtomicU64,
    token_exchange: AtomicU64,
    refused: AtomicU64,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("calls", &self.calls())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn calls(&self) -> Calls {
        Calls {
            client_credentials: self.client_credentials.load(Ordering::Relaxed),
            token_exchange: self.token_exchange.load(Ordering::Relaxed),
            refused: self.refused.load(Ordering::Relaxed),
        }
    }
}

/// The claims of a `subject_token` the device reads.
#[derive(Debug, Deserialize)]
struct Subject {
    sub: String,
    #[serde(default)]
    scope: Option<String>,
}

/// Routes one request.
async fn handle(
    shared: Arc<Shared>,
    parts: Parts,
    body: crate::connection::Body,
) -> Response<Full<Bytes>> {
    match (&parts.method, parts.uri.path()) {
        (&Method::GET, CALLS_PATH) => json(StatusCode::OK, &shared.calls()),
        (&Method::POST, TOKEN_PATH) => match body {
            Ok(body) => token(&shared, &body),
            Err(_) => refuse(&shared, "invalid_request"),
        },
        _ => {
            let mut answer = Response::new(Full::new(Bytes::new()));
            *answer.status_mut() = StatusCode::NOT_FOUND;
            answer
        }
    }
}

/// Answers one token request.
fn token(shared: &Shared, body: &[u8]) -> Response<Full<Bytes>> {
    let form: Vec<(String, String)> = url::form_urlencoded::parse(body).into_owned().collect();
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let settings = &shared.settings;
    // NOTE: RFC 7521 §4.2: `client_id` is optional beside an assertion and names the client when sent.
    let named = field("client_id").is_none_or(|id| id == settings.client_id);
    let asserted = field("client_assertion_type") == Some(JWT_BEARER)
        && field("client_assertion").is_some_and(|assertion| {
            crate::oauth::verify_signed(
                assertion,
                &shared.client,
                (settings.client_id.as_str(), shared.token_url.as_str()),
                Some(Algorithm::ES256),
            )
            .is_ok()
        });
    if !named || !asserted || field("client_secret").is_some() {
        return refuse(shared, "invalid_client");
    }
    let scope = field("scope").map(str::to_owned);
    let audience = field("resource")
        .or_else(|| field("audience"))
        .unwrap_or(settings.audience.as_str());
    match field("grant_type") {
        Some("client_credentials") => {
            let mut claims = Claims::new(&settings.machines_issuer, audience);
            claims.sub.clone_from(&settings.client_id);
            issue(shared, &shared.machines, claims, scope, false)
        }
        Some(TOKEN_EXCHANGE) => {
            let Some(subject) = field("subject_token").and_then(|token| verify(shared, token))
            else {
                return refuse(shared, "invalid_grant");
            };
            let mut claims = Claims::new(&settings.persons_issuer, audience);
            claims.sub = subject.sub;
            // NOTE: RFC 8693 §2.1: with no `scope` the server picks it; the device grants the subject's.
            issue(
                shared,
                &shared.persons,
                claims,
                scope.or(subject.scope),
                true,
            )
        }
        _ => refuse(shared, "unsupported_grant_type"),
    }
}

/// The `subject_token`'s claims when the caller issuer signed it and it has
/// not expired.
fn verify(shared: &Shared, token: &str) -> Option<Subject> {
    let header = jsonwebtoken::decode_header(token).ok()?;
    let jwk = shared.caller.find(header.kid.as_deref()?)?;
    let key = DecodingKey::from_jwk(jwk).ok()?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[shared.settings.caller_issuer.as_str()]);
    validation.validate_aud = false;
    // NOTE: RFC 8693 §2.1: a subject token the device cannot verify is refused as `invalid_grant`.
    jsonwebtoken::decode::<Subject>(token, &key, &validation)
        .ok()
        .map(|data| data.claims)
}

/// Signs `claims` for the client as `issuer` and answers with the token.
fn issue(
    shared: &Shared,
    issuer: &Issuer,
    mut claims: Claims,
    scope: Option<String>,
    exchanged: bool,
) -> Response<Full<Bytes>> {
    let settings = &shared.settings;
    claims.client_id.clone_from(&settings.client_id);
    claims.exp = claims.iat.saturating_add(settings.lifetime_s);
    claims.scope = scope;
    claims.extensions = None;
    let Ok(access_token) = issuer.mint(&claims) else {
        return refuse(shared, "server_error");
    };
    let counter = if exchanged {
        &shared.token_exchange
    } else {
        &shared.client_credentials
    };
    counter.fetch_add(1, Ordering::Relaxed);
    json(
        StatusCode::OK,
        &TokenAnswer {
            access_token,
            token_type: String::from("Bearer"),
            expires_in: settings.lifetime_s,
            issued_token_type: exchanged.then(|| ACCESS_TOKEN_TYPE.to_owned()),
        },
    )
}

/// An RFC 6749 §5.2 error, counted as refused.
fn refuse(shared: &Shared, error: &str) -> Response<Full<Bytes>> {
    shared.refused.fetch_add(1, Ordering::Relaxed);
    json(
        StatusCode::BAD_REQUEST,
        &ErrorAnswer {
            error: error.to_owned(),
        },
    )
}

/// An answer of `status` with `value` as its JSON body, or an empty `500`
/// when it cannot be written.
fn json(status: StatusCode, value: &impl Serialize) -> Response<Full<Bytes>> {
    let Ok(body) = serde_json::to_vec(value) else {
        let mut answer = Response::new(Full::new(Bytes::new()));
        *answer.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        return answer;
    };
    let mut answer = Response::new(Full::new(Bytes::from(body)));
    *answer.status_mut() = status;
    let headers = answer.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    answer
}
