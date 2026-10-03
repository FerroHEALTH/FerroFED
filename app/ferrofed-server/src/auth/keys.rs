// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One issuer's JWK Set (RFC 7517 §5), held between fetches.
//!
//! The set is fetched, or read from its file, when a token first needs it,
//! and again once it is older than its maximum age. A token naming a key the
//! held set does not have fetches the set once more, so a rotation at the
//! issuer is picked up, but at most once per refetch interval, so a flood of
//! tokens naming unknown keys cannot turn the gateway against the issuer.
//! One fetch runs at a time per set. A set that cannot be had is a
//! [`KeyError::Unavailable`], and a failed attempt is not repeated inside the
//! refetch interval either. No specification governs the cache: our own
//! design.

use std::sync::Arc;
use std::time::Duration;

use jsonwebtoken::jwk::{AlgorithmParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse};
use jsonwebtoken::{Algorithm, DecodingKey};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::auth::fetch::{FetchError, Fetcher, MAX_BODY};
use crate::config::auth::KeySource;

/// Why no key verifies a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KeyError {
    /// The key set cannot be had.
    #[error("the key set cannot be had")]
    Unavailable(#[source] FetchError),
    /// The key set holds no key the token names, after a refetch where one
    /// was allowed.
    #[error("the key set holds no key the token names")]
    Unknown,
    /// The key the token names cannot verify its algorithm.
    #[error("the key the token names cannot verify its algorithm")]
    Unusable,
}

/// One issuer's key set, with its fetch policy.
#[derive(Debug)]
pub(super) struct KeySet {
    /// Where the set is read from.
    source: KeySource,
    /// How long a held set is used.
    max_age: Duration,
    /// The least time between two fetches.
    refetch: Duration,
    /// The held set, behind the lock one fetch at a time takes.
    held: Mutex<Held>,
}

/// What a [`KeySet`] holds between requests.
#[derive(Debug, Default)]
struct Held {
    /// The set last fetched.
    set: Option<Arc<JwkSet>>,
    /// When it was fetched.
    fetched: Option<Instant>,
    /// When the last fetch was tried, whether or not it succeeded.
    tried: Option<Instant>,
}

impl KeySet {
    /// Returns an empty set read from `source`.
    pub(super) fn new(source: KeySource, max_age: Duration, refetch: Duration) -> Self {
        Self {
            source,
            max_age,
            refetch,
            held: Mutex::new(Held::default()),
        }
    }

    /// Returns the key that verifies a token signed with `algorithm` under
    /// `kid`.
    ///
    /// A token with no `kid` is verified by the set's only key, and by none
    /// when the set holds several (RFC 7515 §4.1.4).
    pub(super) async fn key(
        &self,
        kid: Option<&str>,
        algorithm: Algorithm,
        fetcher: &Fetcher,
    ) -> Result<DecodingKey, KeyError> {
        let mut held = self.held.lock().await;
        let now = Instant::now();
        let fresh = held
            .fetched
            .is_some_and(|at| now.saturating_duration_since(at) < self.max_age);
        let set = match (&held.set, fresh) {
            (Some(set), true) => Arc::clone(set),
            _ => self.refresh(&mut held, now, fetcher).await?,
        };
        if let Some(key) = select(&set, kid, algorithm)? {
            return Ok(key);
        }
        let refetchable = held
            .tried
            .is_none_or(|at| now.saturating_duration_since(at) >= self.refetch);
        if !refetchable {
            return Err(KeyError::Unknown);
        }
        let set = self.refresh(&mut held, now, fetcher).await?;
        select(&set, kid, algorithm)?.ok_or(KeyError::Unknown)
    }

    /// Fetches the set into `held`, unless the last attempt is younger than
    /// the refetch interval and failed.
    async fn refresh(
        &self,
        held: &mut Held,
        now: Instant,
        fetcher: &Fetcher,
    ) -> Result<Arc<JwkSet>, KeyError> {
        let recent = held
            .tried
            .is_some_and(|at| now.saturating_duration_since(at) < self.refetch);
        if recent && held.fetched != held.tried {
            return Err(KeyError::Unavailable(FetchError::Backoff));
        }
        held.tried = Some(now);
        let set = Arc::new(self.load(fetcher).await.map_err(KeyError::Unavailable)?);
        held.set = Some(Arc::clone(&set));
        held.fetched = Some(now);
        Ok(set)
    }

    /// Reads the set from its source.
    async fn load(&self, fetcher: &Fetcher) -> Result<JwkSet, FetchError> {
        let bytes = match &self.source {
            KeySource::Uri(url) => fetcher.get(url).await?,
            KeySource::File(path) => {
                let bytes = tokio::fs::read(path).await.map_err(FetchError::File)?;
                if bytes.len() > MAX_BODY {
                    return Err(FetchError::TooLarge);
                }
                bytes
            }
            KeySource::Set(set) => return Ok(set.clone()),
        };
        serde_json::from_slice(&bytes).map_err(FetchError::Malformed)
    }
}

/// The key of `set` that verifies `algorithm` under `kid`, `None` when the
/// set has no such key, or [`KeyError::Unusable`] when the key it names
/// cannot verify `algorithm`.
fn select(
    set: &JwkSet,
    kid: Option<&str>,
    algorithm: Algorithm,
) -> Result<Option<DecodingKey>, KeyError> {
    let jwk = match (kid, set.keys.as_slice()) {
        (Some(kid), _) => set.find(kid),
        (None, [only]) => Some(only),
        (None, _) => None,
    };
    jwk.map(|jwk| usable(jwk, algorithm)).transpose()
}

/// The decoding key `jwk` gives for `algorithm`, refused when it is a
/// symmetric key, is declared for another use or another algorithm, or is of
/// another key type (RFC 8725 §3.1, RFC 7517 §4.2, §4.4).
fn usable(jwk: &Jwk, algorithm: Algorithm) -> Result<DecodingKey, KeyError> {
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|key_use| *key_use != PublicKeyUse::Signature)
    {
        return Err(KeyError::Unusable);
    }
    if let Some(declared) = jwk.common.key_algorithm
        && declared_algorithm(declared) != Some(algorithm)
    {
        return Err(KeyError::Unusable);
    }
    let fits = match &jwk.algorithm {
        AlgorithmParameters::EllipticCurve(_) => {
            matches!(algorithm, Algorithm::ES256 | Algorithm::ES384)
        }
        AlgorithmParameters::RSA(_) => matches!(algorithm, Algorithm::RS256 | Algorithm::PS256),
        _ => false,
    };
    if !fits {
        return Err(KeyError::Unusable);
    }
    DecodingKey::from_jwk(jwk).map_err(|_unusable| KeyError::Unusable)
}

/// The signature algorithm a JWK's `alg` declares, among those the gate
/// verifies.
fn declared_algorithm(declared: KeyAlgorithm) -> Option<Algorithm> {
    match declared {
        KeyAlgorithm::ES256 => Some(Algorithm::ES256),
        KeyAlgorithm::ES384 => Some(Algorithm::ES384),
        KeyAlgorithm::PS256 => Some(Algorithm::PS256),
        KeyAlgorithm::RS256 => Some(Algorithm::RS256),
        _ => None,
    }
}
