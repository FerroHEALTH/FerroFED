// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness authorization server acting for NCPeH-A, and the SAML
//! assertions a test hands it.
//!
//! **This is a test device, not an authorization server.** It implements the
//! exchange profile FerroFED publishes for the national authorization server
//! behind a MyHealth@EU contact point (FerroHEALTH/FerroFED#1620): an RFC 8693
//! token exchange whose `subject_token` is the HP Identity Assertion,
//! base64url, of type `urn:ietf:params:oauth:token-type:saml2`, and whose
//! extension parameter `trc_token`, of the same type, is the Treatment
//! Relationship Confirmation; a discovery-phase exchange sends none.
//!
//! The assertions are `sender-vouches` and name no presenter (SAML Profile
//! 10.0.0 §2.1), so the server first authenticates the national connector
//! ([`CLIENT_ID`] with [`CONNECTOR_SECRET`], `client_secret_basic`, RFC 6749
//! §2.3.1) and refuses any other client `invalid_client` (§5.2). Both
//! assertions are then checked by `ferrohealth-smart`'s
//! [`Validator::exchange`]: the enveloped signature by a trusted seal at or
//! above the eHDSI Cryptographic Algorithms 9.0.0 §2 floor, the eHDSI SAML
//! Profile 10.0.0 §2 and §4 checks with the TRC's link to the identity
//! assertion, and the replay guard, which admits each exchange once inside
//! its window. The token it issues carries the `ehdsi` claim of
//! [`Exchange::claim`](ferrohealth_smart::federation::assertion::Exchange::claim),
//! is bound to the connector's certificate (RFC 8705 §3.1), and lives no
//! longer than [`Exchange::ends`](ferrohealth_smart::federation::assertion::Exchange::ends).
//! An assertion refusal is RFC 6749 §5.2 `invalid_grant` whose description
//! leads with the MyHealth@EU Requirements Catalogue 10.0.0 §1.25 code. The
//! parameter names `trc_token` and `trc_token_type` are FerroFED's own
//! design; RFC 6749 §8.2 lets a profile define them.
//!
//! The assertions are built from the SAML Profile 10.0.0 §2.1, §2.3 and §4
//! tables with invented values ([`Assertion`]) and signed at run time by a
//! seal certificate generated for the test ([`Seal`]), so no private key is
//! ever committed.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the issued token's claims and the exchange's answers are written as JSON"
)]

use std::fmt;
use std::future::Future as _;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use aws_lc_rs::digest;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ferrohealth_smart::federation::assertion::replay::MemoryReplayGuard;
use ferrohealth_smart::federation::assertion::signer::{SignerError, TrustedSigner};
use ferrohealth_smart::federation::assertion::{self, ExchangeError, Validator};
use jiff::Timestamp;
use rcgen::{
    CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose, PKCS_RSA_SHA256, RsaKeySize,
};
use serde_json::{Value, json};
use url::form_urlencoded;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::issuer::{Claims, Issuer};
use crate::mock::Server;
use crate::national_connector::{CLIENT_ID, cnf};

/// The RFC 8693 §2.1 `grant_type` of a token exchange.
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The RFC 8693 §3 token type of a SAML 2.0 assertion, the type of both
/// `subject_token` and `trc_token`.
pub const SAML2: &str = "urn:ietf:params:oauth:token-type:saml2";

/// The cross-border audience of both assertions (SAML Profile 10.0.0 §2.1).
pub const X_BORDER: &str = "urn:ehdsi:assertions.audience:x-border";

/// NCP-B's identity provider, the issuer of both assertions and the subject
/// of the seal that signs them (SAML Profile 10.0.0 §1.2.2).
pub const NCP_B: &str = "https://ncp-b.example.test/sts";

/// The professional's `NameID`, the token's `professional.hp_identifier`.
pub const HP_ID: &str = "urn:oid:2.999.9.81";

/// The professional's full name, the identity assertion's `subject-id`.
pub const HP_NAME: &str = "Qz9-exchange-subject-name";

/// The provider's `organization-id`, a `urn:oid:` identifier as SAML Profile
/// 10.0.0 §2.3 requires.
pub const ORGANIZATION_ID: &str = "urn:oid:2.999.9.82";

/// The client secret the harness authenticates the national connector by
/// (`client_secret_basic`, RFC 6749 §2.3.1), an invented value.
pub const CONNECTOR_SECRET: &str = "synthetic-connector-secret";

/// The longest a token the harness issues lives when neither assertion ends
/// sooner, in seconds (no specification governs this: our own design).
pub const LIFETIME: i64 = 300;

/// The SAML 2.0 assertion namespace.
const SAML: &str = "urn:oasis:names:tc:SAML:2.0:assertion";
/// The XML-DSig namespace.
const DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
/// The `rsa-sha256` signature method (RFC 6931 §2.3.2).
const RSA_SHA256: &str = "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256";
/// The `sha256` digest method (XML Encryption 1.1 §5.7.2).
const SHA256: &str = "http://www.w3.org/2001/04/xmlenc#sha256";
/// The ISCO code system the eHDSI roles are a subset of (SAML Profile 10.0.0
/// §2.3).
const ISCO: &str = "2.16.840.1.113883.2.9.6.2.7";

/// A seal could not be made or could not sign.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SealError {
    /// The key or the certificate could not be generated.
    #[error("the seal's key or certificate could not be generated")]
    Generate(#[source] rcgen::Error),
    /// The generated key could not be read for signing.
    #[error("the seal's key could not be read for signing")]
    Key,
    /// The signature could not be made.
    #[error("the seal could not sign")]
    Sign,
}

/// The RSA key a [`Seal`] is generated over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SealKey {
    /// 3072 bits, above the SOG-IS ACM 1.3 floor of 3000 (Cryptographic
    /// Algorithms 9.0.0 §2).
    Rsa3072,
    /// 2048 bits, below that floor.
    Rsa2048,
}

/// A seal certificate of NCP-B generated for the test (eHDSI X.509
/// Certificates Profile 7.0.0 §3.1), with its private key.
pub struct Seal {
    der: Vec<u8>,
    key: RsaKeyPair,
}

impl fmt::Debug for Seal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Seal")
            .field("modulus_bytes", &self.key.public_modulus_len())
            .finish_non_exhaustive()
    }
}

impl Seal {
    /// A self-signed seal of [`NCP_B`] over an RSA key of `key`.
    ///
    /// # Errors
    ///
    /// Returns [`SealError::Generate`] when the key or the certificate
    /// cannot be generated, and [`SealError::Key`] when the key cannot be
    /// read back for signing.
    pub fn new(key: SealKey) -> Result<Self, SealError> {
        let size = match key {
            SealKey::Rsa3072 => RsaKeySize::_3072,
            SealKey::Rsa2048 => RsaKeySize::_2048,
        };
        let key = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, size).map_err(SealError::Generate)?;
        let mut params = CertificateParams::new(Vec::new()).map_err(SealError::Generate)?;
        params.distinguished_name.push(DnType::CountryName, "BE");
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Invented NCP-B");
        params
            .distinguished_name
            .push(DnType::CommonName, "Invented NCP-B seal");
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.is_ca = IsCa::ExplicitNoCa;
        let der = params
            .self_signed(&key)
            .map_err(SealError::Generate)?
            .der()
            .to_vec();
        let key = RsaKeyPair::from_pkcs8(&key.serialize_der()).map_err(|_unread| SealError::Key)?;
        Ok(Self { der, key })
    }

    /// The seal as the validator trusts it: the certificate of [`NCP_B`].
    ///
    /// # Errors
    ///
    /// Returns the [`SignerError`] of a certificate `ferrohealth-smart`
    /// refuses as a signer, such as `ShortKey` for an RSA modulus below the
    /// 3000 bits of SOG-IS ACM 1.3 (Cryptographic Algorithms 9.0.0 §2).
    pub fn trusted(&self) -> Result<TrustedSigner, SignerError> {
        TrustedSigner::from_der(NCP_B, self.der.clone())
    }

    /// `assertion`, rendered and signed with an enveloped `rsa-sha256`
    /// signature over its exclusive canonical form (Cryptographic Algorithms
    /// 9.0.0 §4.1), the certificate in its `ds:KeyInfo`.
    ///
    /// # Errors
    ///
    /// Returns [`SealError::Sign`] when the signature cannot be made.
    pub fn sign(&self, assertion: &Assertion) -> Result<String, SealError> {
        let unsigned = assertion.render();
        let covered = unsigned.replace("{SIG}", "");
        let digest_value = STANDARD.encode(digest::digest(&digest::SHA256, covered.as_bytes()));
        let body = format!(
            concat!(
                r#"<ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"></ds:CanonicalizationMethod>"#,
                r#"<ds:SignatureMethod Algorithm="{method}"></ds:SignatureMethod>"#,
                r##"<ds:Reference URI="#{id}"><ds:Transforms>"##,
                r#"<ds:Transform Algorithm="http://www.w3.org/2000/09/xmldsig#enveloped-signature"></ds:Transform>"#,
                r#"<ds:Transform Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"></ds:Transform>"#,
                r#"</ds:Transforms><ds:DigestMethod Algorithm="{digest}"></ds:DigestMethod>"#,
                r#"<ds:DigestValue>{value}</ds:DigestValue></ds:Reference>"#
            ),
            method = RSA_SHA256,
            id = assertion.id,
            digest = SHA256,
            value = digest_value,
        );
        let canonical = format!(r#"<ds:SignedInfo xmlns:ds="{DSIG}">{body}</ds:SignedInfo>"#);
        let mut signature = vec![0; self.key.public_modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                canonical.as_bytes(),
                &mut signature,
            )
            .map_err(|_unspecified| SealError::Sign)?;
        let value = STANDARD.encode(signature);
        let certificate = STANDARD.encode(&self.der);
        let element = format!(
            r#"<ds:Signature xmlns:ds="{DSIG}"><ds:SignedInfo>{body}</ds:SignedInfo><ds:SignatureValue>{value}</ds:SignatureValue><ds:KeyInfo><ds:X509Data><ds:X509Certificate>{certificate}</ds:X509Certificate></ds:X509Data></ds:KeyInfo></ds:Signature>"#
        );
        Ok(unsigned.replace("{SIG}", &element))
    }
}

/// An assertion as a test builds it: the parts of SAML Profile 10.0.0 §2.1
/// and §4.1 a case changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    /// `@ID`, a UUID.
    pub id: String,
    /// `Conditions/@NotBefore`, in seconds since the epoch.
    pub not_before: i64,
    /// `Conditions/@NotOnOrAfter`, in seconds since the epoch.
    pub not_on_or_after: i64,
    /// The `@ID` of the identity assertion a TRC's `Advice/AssertionIDRef`
    /// names; `None` for an identity assertion.
    pub names: Option<String>,
    /// `AuthnContextClassRef`.
    pub authn_class: String,
    /// Each `saml:Attribute`, rendered.
    pub attributes: Vec<String>,
}

impl Assertion {
    /// An HP Identity Assertion of SAML Profile 10.0.0 §2.1 and §2.3 with
    /// the `@ID` `id`, valid from a minute before `now` for three hours: a
    /// medical doctor (ISCO `221`) of [`ORGANIZATION_ID`] at a hospital, for
    /// the purpose of use `TREATMENT`, authenticated by smartcard.
    #[must_use]
    pub fn identity(id: &str, now: i64) -> Self {
        Self {
            id: id.to_owned(),
            not_before: now.saturating_sub(60),
            not_on_or_after: now.saturating_add(3 * 3600),
            names: None,
            authn_class: String::from("urn:oasis:names:tc:SAML:2.0:ac:classes:SmartcardPKI"),
            attributes: vec![
                attribute("urn:ihe:iti:xca:2010:homeCommunityId", "urn:oid:2.999.9.83"),
                attribute("urn:oasis:names:tc:xspa:1.0:subject:subject-id", HP_NAME),
                coded("urn:oasis:names:tc:xacml:2.0:subject:role", "Role", "221"),
                attribute(
                    "urn:oasis:names:tc:xspa:1.0:subject:organization-id",
                    ORGANIZATION_ID,
                ),
                attribute(
                    "urn:ehdsi:names:subject:healthcare-facility-type",
                    "Hospital",
                ),
                coded(
                    "urn:oasis:names:tc:xspa:1.0:subject:purposeofuse",
                    "PurposeOfUse",
                    "TREATMENT",
                ),
                attribute(
                    "urn:oasis:names:tc:xspa:1.0:environment:locality",
                    "Qz9-exchange-point-of-care",
                ),
            ],
        }
    }

    /// A Treatment Relationship Confirmation of SAML Profile 10.0.0 §4 with
    /// the `@ID` `id`, linked to the identity assertion `identity`, valid
    /// from a minute before `now` for an hour, binding the patient `value`
    /// assigned under the ISO OID `oid`, written as the XCPD handshake's CX
    /// (§4.3).
    #[must_use]
    pub fn treatment(id: &str, identity: &str, (oid, value): (&str, &str), now: i64) -> Self {
        Self {
            id: id.to_owned(),
            not_before: now.saturating_sub(60),
            not_on_or_after: now.saturating_add(3600),
            names: Some(identity.to_owned()),
            authn_class: String::from("urn:oasis:names:tc:SAML:2.0:ac:classes:PreviousSession"),
            attributes: vec![attribute(
                "urn:oasis:names:tc:xspa:1.0:subject:subject-id",
                &format!("{value}^^^&amp;{oid}&amp;ISO"),
            )],
        }
    }

    /// The assertion in canonical form, with the `{SIG}` mark where the
    /// enveloped signature goes.
    fn render(&self) -> String {
        let advice = self.names.as_ref().map_or_else(String::new, |names| {
            format!("<saml:Advice><saml:AssertionIDRef>{names}</saml:AssertionIDRef></saml:Advice>")
        });
        let instant = at(self.not_before);
        format!(
            concat!(
                r#"<saml:Assertion xmlns:saml="{saml}" ID="{id}" IssueInstant="{instant}" Version="2.0">"#,
                "<saml:Issuer>{issuer}</saml:Issuer>{{SIG}}",
                "<saml:Subject>",
                r#"<saml:NameID Format="urn:oasis:names:tc:SAML:1.1:nameid-format:unspecified">{name_id}</saml:NameID>"#,
                r#"<saml:SubjectConfirmation Method="urn:oasis:names:tc:SAML:2.0:cm:sender-vouches"></saml:SubjectConfirmation>"#,
                "</saml:Subject>",
                r#"<saml:Conditions NotBefore="{instant}" NotOnOrAfter="{end}">"#,
                "<saml:AudienceRestriction><saml:Audience>{audience}</saml:Audience></saml:AudienceRestriction>",
                "</saml:Conditions>{advice}",
                r#"<saml:AuthnStatement AuthnInstant="{instant}"><saml:AuthnContext>"#,
                "<saml:AuthnContextClassRef>{class}</saml:AuthnContextClassRef></saml:AuthnContext></saml:AuthnStatement>",
                "<saml:AttributeStatement>{attributes}</saml:AttributeStatement></saml:Assertion>"
            ),
            saml = SAML,
            id = self.id,
            instant = instant,
            issuer = NCP_B,
            name_id = HP_ID,
            end = at(self.not_on_or_after),
            audience = X_BORDER,
            advice = advice,
            class = self.authn_class,
            attributes = self.attributes.concat(),
        )
    }
}

/// `seconds` since the epoch as SAML writes an instant, or the epoch for a
/// time out of range, which the validator refuses as outside the window.
fn at(seconds: i64) -> String {
    Timestamp::from_second(seconds)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_string()
}

/// A `saml:Attribute` of one plain value.
fn attribute(name: &str, value: &str) -> String {
    format!(
        r#"<saml:Attribute Name="{name}"><saml:AttributeValue>{value}</saml:AttributeValue></saml:Attribute>"#
    )
}

/// A `saml:Attribute` of one HL7 v3 coded value in the ISCO system.
fn coded(name: &str, element: &str, code: &str) -> String {
    let value = format!(
        r#"<{element} xmlns="urn:hl7-org:v3" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" code="{code}" codeSystem="{ISCO}" xsi:type="CE"></{element}>"#
    );
    attribute(name, &value)
}

/// `xml` as the exchange carries it: base64url without padding.
#[must_use]
pub fn token_of(xml: &str) -> String {
    URL_SAFE_NO_PAD.encode(xml)
}

/// What the harness authorization server issues for: the seals it trusts,
/// the issuer that signs its tokens, and the connector the tokens are bound
/// to.
pub struct Exchange {
    /// NCP-B's seals, which the server's [`Validator`] trusts.
    pub seals: Vec<TrustedSigner>,
    /// The key the tokens are signed with, whose key set the gateway trusts.
    pub signer: &'static Issuer,
    /// The tokens' `iss`: the contact point's issuer the gateway declares.
    pub issuer: String,
    /// The tokens' `aud`: the gateway.
    pub audience: String,
    /// The DER encoding of the connector's client certificate, which every
    /// token is bound to.
    pub certificate: Vec<u8>,
}

impl fmt::Debug for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exchange")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish_non_exhaustive()
    }
}

/// The harness authorization server, answering token exchanges at `/token`.
#[derive(Debug)]
pub struct ExchangeServer {
    server: Server,
}

impl ExchangeServer {
    /// Starts a server issuing for `exchange`, its validator recording every
    /// admitted exchange in a replay guard of its own.
    pub async fn start(exchange: Exchange) -> Self {
        let server = Server::start().await;
        let validator = Validator::new(
            exchange.seals.clone(),
            vec![X_BORDER.to_owned()],
            Arc::new(MemoryReplayGuard::default()),
        );
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(Answer {
                exchange,
                validator,
            })
            .mount(&server)
            .await;
        Self { server }
    }

    /// The token endpoint.
    #[must_use]
    pub fn token_url(&self) -> String {
        format!("{}/token", self.server.uri())
    }
}

/// The `Authorization` value the national connector authenticates with
/// (RFC 6749 §2.3.1, RFC 7617 §2).
#[must_use]
pub fn connector_authorization() -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{CLIENT_ID}:{CONNECTOR_SECRET}"))
    )
}

/// The responder of the token endpoint.
struct Answer {
    /// What the server issues for.
    exchange: Exchange,
    /// The check of both assertions, with the server's replay guard.
    validator: Validator,
}

impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let authenticated = request
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == connector_authorization());
        if !authenticated {
            return ResponseTemplate::new(401)
                .insert_header("WWW-Authenticate", "Basic realm=\"ehdsi-exchange\"")
                .set_body_json(Refused::Client.body());
        }
        match self.exchanged(&request.body, Timestamp::now()) {
            Ok(token) => ResponseTemplate::new(200).set_body_json(json!({
                "access_token": token.access_token,
                "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
                "token_type": "Bearer",
                "expires_in": token.expires_in,
            })),
            Err(Refused::Server) => {
                ResponseTemplate::new(500).set_body_json(Refused::Server.body())
            }
            Err(refusal) => ResponseTemplate::new(400).set_body_json(refusal.body()),
        }
    }
}

/// A token the exchange issued.
struct Issued {
    access_token: String,
    expires_in: i64,
}

/// Why the exchange issued nothing (RFC 6749 §5.2, RFC 8693 §2.2.2).
enum Refused {
    /// The client is not the national connector.
    Client,
    /// A parameter is missing, repeated or not one the profile names.
    Request(&'static str),
    /// An assertion is refused, or the exchange was admitted before, with
    /// its §1.25 code.
    Grant(ExchangeError),
    /// The replay guard could not answer, or the token could not be signed.
    Server,
}

impl Refused {
    /// The RFC 6749 §5.2 error body.
    fn body(&self) -> Value {
        match self {
            Self::Client => json!({ "error": "invalid_client" }),
            Self::Request(why) => json!({ "error": "invalid_request", "error_description": why }),
            Self::Grant(refused) => json!({
                "error": "invalid_grant",
                "error_description": format!("{}: {refused}", refused.code()),
            }),
            Self::Server => json!({ "error": "server_error" }),
        }
    }
}

impl Answer {
    /// The token the form `body` is exchanged for at `now`.
    fn exchanged(&self, body: &[u8], now: Timestamp) -> Result<Issued, Refused> {
        let form: Vec<(String, String)> = form_urlencoded::parse(body).into_owned().collect();
        let one = |name: &'static str| -> Result<Option<&str>, Refused> {
            let mut values = form.iter().filter(|(key, _)| key == name);
            match (values.next(), values.next()) {
                (None, _) => Ok(None),
                (Some((_, value)), None) => Ok(Some(value.as_str())),
                (Some(_), Some(_)) => Err(Refused::Request("a parameter is repeated")),
            }
        };
        if one("grant_type")? != Some(TOKEN_EXCHANGE) {
            return Err(Refused::Request("grant_type is no token exchange"));
        }
        if one("subject_token_type")? != Some(SAML2) {
            return Err(Refused::Request("subject_token_type is no saml2"));
        }
        let identity = decoded(one("subject_token")?.ok_or(Refused::Request("no subject_token"))?)?;
        let treatment = match one("trc_token")? {
            None => None,
            Some(trc) => {
                if one("trc_token_type")? != Some(SAML2) {
                    return Err(Refused::Request("trc_token_type is no saml2"));
                }
                Some(decoded(trc)?)
            }
        };
        let checked = self
            .validator
            .exchange(&identity, treatment.as_deref(), now);
        // NOTE: no specification governs this: our own design; the harness's guard is
        // in memory and answers at once, so the check completes on its first poll.
        let Poll::Ready(outcome) = pin!(checked).poll(&mut Context::from_waker(Waker::noop()))
        else {
            return Err(Refused::Server);
        };
        match outcome {
            Ok(admitted) => self.issue(&admitted, now),
            Err(ExchangeError::Guard(_)) => Err(Refused::Server),
            Err(refused) => Err(Refused::Grant(refused)),
        }
    }

    /// The token for the `admitted` exchange: living no longer than
    /// [`LIFETIME`] or the exchange's windows.
    fn issue(&self, admitted: &assertion::Exchange, now: Timestamp) -> Result<Issued, Refused> {
        let now = now.as_second();
        let exp = now
            .saturating_add(LIFETIME)
            .min(admitted.ends().as_second());
        let mut claims = Claims::new(&self.exchange.issuer, &self.exchange.audience);
        CLIENT_ID.clone_into(&mut claims.client_id);
        CLIENT_ID.clone_into(&mut claims.sub);
        claims.iat = now;
        claims.exp = exp;
        claims.cnf = Some(cnf(&self.exchange.certificate));
        let mut payload = serde_json::to_value(&claims).map_err(|_unwritten| Refused::Server)?;
        if let Some(object) = payload.as_object_mut() {
            object.insert(String::from("ehdsi"), admitted.claim());
        }
        let access_token = self
            .exchange
            .signer
            .sign(&self.exchange.signer.header(), &payload.to_string())
            .map_err(|_unsigned| Refused::Server)?;
        Ok(Issued {
            access_token,
            expires_in: exp.saturating_sub(now),
        })
    }
}

/// The assertion a base64url token carries, padded or not.
fn decoded(token: &str) -> Result<String, Refused> {
    let bytes = URL_SAFE_NO_PAD
        .decode(token.trim_end_matches('='))
        .map_err(|_undecoded| Refused::Request("a token is no base64url"))?;
    String::from_utf8(bytes).map_err(|_unread| Refused::Request("a token is no UTF-8"))
}
