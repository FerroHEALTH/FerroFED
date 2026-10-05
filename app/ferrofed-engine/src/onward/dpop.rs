// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DPoP`: onward tokens bound to a key of the gateway's (RFC 9449), for a
//! deployment that requires sender-constrained tokens (§13.4).
//!
//! A grant's [`Prover`] holds the key. Every request of that grant carries a
//! proof in the [`HEADER`]: a JWS typed [`PROOF_TYPE`] whose header carries
//! the public key, and whose claims bind the request's method (`htm`) and
//! URL without query and fragment (`htu`), with a fresh `jti` and `iat`
//! (RFC 9449 §4.2). A token request proves itself where it is composed
//! ([`token`](crate::onward::token)). A node request carries its bound
//! token as `openehr-its`'s `Credentials::Dpop`, under the `DPoP` scheme
//! (§7.1), and the node's client asks the endpoint's [`NodeProver`] for the
//! proof over the final method and URL, with the token's hash in `ath`.
//!
//! A server that demands a nonce answers with one in [`NONCE_HEADER`]: the
//! token endpoint with `400` and [`USE_NONCE`] (RFC 9449 §8), a node with
//! `401` and a `DPoP` challenge naming it (§9). The request is then sent
//! once more, with the nonce in a new proof, to the same URL: a token
//! request by [`token`](crate::onward::token), a node request by the
//! `openehr-its` client, which answers the challenge for a client given a
//! prover. Every nonce a server sends is kept per origin and per role,
//! the token endpoint's apart from the node's even on one origin (§9), and
//! put in the next proof to that server.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::jwk::{Jwk, ThumbprintHash};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use openehr_its::rest::client::{CredentialsError, DpopProofRequest, DpopProver};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
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
    nonces: Mutex<BTreeMap<(Role, String), String>>,
}

/// The part a server plays toward a `DPoP`-bound grant, which keeps the
/// nonces of each apart (RFC 9449 §9: a nonce of the authorization server
/// and one of a resource server are never confused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Role {
    /// The authorization server: the grant's token endpoint (§8).
    Authorization,
    /// The resource server: the node (§9).
    Resource,
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

    /// A proof of a request with `method` to `url`, a server in `role`,
    /// bound to `token` when the request carries one, and naming the nonce
    /// that server sent last, when it sent one (RFC 9449 §4.2).
    ///
    /// The proof's `htu` is `url` without its query and fragment.
    pub(crate) fn prove(
        &self,
        (method, url): (&http::Method, &Url),
        role: Role,
        token: Option<&str>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let nonce = self.nonce(role, url);
        let mut htu = url.clone();
        htu.set_query(None);
        htu.set_fragment(None);
        let claims = Claims {
            jti: uuid::Uuid::new_v4().to_string(),
            htm: method.as_str(),
            htu: htu.as_str(),
            iat: jiff::Timestamp::now().as_second(),
            ath: token.map(|token| URL_SAFE_NO_PAD.encode(digest(&SHA256, token.as_bytes()))),
            nonce: nonce.as_deref(),
        };
        let mut header = Header::new(self.algorithm);
        header.typ = Some(PROOF_TYPE.to_owned());
        header.jwk = Some(self.public.clone());
        jsonwebtoken::encode(&header, &claims, &self.private)
    }

    /// The nonce the server in `role` at `url`'s origin sent last, when it
    /// sent one.
    fn nonce(&self, role: Role, url: &Url) -> Option<String> {
        self.lock()
            .get(&(role, url.origin().ascii_serialization()))
            .cloned()
    }

    /// Keeps `nonce` as the one the server in `role` at `url`'s origin sent
    /// last.
    pub(crate) fn remember(&self, role: Role, url: &Url, nonce: &str) {
        self.lock()
            .insert((role, url.origin().ascii_serialization()), nonce.to_owned());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(Role, String), String>> {
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

/// The proofs of one node's requests: a [`Prover`] for the node at one base
/// URL, as the `DpopProver` of the node's `openehr-its` client.
///
/// The client asks it for the proof of every request it sends under a bound
/// token, over the request's final method and URL and the token (RFC 9449
/// §4.2, §7.1), and hands it every nonce the node sends, which it keeps for
/// the node's origin (§9).
#[derive(Debug, Clone)]
pub struct NodeProver {
    prover: Arc<Prover>,
    base: Url,
}

impl NodeProver {
    /// The proofs of `prover` for the node at `base`.
    #[must_use]
    pub fn new(prover: Arc<Prover>, base: Url) -> Self {
        Self { prover, base }
    }
}

/// A request URI no proof can name.
#[derive(Debug, thiserror::Error)]
#[error("the request URI is not an absolute URL a DPoP proof can name")]
struct Unnamed(#[source] url::ParseError);

#[async_trait::async_trait]
impl DpopProver for NodeProver {
    async fn proof(&self, request: &DpopProofRequest<'_>) -> Result<String, CredentialsError> {
        let url = Url::parse(&request.uri().to_string())
            .map_err(|source| CredentialsError::new(Unnamed(source)))?;
        self.prover
            .prove(
                (request.method(), &url),
                Role::Resource,
                Some(request.access_token().expose_secret()),
            )
            .map_err(CredentialsError::new)
    }

    fn nonce(&self, nonce: &str) {
        self.prover.remember(Role::Resource, &self.base, nonce);
    }
}
