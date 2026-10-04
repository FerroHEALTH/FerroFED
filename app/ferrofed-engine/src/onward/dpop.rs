// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DPoP`: onward tokens bound to a key of the gateway's (RFC 9449), for a
//! deployment that requires sender-constrained tokens (§13.4).
//!
//! [`DpopTransport`] wraps the HTTP engine every node request and every
//! token request is sent through. A request whose URL lies under a route,
//! a node's base URL or its token endpoint, gets a proof in the [`HEADER`]
//! signed by that route's [`Prover`]: a JWS typed [`PROOF_TYPE`] whose
//! header carries the public key, and whose claims bind the request's
//! method (`htm`) and URL without query and fragment (`htu`), with a fresh
//! `jti` and `iat` (RFC 9449 §4.2). The proof is made where the request is
//! final, because only the engine sees its method and URL. A request that
//! carries an access token sends it under the `DPoP` scheme, and its proof
//! carries the token's hash in `ath` (RFC 9449 §7.1).
//!
//! A server that demands a nonce answers with one in [`NONCE_HEADER`]: the
//! token endpoint with `400` and `use_dpop_nonce` (RFC 9449 §8), a node
//! with `401` and a `DPoP` challenge naming `use_dpop_nonce` (§9). The
//! transport then sends the request once more, with the nonce in a new
//! proof, to the same URL and within the time the request had left. Every
//! nonce a server sends is kept per origin and put in the next proof to it.
//! A request under no route passes through unchanged.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{HeaderValue, StatusCode};
use jsonwebtoken::jwk::{Jwk, ThumbprintHash};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use openehr_its::rest::client::{RequestTimeout, Transport, TransportError};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use url::Url;

/// The header a proof travels in (RFC 9449 §4.1).
pub const HEADER: &str = "DPoP";

/// The header a server sends a nonce in (RFC 9449 §8, §9).
pub const NONCE_HEADER: &str = "DPoP-Nonce";

/// The JOSE `typ` of a proof (RFC 9449 §4.2).
pub const PROOF_TYPE: &str = "dpop+jwt";

/// The `error` a server names when it demands a nonce (RFC 9449 §8, §9).
pub const USE_NONCE: &str = "use_dpop_nonce";

/// A `DPoP` key that cannot be used.
///
/// No variant carries the key or any part of it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DpopKeyError {
    /// The text is not an EC private key in PKCS#8 PEM.
    #[error("the DPoP key is not an EC private key in PKCS#8 PEM")]
    Pem(#[source] jsonwebtoken::errors::Error),
    /// The key is on neither P-256 nor P-384, the curves ES256 and ES384
    /// sign with (RFC 7518 §3.4).
    #[error("the DPoP key is on neither P-256 (ES256) nor P-384 (ES384)")]
    Curve(#[source] jsonwebtoken::errors::Error),
    /// The key's RFC 7638 thumbprint could not be computed.
    #[error("the RFC 7638 thumbprint of the DPoP key could not be computed")]
    Thumbprint(#[source] jsonwebtoken::errors::Error),
}

/// The key one endpoint's tokens are bound to, and the nonces its servers
/// sent.
///
/// `Debug` shows the key's thumbprint alone.
pub struct Prover {
    private: EncodingKey,
    algorithm: Algorithm,
    public: Jwk,
    thumbprint: String,
    nonces: Mutex<BTreeMap<String, String>>,
}

/// The claims of one proof (RFC 9449 §4.2).
#[derive(Serialize)]
struct Claims<'a> {
    jti: String,
    htm: &'a str,
    htu: &'a str,
    iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    ath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<&'a str>,
}

impl Prover {
    /// Reads the private key `pem` holds in PKCS#8 PEM: a P-256 key signs
    /// ES256, a P-384 key ES384.
    ///
    /// # Errors
    ///
    /// Returns [`DpopKeyError::Pem`] for text that is no EC private key in
    /// PKCS#8 PEM, [`DpopKeyError::Curve`] for a key on another curve, and
    /// [`DpopKeyError::Thumbprint`] when its thumbprint cannot be computed.
    pub fn from_pem(pem: &SecretString) -> Result<Self, DpopKeyError> {
        let private =
            EncodingKey::from_ec_pem(pem.expose_secret().as_bytes()).map_err(DpopKeyError::Pem)?;
        let (algorithm, public) = match Jwk::from_encoding_key(&private, Algorithm::ES256) {
            Ok(public) => (Algorithm::ES256, public),
            Err(_not_p256) => (
                Algorithm::ES384,
                Jwk::from_encoding_key(&private, Algorithm::ES384).map_err(DpopKeyError::Curve)?,
            ),
        };
        let thumbprint = public
            .thumbprint(ThumbprintHash::SHA256)
            .map_err(DpopKeyError::Thumbprint)?;
        Ok(Self {
            private,
            algorithm,
            public,
            thumbprint,
            nonces: Mutex::new(BTreeMap::new()),
        })
    }

    /// The algorithm every proof is signed with.
    #[must_use]
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// The public key every proof carries.
    #[must_use]
    pub fn public(&self) -> &Jwk {
        &self.public
    }

    /// The key's RFC 7638 thumbprint over SHA-256, the `jkt` a bound token
    /// names (RFC 9449 §6.1).
    #[must_use]
    pub fn thumbprint(&self) -> &str {
        &self.thumbprint
    }

    /// A proof of a request with `method` to `htu`, bound to `token` when
    /// the request carries one and naming `nonce` when the server sent one.
    fn proof(
        &self,
        method: &http::Method,
        htu: &str,
        token: Option<&str>,
        nonce: Option<&str>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let claims = Claims {
            jti: uuid::Uuid::new_v4().to_string(),
            htm: method.as_str(),
            htu,
            iat: jiff::Timestamp::now().as_second(),
            ath: token.map(|token| URL_SAFE_NO_PAD.encode(digest(&SHA256, token.as_bytes()))),
            nonce,
        };
        let mut header = Header::new(self.algorithm);
        header.typ = Some(PROOF_TYPE.to_owned());
        header.jwk = Some(self.public.clone());
        jsonwebtoken::encode(&header, &claims, &self.private)
    }

    /// The nonce `origin` sent last, when it sent one.
    fn nonce(&self, origin: &str) -> Option<String> {
        self.lock().get(origin).cloned()
    }

    /// Keeps `nonce` as the one `origin` sent last.
    fn remember(&self, origin: &str, nonce: String) {
        self.lock().insert(origin.to_owned(), nonce);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        // NOTE: no specification governs this: our own design; a panic while the
        // lock was held leaves at worst a stale nonce, which the server replaces.
        self.nonces.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for Prover {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prover")
            .field("thumbprint", &self.thumbprint)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

/// A route: the URLs one [`Prover`] proves requests to.
#[derive(Debug, Clone)]
struct Route {
    prefix: Url,
    prover: Arc<Prover>,
}

impl Route {
    /// Whether `url` lies under this route: the same origin, and a path
    /// that is the route's path or continues it after a `/`.
    fn covers(&self, url: &Url) -> bool {
        if url.origin() != self.prefix.origin() {
            return false;
        }
        let root = self.prefix.path().trim_end_matches('/');
        url.path()
            .strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

/// The HTTP engine `inner`, with a `DPoP` proof on every request under one
/// of its routes.
///
/// `Debug` names the routes and their keys' thumbprints.
#[derive(Debug, Clone)]
pub struct DpopTransport<T> {
    inner: T,
    routes: Arc<[Route]>,
}

impl<T> DpopTransport<T> {
    /// The engine `inner` with no route: every request passes through
    /// unchanged.
    #[must_use]
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            routes: Arc::from(Vec::new()),
        }
    }

    /// This engine, proving every request under `prefix` with `prover`.
    ///
    /// The longest route that covers a request proves it.
    #[must_use]
    pub fn with_route(mut self, prefix: Url, prover: Arc<Prover>) -> Self {
        let mut routes = self.routes.to_vec();
        routes.push(Route { prefix, prover });
        routes.sort_by_key(|route| std::cmp::Reverse(route.prefix.as_str().len()));
        self.routes = Arc::from(routes);
        self
    }

    /// The prover of the route that covers `url`, when one does.
    fn prover_for(&self, url: &Url) -> Option<&Arc<Prover>> {
        self.routes
            .iter()
            .find(|route| route.covers(url))
            .map(|route| &route.prover)
    }
}

#[async_trait::async_trait]
impl<T: Transport> Transport for DpopTransport<T> {
    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        // NOTE: no specification governs this: our own design; a URI the url crate
        // cannot read is under no route, and its request passes through unchanged.
        let url = Url::parse(&request.uri().to_string()).ok();
        let Some(prover) = url.as_ref().and_then(|url| self.prover_for(url)) else {
            return self.inner.send(request).await;
        };
        let Some(url) = url.as_ref() else {
            return self.inner.send(request).await;
        };
        let started = Instant::now();
        let (parts, body) = request.into_parts();
        let token = bearer(&parts.headers);
        let origin = url.origin().ascii_serialization();
        let mut htu = url.clone();
        htu.set_query(None);
        htu.set_fragment(None);
        let sent = prover.nonce(&origin);
        let first = self
            .attempt(
                prover,
                (parts.clone(), &body),
                (htu.as_str(), token.as_deref()),
                sent.as_deref(),
            )
            .await?;
        let learned = nonce_of(&first);
        if let Some(nonce) = &learned {
            prover.remember(&origin, nonce.clone());
        }
        let fresh = learned.filter(|nonce| sent.as_deref() != Some(nonce.as_str()));
        let (Some(nonce), true) = (fresh, demands_nonce(&first)) else {
            return Ok(first);
        };
        let mut parts = parts;
        if let Some(RequestTimeout(budget)) = parts.extensions.get::<RequestTimeout>().copied() {
            let left = budget.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Ok(first);
            }
            parts.extensions.insert(RequestTimeout(left));
        }
        let retried = self
            .attempt(
                prover,
                (parts, &body),
                (htu.as_str(), token.as_deref()),
                Some(&nonce),
            )
            .await?;
        if let Some(nonce) = nonce_of(&retried) {
            prover.remember(&origin, nonce);
        }
        Ok(retried)
    }
}

impl<T: Transport> DpopTransport<T> {
    /// Sends the request of `parts` and `body` once, with a proof of
    /// `prover` over `htu`, bound to `token` and naming `nonce`.
    async fn attempt(
        &self,
        prover: &Prover,
        (parts, body): (http::request::Parts, &[u8]),
        (htu, token): (&str, Option<&str>),
        nonce: Option<&str>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        let proof = prover
            .proof(&parts.method, htu, token, nonce)
            .map_err(unsent)?;
        let mut request = http::Request::from_parts(parts, body.to_vec());
        let headers = request.headers_mut();
        if let Some(token) = token {
            let value = HeaderValue::from_str(&format!("DPoP {token}")).map_err(unsent)?;
            let mut value = value;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        headers.insert(HEADER, HeaderValue::from_str(&proof).map_err(unsent)?);
        self.inner.send(request).await
    }
}

/// The access token an `Authorization: Bearer` header carries.
///
/// The node client composes every onward token as a bearer credential;
/// under a route it is sent under the `DPoP` scheme instead (RFC 9449 §7.1).
fn bearer(headers: &http::HeaderMap) -> Option<String> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim_start_matches(' ').to_owned())
}

/// The nonce `response` carries in [`NONCE_HEADER`], when it carries one.
fn nonce_of(response: &http::Response<Vec<u8>>) -> Option<String> {
    // NOTE: RFC 9449 §8, a nonce is visible ASCII; one that is not is
    // legitimately unusable, and the server is answered as if it sent none.
    response
        .headers()
        .get(NONCE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The `error` member of an RFC 6749 §5.2 error body.
#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// Whether `response` demands a nonce: a token endpoint's `400` with
/// [`USE_NONCE`] (RFC 9449 §8), or a resource's `401` whose `DPoP`
/// challenge names it (§9).
fn demands_nonce(response: &http::Response<Vec<u8>>) -> bool {
    let status = response.status();
    if status == StatusCode::BAD_REQUEST {
        return serde_json::from_slice::<ErrorBody>(response.body())
            .is_ok_and(|body| body.error == USE_NONCE);
    }
    status == StatusCode::UNAUTHORIZED
        && response
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|challenge| {
                challenge
                    .get(..4)
                    .is_some_and(|scheme| scheme.eq_ignore_ascii_case("dpop"))
                    && challenge.contains(USE_NONCE)
            })
}

/// The transport error of a request the proof could not be attached to.
fn unsent(source: impl std::error::Error + Send + Sync + 'static) -> TransportError {
    TransportError::Send {
        source: Box::new(source),
    }
}
