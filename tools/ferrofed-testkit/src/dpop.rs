// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness side of `DPoP` (RFC 9449): the check a test device makes of a
//! proof, and the challenge a node answers a proof without its nonce with.
//!
//! [`verify`] holds a proof to RFC 9449 §4.3: typed `dpop+jwt`, signed ES256
//! or ES384 by the public key its header carries, binding the request's
//! method and URL without query and fragment, issued within a minute of
//! now, with a `jti`, and, for a request carrying an access token, the
//! token's SHA-256 in `ath` (§7.1). [`challenge`] is the `401` a node sends
//! a proof that lacks the nonce it demands (§9). No specification governs
//! the device: our own design.

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::jwk::ThumbprintHash;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use wiremock::ResponseTemplate;

/// The header a proof travels in.
pub const HEADER: &str = "DPoP";

/// The header a server sends a nonce in.
pub const NONCE_HEADER: &str = "DPoP-Nonce";

/// How far a proof's `iat` may lie from now, in seconds.
pub const IAT_WINDOW_S: i64 = 60;

/// The claims of a proof, as a test device reads them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Proof {
    /// The proof's unique id.
    pub jti: String,
    /// The method it binds.
    pub htm: String,
    /// The URL it binds.
    pub htu: String,
    /// When it was made, in seconds since the epoch.
    pub iat: i64,
    /// The hash of the access token it binds, when it binds one.
    pub ath: Option<String>,
    /// The nonce it names, when it names one.
    pub nonce: Option<String>,
}

/// A proof that verified, with the RFC 7638 thumbprint of its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The thumbprint of the key that signed it, the `jkt` of a token bound
    /// to it (RFC 9449 §6.1).
    pub jkt: String,
    /// Its claims.
    pub proof: Proof,
}

/// The `ath` of `token`: the base64url SHA-256 of its text (RFC 9449 §4.2).
#[must_use]
pub fn ath(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, token.as_bytes()))
}

/// Verifies `proof` as a proof of a request with `method` to `htu`,
/// carrying `token` when it carries one.
///
/// # Errors
///
/// Returns the reason the proof is refused.
pub fn verify(
    proof: &str,
    method: &str,
    htu: &str,
    token: Option<&str>,
) -> Result<Verified, String> {
    let header = jsonwebtoken::decode_header(proof).map_err(|error| format!("header: {error}"))?;
    if header.typ.as_deref() != Some("dpop+jwt") {
        return Err(format!("the proof is typed {:?}", header.typ));
    }
    if !matches!(header.alg, Algorithm::ES256 | Algorithm::ES384) {
        return Err(format!("the proof is signed with {:?}", header.alg));
    }
    let jwk = header.jwk.ok_or("the proof carries no jwk")?;
    let key = DecodingKey::from_jwk(&jwk).map_err(|error| format!("jwk: {error}"))?;
    let mut validation = Validation::new(header.alg);
    validation.validate_exp = false;
    validation.validate_aud = false;
    validation.set_required_spec_claims::<&str>(&[]);
    let claims = jsonwebtoken::decode::<Proof>(proof, &key, &validation)
        .map_err(|error| format!("verification: {error}"))?
        .claims;
    if claims.htm != method {
        return Err(format!("the proof binds {} for {method}", claims.htm));
    }
    if claims.htu != htu {
        return Err(format!("the proof binds {} for {htu}", claims.htu));
    }
    let skew = jiff::Timestamp::now()
        .as_second()
        .saturating_sub(claims.iat)
        .abs();
    if skew > IAT_WINDOW_S {
        return Err(format!("the proof was made {skew} s from now"));
    }
    if claims.jti.is_empty() {
        return Err("the proof's jti is empty".to_owned());
    }
    match (token, claims.ath.as_deref()) {
        (Some(token), Some(hash)) if hash == ath(token) => {}
        (Some(_), _) => return Err("the proof's ath is not the token's hash".to_owned()),
        (None, Some(_)) => return Err("the proof binds a token the request lacks".to_owned()),
        (None, None) => {}
    }
    let jkt = jwk
        .thumbprint(ThumbprintHash::SHA256)
        .map_err(|error| format!("thumbprint: {error}"))?;
    Ok(Verified { jkt, proof: claims })
}

/// The `401` a node answers a proof without its nonce with: a `DPoP`
/// challenge naming `use_dpop_nonce`, and `nonce` in [`NONCE_HEADER`]
/// (RFC 9449 §9).
#[must_use]
pub fn challenge(nonce: &str) -> ResponseTemplate {
    ResponseTemplate::new(401)
        .insert_header(
            "WWW-Authenticate",
            "DPoP error=\"use_dpop_nonce\", error_description=\"a nonce is required\"",
        )
        .insert_header(NONCE_HEADER, nonce)
}

/// A test client's `DPoP` key, which signs the proofs of its requests and
/// whose thumbprint the tokens bound to it name (RFC 9449 §4.2, §6.1).
///
/// `Debug` shows the thumbprint alone.
pub struct Holder {
    /// The private key.
    private: jsonwebtoken::EncodingKey,
    /// The public key each proof carries in its header.
    public: jsonwebtoken::jwk::Jwk,
    /// The public key's RFC 7638 thumbprint.
    jkt: String,
}

impl std::fmt::Debug for Holder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Holder")
            .field("jkt", &self.jkt)
            .finish_non_exhaustive()
    }
}

/// A test client's `DPoP` key could not be made.
#[derive(Debug, thiserror::Error)]
#[error("the DPoP test key could not be made")]
pub struct HolderError(#[source] Box<dyn std::error::Error + Send + Sync + 'static>);

impl Holder {
    /// Returns a holder of a fresh P-256 key, signing ES256.
    ///
    /// # Errors
    ///
    /// Returns [`HolderError`] when the key cannot be generated or read.
    pub fn generate() -> Result<Self, HolderError> {
        let pem = crate::oauth::Curve::P256
            .pem()
            .map_err(|source| HolderError(Box::new(source)))?;
        let private = jsonwebtoken::EncodingKey::from_ec_pem(pem.as_bytes())
            .map_err(|source| HolderError(Box::new(source)))?;
        let public = jsonwebtoken::jwk::Jwk::from_encoding_key(&private, Algorithm::ES256)
            .map_err(|source| HolderError(Box::new(source)))?;
        let jkt = public
            .thumbprint(ThumbprintHash::SHA256)
            .map_err(|source| HolderError(Box::new(source)))?;
        Ok(Self {
            private,
            public,
            jkt,
        })
    }

    /// Returns the key's thumbprint, the `cnf.jkt` of a token bound to it.
    #[must_use]
    pub fn jkt(&self) -> &str {
        &self.jkt
    }

    /// Returns a fresh proof of a request with `method` to `htu`, a URL with
    /// no query or fragment, carrying `token`, with its own `jti`.
    ///
    /// # Errors
    ///
    /// Returns [`HolderError`] when the proof cannot be signed.
    pub fn prove(&self, method: &str, htu: &str, token: &str) -> Result<String, HolderError> {
        let mut header = jsonwebtoken::Header::new(Algorithm::ES256);
        header.typ = Some(String::from("dpop+jwt"));
        header.jwk = Some(self.public.clone());
        let claims = serde_json::json!({
            "jti": uuid::Uuid::new_v4().to_string(),
            "htm": method,
            "htu": htu,
            "iat": jiff::Timestamp::now().as_second(),
            "ath": ath(token),
        });
        jsonwebtoken::encode(&header, &claims, &self.private)
            .map_err(|source| HolderError(Box::new(source)))
    }
}
