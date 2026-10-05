// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's signing keys and the JWK Set (RFC 7517 §5) that publishes
//! them.
//!
//! The `[signing]` key signs every onward client assertion and every
//! conveyance, and the previous key stays published through a rotation.
//! A key is read from PKCS#8 PEM, and its curve decides its algorithm
//! (RFC 7518 §3.4): a P-256 key signs ES256, a P-384 key ES384
//! ([`SigningKey::from_ec_pem`]). A grant whose profile admits no ES384, the
//! FAPI 2.0 grant, holds a P-256 key of its own
//! ([`SigningKey::from_p256_pem`]). Its `kid` is its RFC 7638 JWK thumbprint over
//! SHA-256, so the same key always has the same `kid` and no operator names
//! one. The previous key is published, and never signs, for one overlap
//! window from the moment the ring is built. No specification governs the
//! rotation: our own design.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet, PublicKeyUse, ThumbprintHash};
use jsonwebtoken::{Algorithm, EncodingKey};
use secrecy::{ExposeSecret, SecretString};

use crate::onward::Clock;

/// The media type of a JWK Set document (RFC 7517 §8.5.1).
pub const JWK_SET_MEDIA_TYPE: &str = "application/jwk-set+json";

/// A signing key that cannot be used.
///
/// No variant carries the key or any part of it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KeyError {
    /// The text is not an EC private key in PKCS#8 PEM.
    #[error("the key is not an EC private key in PKCS#8 PEM")]
    Pem(#[source] jsonwebtoken::errors::Error),
    /// The key is not a P-384 key, the curve ES384 signs with (RFC 7518
    /// §3.4).
    #[error("the key is not a P-384 key, the curve ES384 signs with (RFC 7518 §3.4)")]
    Curve(#[source] jsonwebtoken::errors::Error),
    /// The key is not a P-256 key, the curve ES256 signs with (RFC 7518
    /// §3.4).
    #[error("the key is not a P-256 key, the curve ES256 signs with (RFC 7518 §3.4)")]
    CurveP256(#[source] jsonwebtoken::errors::Error),
    /// The key is on neither P-256 nor P-384, the curves ES256 and ES384
    /// sign with (RFC 7518 §3.4).
    #[error("the key is on neither P-256 (ES256) nor P-384 (ES384) (RFC 7518 §3.4)")]
    CurveUnsupported(#[source] jsonwebtoken::errors::Error),
    /// The key's RFC 7638 thumbprint could not be computed.
    #[error("the RFC 7638 thumbprint of the key could not be computed")]
    Thumbprint(#[source] jsonwebtoken::errors::Error),
    /// The previous key is the current key, so a rotation would publish one
    /// key under two roles.
    #[error("the previous key is the current key (kid {kid})")]
    SameKey {
        /// The `kid` both keys have.
        kid: String,
    },
}

/// One signing key, ES384 or ES256, its public half as a JWK, and its `kid`.
///
/// `Debug` shows the `kid` and the algorithm alone.
#[derive(Clone)]
pub struct SigningKey {
    kid: String,
    algorithm: Algorithm,
    private: EncodingKey,
    public: Jwk,
}

impl SigningKey {
    /// Reads the ES384 private key `pem` holds in PKCS#8 PEM.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::Pem`] for text that is no EC private key in
    /// PKCS#8 PEM, [`KeyError::Curve`] for a key on a curve other than P-384,
    /// and [`KeyError::Thumbprint`] when its `kid` cannot be computed.
    pub fn from_pem(pem: &SecretString) -> Result<Self, KeyError> {
        Self::read(pem, Algorithm::ES384, KeyError::Curve)
    }

    /// Reads the private key `pem` holds in PKCS#8 PEM, its algorithm
    /// read from its curve: a P-256 key signs ES256, a P-384 key ES384
    /// (RFC 7518 §3.4).
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::Pem`] for text that is no EC private key in
    /// PKCS#8 PEM, [`KeyError::CurveUnsupported`] for a key on neither
    /// curve, and [`KeyError::Thumbprint`] when its `kid` cannot be
    /// computed.
    pub fn from_ec_pem(pem: &SecretString) -> Result<Self, KeyError> {
        match Self::read(pem, Algorithm::ES256, KeyError::CurveP256) {
            Err(KeyError::CurveP256(_not_p256)) => {
                Self::read(pem, Algorithm::ES384, KeyError::CurveUnsupported)
            }
            read => read,
        }
    }

    /// Reads the ES256 private key `pem` holds in PKCS#8 PEM.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::Pem`] for text that is no EC private key in
    /// PKCS#8 PEM, [`KeyError::CurveP256`] for a key on a curve other than
    /// P-256, and [`KeyError::Thumbprint`] when its `kid` cannot be computed.
    pub fn from_p256_pem(pem: &SecretString) -> Result<Self, KeyError> {
        Self::read(pem, Algorithm::ES256, KeyError::CurveP256)
    }

    /// Reads the private key `pem` holds for `algorithm`, refusing a key on
    /// another curve with `curve`.
    fn read(
        pem: &SecretString,
        algorithm: Algorithm,
        curve: fn(jsonwebtoken::errors::Error) -> KeyError,
    ) -> Result<Self, KeyError> {
        let private =
            EncodingKey::from_ec_pem(pem.expose_secret().as_bytes()).map_err(KeyError::Pem)?;
        let mut public = Jwk::from_encoding_key(&private, algorithm).map_err(curve)?;
        let kid = public
            .thumbprint(ThumbprintHash::SHA256)
            .map_err(KeyError::Thumbprint)?;
        public.common.key_id = Some(kid.clone());
        public.common.public_key_use = Some(PublicKeyUse::Signature);
        Ok(Self {
            kid,
            algorithm,
            private,
            public,
        })
    }

    /// The algorithm the key signs with: `ES256` for a P-256 key, `ES384`
    /// for a P-384 key.
    #[must_use]
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// The key's `kid`: its RFC 7638 thumbprint over SHA-256, base64url.
    #[must_use]
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// The public half, as the JWK the JWK Set publishes, with its `kid`,
    /// `use` `sig` and its `alg`.
    #[must_use]
    pub fn public(&self) -> &Jwk {
        &self.public
    }

    /// The private half, for signing.
    pub(crate) fn private(&self) -> &EncodingKey {
        &self.private
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("kid", &self.kid)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

/// The current signing key, and the previous one while a rotation's overlap
/// window lasts.
///
/// The current key signs every assertion, with its own algorithm. The
/// previous key may be on the other curve, so a rotation can move the
/// gateway from ES384 to ES256 or back. The previous key never signs, and
/// is published until the window that started when the ring was built ends,
/// so an assertion it signed before the rotation still verifies while a node
/// may hold it (RFC 7517 §5).
#[derive(Debug)]
pub struct KeyRing {
    current: SigningKey,
    previous: Option<(SigningKey, Option<Instant>)>,
    clock: Arc<dyn Clock>,
}

impl KeyRing {
    /// A ring that signs with `current` and publishes `previous` for
    /// `overlap` from now, as `clock` tells the time.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::SameKey`] when `previous` is `current`.
    pub fn new(
        current: SigningKey,
        previous: Option<SigningKey>,
        overlap: Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, KeyError> {
        if let Some(previous) = &previous
            && previous.kid == current.kid
        {
            return Err(KeyError::SameKey {
                kid: current.kid.clone(),
            });
        }
        // NOTE: no specification governs this: our own design; a window past the
        // range of the clock never ends, so the previous key stays published.
        let until = clock.now().checked_add(overlap);
        let previous = previous.map(|key| (key, until));
        Ok(Self {
            current,
            previous,
            clock,
        })
    }

    /// The key every assertion is signed with.
    #[must_use]
    pub fn current(&self) -> &SigningKey {
        &self.current
    }

    /// The previous key while its overlap window lasts, `None` once it
    /// ended or when there is none.
    #[must_use]
    pub fn previous(&self) -> Option<&SigningKey> {
        let now = self.clock.now();
        self.previous
            .as_ref()
            .filter(|(_, until)| until.is_none_or(|until| now < until))
            .map(|(key, _)| key)
    }

    /// The previous key the ring was built with, whether or not its window
    /// has ended.
    #[must_use]
    pub fn retiring(&self) -> Option<&SigningKey> {
        self.previous.as_ref().map(|(key, _)| key)
    }

    /// The JWK Set the gateway publishes now: the current key, then the
    /// previous one while its window lasts (RFC 7517 §5).
    #[must_use]
    pub fn published(&self) -> JwkSet {
        let mut keys = vec![self.current.public.clone()];
        if let Some(previous) = self.previous() {
            keys.push(previous.public.clone());
        }
        JwkSet { keys }
    }
}
