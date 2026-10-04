// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A test issuer: an authorization server the suites mint RFC 9068 access
//! tokens with, its key pairs generated per run so no private key is ever
//! committed.
//!
//! [`Issuer`] holds one or more signing keys, publishes their public halves
//! as a JWK Set (RFC 7517 §5), serves that set over HTTP from a mock
//! [`Server`], and signs whatever claims a test chooses. [`Claims`] starts
//! from a token the gateway admits for every operation, so a test changes
//! only the claim it is about. No specification governs the harness: our own
//! design.

use std::fmt;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P384_SHA384_FIXED_SIGNING, EcdsaKeyPair,
};
use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm, PublicKeyUse};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::value::RawValue;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;

/// The path a served key set answers at.
pub const JWKS_PATH: &str = "/jwks.json";

/// The scopes a default token grants: every family, every permission, every
/// resource, in the `user/` compartment.
pub const EVERY_SCOPE: &str = "user/aql-*.cruds user/composition-*.cruds user/template-*.cruds";

/// The HL7 v3 `ActReason` code system a purpose of use is coded in.
pub const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

/// A test issuer could not be set up or could not sign.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IssuerError {
    /// A key pair could not be generated.
    #[error("a key pair could not be generated")]
    Generate,
    /// The key could not be described as a JWK, or the token not signed.
    #[error("the token or the key could not be written")]
    Sign(#[source] jsonwebtoken::errors::Error),
    /// The claims could not be written as JSON.
    #[error("the claims could not be written")]
    Json(#[source] serde_json::Error),
}

/// One signing key of an issuer.
struct Key {
    /// The `kid` it is published under.
    kid: String,
    /// The algorithm it signs with.
    algorithm: Algorithm,
    /// The private half.
    encoding: EncodingKey,
    /// The public half, as published.
    jwk: Jwk,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key")
            .field("kid", &self.kid)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

/// A test issuer: its identifier, its signing keys and the key it signs with.
#[derive(Debug)]
pub struct Issuer {
    /// The `iss` its tokens carry.
    name: String,
    /// Every key it publishes; the last one signs.
    keys: Vec<Key>,
}

impl Issuer {
    /// Returns an issuer named `name` with one ES256 key, `kid` `k1`.
    ///
    /// # Errors
    /// Returns [`IssuerError`] when the key pair cannot be generated.
    pub fn new(name: impl Into<String>) -> Result<Self, IssuerError> {
        let mut issuer = Self {
            name: name.into(),
            keys: Vec::new(),
        };
        issuer.add_key("k1", Algorithm::ES256)?;
        Ok(issuer)
    }

    /// Returns the `iss` its tokens carry.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Adds a key of `algorithm` (ES256 or ES384) published as `kid`, which
    /// signs from now on; the earlier keys stay published.
    ///
    /// # Errors
    /// Returns [`IssuerError`] when the key pair cannot be generated, or the
    /// algorithm is not one this issuer generates.
    pub fn add_key(&mut self, kid: &str, algorithm: Algorithm) -> Result<(), IssuerError> {
        let curve = match algorithm {
            Algorithm::ES256 => &ECDSA_P256_SHA256_FIXED_SIGNING,
            Algorithm::ES384 => &ECDSA_P384_SHA384_FIXED_SIGNING,
            _ => return Err(IssuerError::Generate),
        };
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(curve, &SystemRandom::new())
            .map_err(|_unspecified| IssuerError::Generate)?;
        let encoding = EncodingKey::from_ec_der(pkcs8.as_ref());
        let mut jwk = Jwk::from_encoding_key(&encoding, algorithm).map_err(IssuerError::Sign)?;
        jwk.common.key_id = Some(kid.to_owned());
        jwk.common.public_key_use = Some(PublicKeyUse::Signature);
        jwk.common.key_algorithm = Some(match algorithm {
            Algorithm::ES384 => KeyAlgorithm::ES384,
            _ => KeyAlgorithm::ES256,
        });
        self.keys.push(Key {
            kid: kid.to_owned(),
            algorithm,
            encoding,
            jwk,
        });
        Ok(())
    }

    /// Stops publishing every key but the one that signs.
    pub fn retire_old_keys(&mut self) {
        let keep = self.keys.len().saturating_sub(1);
        self.keys.drain(..keep);
    }

    /// Returns the published key set.
    #[must_use]
    pub fn jwks(&self) -> JwkSet {
        JwkSet {
            keys: self.keys.iter().map(|key| key.jwk.clone()).collect(),
        }
    }

    /// Returns the published key set as JSON.
    ///
    /// # Errors
    /// Returns [`IssuerError::Json`] when the set cannot be written.
    pub fn jwks_json(&self) -> Result<String, IssuerError> {
        serde_json::to_string(&self.jwks()).map_err(IssuerError::Json)
    }

    /// Starts a mock server that answers `GET` [`JWKS_PATH`] with the key set
    /// as it is now.
    ///
    /// # Errors
    /// Returns [`IssuerError::Json`] when the set cannot be written.
    pub async fn serve(&self) -> Result<Server, IssuerError> {
        let server = Server::start().await;
        self.publish(&server).await?;
        Ok(server)
    }

    /// Mounts the key set as it is now on `server`, ahead of any set mounted
    /// before.
    ///
    /// # Errors
    /// Returns [`IssuerError::Json`] when the set cannot be written.
    pub async fn publish(&self, server: &Server) -> Result<(), IssuerError> {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(self.jwks_json()?),
            )
            .mount(server)
            .await;
        Ok(())
    }

    /// Returns the header the signing key writes: its algorithm, its `kid`,
    /// and `typ` `at+jwt` (RFC 9068 §2.1).
    #[must_use]
    pub fn header(&self) -> Header {
        let Some(key) = self.keys.last() else {
            return Header::default();
        };
        let mut header = Header::new(key.algorithm);
        header.kid = Some(key.kid.clone());
        header.typ = Some(String::from("at+jwt"));
        header
    }

    /// Signs `claims` under the signing key's header.
    ///
    /// # Errors
    /// Returns [`IssuerError`] when the token cannot be signed.
    pub fn mint(&self, claims: &Claims) -> Result<String, IssuerError> {
        self.mint_with(&self.header(), claims)
    }

    /// Signs `claims` under `header`, which a test may have changed.
    ///
    /// # Errors
    /// Returns [`IssuerError`] when the token cannot be signed.
    pub fn mint_with(&self, header: &Header, claims: &Claims) -> Result<String, IssuerError> {
        let payload = serde_json::to_string(claims).map_err(IssuerError::Json)?;
        self.sign(header, &payload)
    }

    /// Signs the JSON text `payload`, whatever it holds, under `header`.
    ///
    /// # Errors
    /// Returns [`IssuerError`] when `payload` is no JSON or the token cannot
    /// be signed.
    pub fn sign(&self, header: &Header, payload: &str) -> Result<String, IssuerError> {
        let raw = RawValue::from_string(payload.to_owned()).map_err(IssuerError::Json)?;
        let Some(key) = self.keys.last() else {
            return Err(IssuerError::Generate);
        };
        jsonwebtoken::encode(header, &raw, &key.encoding).map_err(IssuerError::Sign)
    }
}

/// The claims of a test token: by default, every claim RFC 9068 §2.2
/// requires, every scope, and the purpose of use `TREAT` in the IHE IUA
/// extension.
#[derive(Debug, Clone, Serialize)]
pub struct Claims {
    /// `iss`.
    pub iss: String,
    /// `sub`.
    pub sub: String,
    /// `aud`.
    pub aud: String,
    /// `exp`, in seconds since the epoch.
    pub exp: i64,
    /// `nbf`, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// `iat`.
    pub iat: i64,
    /// `jti`.
    pub jti: String,
    /// `client_id`.
    pub client_id: String,
    /// `scope`, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The IHE IUA extension, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,
    /// RFC 9396 `authorization_details`, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_details: Option<Vec<AuthorizationDetail>>,
    /// The SMART on openEHR `ehrId` of the launch context, when set.
    #[serde(rename = "ehrId", skip_serializing_if = "Option::is_none")]
    pub ehr_id: Option<String>,
}

impl Claims {
    /// Returns the default claims of a token `issuer` issues for `audience`,
    /// valid for ten minutes from now.
    #[must_use]
    pub fn new(issuer: &str, audience: &str) -> Self {
        let now = jiff::Timestamp::now().as_second();
        Self {
            iss: issuer.to_owned(),
            sub: String::from("synthetic-caller"),
            aud: audience.to_owned(),
            exp: now.saturating_add(600),
            nbf: None,
            iat: now,
            jti: uuid::Uuid::new_v4().to_string(),
            client_id: String::from("synthetic-client"),
            scope: Some(EVERY_SCOPE.to_owned()),
            extensions: Some(Extensions::treatment()),
            authorization_details: None,
            ehr_id: None,
        }
    }
}

/// The `extensions` claim of IHE IUA.
#[derive(Debug, Clone, Serialize)]
pub struct Extensions {
    /// `ihe_iua`.
    pub ihe_iua: IheIua,
}

impl Extensions {
    /// The IUA extension declaring the purpose `TREAT` for the synthetic
    /// organisation `urn:oid:2.999.7`.
    #[must_use]
    pub fn treatment() -> Self {
        Self {
            ihe_iua: IheIua {
                subject_organization_id: Some(String::from("urn:oid:2.999.7")),
                purpose_of_use: vec![Coding {
                    system: ACT_REASON.to_owned(),
                    code: String::from("TREAT"),
                }],
            },
        }
    }
}

/// The `ihe_iua` extension.
#[derive(Debug, Clone, Serialize)]
pub struct IheIua {
    /// `subject_organization_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_organization_id: Option<String>,
    /// `purpose_of_use`, an array of FHIR `Coding`.
    pub purpose_of_use: Vec<Coding>,
}

/// A FHIR `Coding`.
#[derive(Debug, Clone, Serialize)]
pub struct Coding {
    /// `system`.
    pub system: String,
    /// `code`.
    pub code: String,
}

/// One RFC 9396 authorization detail with a `purpose_of_use`.
#[derive(Debug, Clone, Serialize)]
pub struct AuthorizationDetail {
    /// `type`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `purpose_of_use`, written `system|code`.
    pub purpose_of_use: String,
}

#[cfg(test)]
mod tests {
    use super::{Claims, Issuer};
    use jsonwebtoken::{DecodingKey, Validation};

    #[test]
    fn a_minted_token_verifies_under_the_published_key() {
        let issuer = Issuer::new("https://issuer.example").unwrap();
        let token = issuer
            .mint(&Claims::new(issuer.name(), "urn:example:gateway"))
            .unwrap();
        let jwks = issuer.jwks();
        let key = DecodingKey::from_jwk(jwks.find("k1").unwrap()).unwrap();
        let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.set_audience(&["urn:example:gateway"]);
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(Some("at+jwt"), header.typ.as_deref());
        jsonwebtoken::decode::<serde::de::IgnoredAny>(&token, &key, &validation).unwrap();
    }

    #[test]
    fn a_new_key_signs_and_the_old_one_stays_published_until_retired() {
        let mut issuer = Issuer::new("https://issuer.example").unwrap();
        issuer
            .add_key("k2", jsonwebtoken::Algorithm::ES384)
            .unwrap();
        assert_eq!(Some("k2"), issuer.header().kid.as_deref());
        assert_eq!(2, issuer.jwks().keys.len());
        issuer.retire_old_keys();
        assert_eq!(1, issuer.jwks().keys.len());
        assert!(issuer.jwks().find("k2").is_some());
    }
}
