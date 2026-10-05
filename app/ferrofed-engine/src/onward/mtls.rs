// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Mutual TLS on an onward grant (RFC 8705, §13.4).
//!
//! The gateway authenticates by its certificate and takes access tokens
//! bound to it, for a deployment whose authorization server authenticates
//! its clients that way. A grant that authenticates with [`TlsClientAuth`] sends its `client_id`
//! and no client assertion (RFC 8705 §2): the TLS handshake on the
//! connection to the token endpoint proves the client, by a certificate the
//! server checks against its PKI ([`TlsClientAuth::Pki`], §2.1) or against
//! the certificate registered for the client ([`TlsClientAuth::SelfSigned`],
//! §2.2). A grant whose tokens are certificate-bound (§3) holds the
//! [`Thumbprint`] of that certificate and checks every token it is issued
//! against it, where the token states its binding: a `cnf` member of the
//! token response, in the form RFC 8705 §3.2 gives an introspection
//! response, or the `cnf` claim of an access token that is a JWT (§3.1). A
//! token bound to another certificate, or confirmed by another method, is
//! refused before it is cached or sent anywhere. An opaque token states no
//! binding and is sent as it came.
//!
//! The certificate is presented by the endpoint's transport, which the
//! token endpoint and the node share, so a bound token only ever travels
//! over a connection that presents the certificate it is bound to (§3).

use std::fmt;

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// The `x5t#S256` confirmation method member (RFC 8705 §3.1).
pub const X5T_S256: &str = "x5t#S256";

/// How a grant authenticates the gateway by its TLS client certificate
/// (RFC 8705 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TlsClientAuth {
    /// `tls_client_auth`: the server validates the certificate chain and
    /// matches the subject it registered for the client (§2.1).
    Pki,
    /// `self_signed_tls_client_auth`: the server matches the certificate
    /// against the ones registered for the client, with no chain validation
    /// (§2.2).
    SelfSigned,
}

impl TlsClientAuth {
    /// The method as RFC 8705 §2.1.1 and §2.2.1 register it in the "OAuth
    /// Token Endpoint Authentication Methods" registry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pki => "tls_client_auth",
            Self::SelfSigned => "self_signed_tls_client_auth",
        }
    }
}

/// The X.509 certificate SHA-256 thumbprint of RFC 8705 §3.1: the unpadded
/// base64url SHA-256 of the certificate's DER encoding.
///
/// A thumbprint is the hash of a public certificate and no secret, so
/// `Debug` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbprint(String);

/// A client certificate whose thumbprint cannot be taken.
///
/// No variant carries any part of the PEM text, which holds the private
/// key beside the certificate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ThumbprintError {
    /// The text does not read as PEM.
    #[error("the client certificate and key do not read as PEM")]
    Pem(#[source] pem::PemError),
    /// The PEM text holds no `CERTIFICATE` block.
    #[error("the client certificate and key hold no CERTIFICATE block")]
    NoCertificate,
}

impl Thumbprint {
    /// The thumbprint of the certificate whose DER encoding is `der`.
    #[must_use]
    pub fn of_certificate(der: &[u8]) -> Self {
        Self(URL_SAFE_NO_PAD.encode(digest(&SHA256, der)))
    }

    /// The thumbprint of the certificate `identity` presents: the first
    /// `CERTIFICATE` block of a PEM chain and key, the end-entity
    /// certificate a TLS client sends first (RFC 8446 §4.4.2).
    ///
    /// # Errors
    ///
    /// Returns [`ThumbprintError::Pem`] for text that does not read as PEM
    /// and [`ThumbprintError::NoCertificate`] for one with no certificate.
    pub fn of_identity(identity: &SecretString) -> Result<Self, ThumbprintError> {
        let blocks =
            pem::parse_many(identity.expose_secret().as_bytes()).map_err(ThumbprintError::Pem)?;
        blocks
            .iter()
            .find(|block| block.tag() == "CERTIFICATE")
            .map(|block| Self::of_certificate(block.contents()))
            .ok_or(ThumbprintError::NoCertificate)
    }

    /// The thumbprint as the `x5t#S256` member writes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Thumbprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A `cnf` confirmation (RFC 7800 §3.1), the members a certificate-bound
/// grant reads.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Confirmation {
    /// The certificate thumbprint the token is bound to (RFC 8705 §3.1).
    #[serde(rename = "x5t#S256")]
    x5t_s256: Option<String>,
}

/// The claims of a JWT access token a certificate-bound grant reads.
#[derive(Deserialize)]
struct Claims {
    cnf: Option<Confirmation>,
}

/// How an issued token's stated binding compares with the grant's
/// certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    /// The token states no binding: an opaque token with no `cnf` beside
    /// it.
    Unstated,
    /// Every binding the token states names `expected`.
    Confirmed,
    /// A binding the token states names another certificate, or another
    /// confirmation method.
    Mismatch,
}

/// Compares the bindings `token` and `response`, the `cnf` member of its
/// token response, state with `expected` (RFC 8705 §3.1, §3.2).
pub(crate) fn binding(
    token: &str,
    response: Option<&Confirmation>,
    expected: &Thumbprint,
) -> Binding {
    let stated: Vec<Confirmation> = response
        .cloned()
        .into_iter()
        .chain(claimed(token))
        .collect();
    if stated.is_empty() {
        return Binding::Unstated;
    }
    if stated
        .iter()
        .all(|cnf| cnf.x5t_s256.as_deref() == Some(expected.as_str()))
    {
        Binding::Confirmed
    } else {
        Binding::Mismatch
    }
}

/// The `cnf` claim of `token`, when it is a JWT that carries one (RFC 8705
/// §3.1).
fn claimed(token: &str) -> Option<Confirmation> {
    let mut parts = token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    // NOTE: RFC 6749 §1.4, an access token is opaque to the client; one whose
    // payload does not read as JWT claims is legitimately opaque, not defective.
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<Claims>(&decoded).ok()?.cnf
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use secrecy::SecretString;

    use super::{Binding, Confirmation, Thumbprint, ThumbprintError, TlsClientAuth, binding};

    fn jwt(claims: &str) -> String {
        format!(
            "{}.{}.c2ln",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"ES256"}"#),
            URL_SAFE_NO_PAD.encode(claims)
        )
    }

    #[test]
    fn the_thumbprint_is_the_unpadded_base64url_sha256_of_the_der() {
        // RFC 8705 §3.1: the SHA-256 of the empty input, base64url, no padding.
        assert_eq!(
            "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU",
            Thumbprint::of_certificate(b"").as_str()
        );
    }

    #[test]
    fn the_identity_thumbprint_is_that_of_its_first_certificate() {
        let pem = "-----BEGIN CERTIFICATE-----\nAAEC\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nAwQF\n-----END CERTIFICATE-----\n-----BEGIN PRIVATE KEY-----\nBgcI\n-----END PRIVATE KEY-----\n";
        let thumbprint = Thumbprint::of_identity(&SecretString::from(pem)).expect("the PEM reads");
        assert_eq!(Thumbprint::of_certificate(&[0, 1, 2]), thumbprint);
    }

    #[test]
    fn an_identity_without_a_certificate_names_no_key_material() {
        let pem = "-----BEGIN PRIVATE KEY-----\nQz7secretkeymaterial\n-----END PRIVATE KEY-----\n";
        let refused = Thumbprint::of_identity(&SecretString::from(pem))
            .expect_err("no certificate is refused");
        assert!(matches!(refused, ThumbprintError::NoCertificate));
        assert!(!format!("{refused} {refused:?}").contains("Qz7"));
        let refused = Thumbprint::of_identity(&SecretString::from("Qz7 not pem"));
        assert!(refused.is_err());
    }

    #[test]
    fn the_methods_are_named_as_rfc_8705_registers_them() {
        assert_eq!("tls_client_auth", TlsClientAuth::Pki.as_str());
        assert_eq!(
            "self_signed_tls_client_auth",
            TlsClientAuth::SelfSigned.as_str()
        );
    }

    #[test]
    fn a_jwt_claim_and_a_response_member_are_held_to_the_certificate() {
        let ours = Thumbprint::of_certificate(b"ours");
        let theirs = Thumbprint::of_certificate(b"theirs");
        let bound =
            |thumbprint: &Thumbprint| jwt(&format!(r#"{{"cnf":{{"x5t#S256":"{thumbprint}"}}}}"#));
        assert_eq!(Binding::Unstated, binding("opaque-token", None, &ours));
        assert_eq!(
            Binding::Unstated,
            binding(&jwt(r#"{"sub":"x"}"#), None, &ours)
        );
        assert_eq!(Binding::Confirmed, binding(&bound(&ours), None, &ours));
        assert_eq!(Binding::Mismatch, binding(&bound(&theirs), None, &ours));
        assert_eq!(
            Binding::Mismatch,
            binding(&jwt(r#"{"cnf":{"jkt":"abc"}}"#), None, &ours),
            "a token confirmed by a DPoP key is not bound to the certificate"
        );
        let response = |thumbprint: &Thumbprint| -> Confirmation {
            serde_json::from_str(&format!(r#"{{"x5t#S256":"{thumbprint}"}}"#))
                .expect("the member reads")
        };
        assert_eq!(
            Binding::Confirmed,
            binding("opaque", Some(&response(&ours)), &ours)
        );
        assert_eq!(
            Binding::Mismatch,
            binding("opaque", Some(&response(&theirs)), &ours)
        );
        assert_eq!(
            Binding::Mismatch,
            binding(&bound(&ours), Some(&response(&theirs)), &ours),
            "every stated binding must name the certificate"
        );
    }
}
