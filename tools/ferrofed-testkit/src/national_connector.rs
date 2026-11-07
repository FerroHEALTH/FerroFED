// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness national connector, a test device on the gateway's FHIR face.
//!
//! It plays a national contact point's national connector (Regulation (EU)
//! 2025/327 Art 23; Implementing Regulation (EU) 2026/2099 Art 7).
//!
//! **This is a test device, not a national connector.** It issues its own
//! access tokens as the contact point's authorization server would, each
//! carrying what a conforming eHDSI HP Identity Assertion and Treatment
//! Relationship Confirmation provide (eHDSI SAML Profile 10.0.0 §2.1, §2.3,
//! §4), synthetic and inside the `urn:oid:2.999` example arc, bound to the
//! device's client certificate (RFC 8705 §3), and calls the face the way a
//! national connector reaches a Member State's national infrastructure: it
//! confirms the patient's national identifier at a PDQm Supplier over ITI-78
//! (IHE PDQm 3.2.0 §2:3.78), then looks the patient up, asks
//! `Patient/$summary`, the document list, the prescription list and read,
//! and posts a dispensation, each with a correlation identifier of its own.
//! The `NCPeH` API IG 9.1.1-ci puts the interface with the national connector
//! out of scope, so no specification governs the device: our own design.
//!
//! The attributes travel under the `ehdsi` claim, each at its field name,
//! which `ferrohealth-smart`'s `ehdsi` mapping reads; [`contact_point_toml`]
//! is the configuration that declares the device's issuer a contact point.
//! Its `Debug` shows the issuer and the base it calls, and nothing here
//! logs a value.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the token's ehdsi and cnf claims are written as JSON"
)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use base64::Engine as _;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, LOCATION};
use http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use url::form_urlencoded;

use crate::issuer::{Claims, Issuer, IssuerError};

/// The media type of every FHIR body the device sends and accepts.
const FHIR_JSON: &str = "application/fhir+json";

/// The `client_id` and `sub` of the device's tokens: a client token, the
/// connector acting for the professional it relays.
pub const CLIENT_ID: &str = "synthetic-national-connector";

/// The request header the device sends each correlation identifier in.
pub const CORRELATION_HEADER: &str = "ncp-correlation-id";

/// The `country_code` of the Member State that issued the professional's
/// identification data (2026/2099 Annex Table 1), a Member State, which the
/// deployment configures for the contact point.
pub const COUNTRY: &str = "LU";

/// The professional's identifier, the assertion's `NameID` (SAML Profile
/// 10.0.0 §2.1), the Annex `hp_identifier`.
pub const HP_ID: &str = "urn:oid:2.999.9.71";

/// The professional's name, the assertion's `subject-id` (SAML Profile
/// 10.0.0 §2.3), which the gateway never relays or records.
pub const HP_NAME: &str = "Qz9-ncp-subject-name";

/// The `issuing_authority_name` of [`HP_ID`] (Annex Table 1), which the
/// deployment configures for the contact point.
pub const HP_AUTHORITY: &str = "Qz9-ncp-hp-authority";

/// The code system of [`ROLE`].
pub const ROLE_SYSTEM: &str = "urn:oid:2.999.9";

/// The professional's `hp_professional_role` (Annex Table 1).
pub const ROLE: &str = "Qz9-ncp-role";

/// The `healthcare_provider_identifier`, the assertion's `organization-id`
/// (Annex Table 2).
pub const HCP_ID: &str = "urn:oid:2.999.9.72";

/// The provider's name, the assertion's `organization`, which the gateway
/// never relays or records.
pub const HCP_NAME: &str = "Qz9-ncp-hcp-name";

/// The `issuing_authority_name` of [`HCP_ID`] (Annex Table 2), which the
/// deployment configures for the contact point.
pub const HCP_AUTHORITY: &str = "Qz9-ncp-hcp-authority";

/// A synthetic IHE IUA `person_id`, a patient identifier the device's token
/// carries so a test can show the gateway never reads it (§5.4.1, N33).
pub const PERSON_ID: &str = "Qz9-ncp-person-id";

/// The authentication class of the professional, one of the seven of SAML
/// Profile 10.0.0 §2.1.
pub const AUTHENTICATION_CLASS: &str = "urn:oasis:names:tc:SAML:2.0:ac:classes:X509";

/// What a contact point's authorization server writes under `ehdsi` for a
/// conforming assertion of the professional `hp_identifier`.
///
/// The identity assertion and the treatment relationship confirmation are
/// both valid from a minute before `now` for an hour, and each attribute
/// sits at its field name: the mandatory attributes of SAML Profile 10.0.0
/// §2.1 and §2.3, the provider's name, the purpose of use `purpose`, and no
/// patient.
#[must_use]
pub fn asserted(hp_identifier: &str, purpose: &str, now: i64) -> BTreeMap<String, Value> {
    let (start, end) = (now.saturating_sub(60), now.saturating_add(3600));
    [
        ("professional.hp_identifier", json!(hp_identifier)),
        ("professional.name", json!(HP_NAME)),
        (
            "professional.hp_professional_role",
            json!([{ "system": ROLE_SYSTEM, "code": ROLE }]),
        ),
        (
            "professional.authentication_method",
            json!(AUTHENTICATION_CLASS),
        ),
        ("provider.healthcare_provider_identifier", json!(HCP_ID)),
        ("provider.healthcare_provider_name", json!(HCP_NAME)),
        ("provider.facility_type", json!("Hospital")),
        ("provider.home_community_id", json!("urn:oid:2.999.9.73")),
        ("point_of_care", json!("Qz9-ncp-point-of-care")),
        ("purpose_of_use", json!(purpose)),
        ("assertion.issuer", json!("https://ncp-idp.example.test")),
        ("assertion.id", json!("_Qz9-ncp-ida-1")),
        ("assertion.not_before", json!(start)),
        ("assertion.not_on_or_after", json!(end)),
        ("trc.id", json!("_Qz9-ncp-trc-1")),
        ("trc.not_before", json!(start)),
        ("trc.not_on_or_after", json!(end)),
    ]
    .into_iter()
    .map(|(field, value)| (field.to_owned(), value))
    .collect()
}

/// A synthetic self-signed client certificate, its DER encoding, generated
/// at run time so no private key is ever committed.
///
/// # Errors
///
/// Returns the [`rcgen::Error`] of a certificate that cannot be made.
pub fn client_certificate() -> Result<Vec<u8>, rcgen::Error> {
    let made = rcgen::generate_simple_self_signed(vec![String::from("ncp.example.test")])?;
    Ok(made.cert.der().to_vec())
}

/// The `cnf` claim of a token bound to the certificate `der`: its
/// `x5t#S256`, the base64url SHA-256 of the DER encoding (RFC 8705 §3.1).
#[must_use]
pub fn cnf(der: &[u8]) -> Value {
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, der);
    json!({ "x5t#S256": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_ref()) })
}

/// The configuration that declares `issuer` a contact point whose tokens
/// name `audience`, its key set in the file `jwks_file`.
///
/// It is an `[[auth.issuer]]` of kind `federation` with `sender_constraint =
/// "required"`, its `[[auth.federation]]` table on the `ehdsi` mapping with
/// [`COUNTRY`], [`HP_AUTHORITY`] and [`HCP_AUTHORITY`] configured, and its
/// `[[federation.contact_point]]` naming [`CORRELATION_HEADER`].
#[must_use]
pub fn contact_point_toml(issuer: &str, audience: &str, jwks_file: &str) -> String {
    let jwks = toml_string(jwks_file);
    format!(
        "\n[[auth.issuer]]\nissuer = \"{issuer}\"\nkind = \"federation\"\n\
         audience = [\"{audience}\"]\njwks_file = {jwks}\nsender_constraint = \"required\"\n\n\
         [[auth.federation]]\nissuer = \"{issuer}\"\ncontact_point = true\nmapping = \"ehdsi\"\n\
         countries = [\"{COUNTRY}\"]\n\
         fields.\"professional.country_code\" = [{{ constant = \"{COUNTRY}\" }}]\n\
         fields.\"professional.issuing_authority_name\" = [{{ constant = \"{HP_AUTHORITY}\" }}]\n\
         fields.\"provider.issuing_authority_name\" = [{{ constant = \"{HCP_AUTHORITY}\" }}]\n\n\
         [[federation.contact_point]]\nissuer = \"{issuer}\"\ncorrelation_header = \"{CORRELATION_HEADER}\"\n"
    )
}

/// `text` as a TOML basic string.
fn toml_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// The device could not sign a token or reach a service.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConnectorError {
    /// The token could not be signed.
    #[error("the national connector's token could not be signed")]
    Token(#[source] IssuerError),
    /// The HTTP client could not be built.
    #[error("the national connector's HTTP client could not be built")]
    Client(#[source] reqwest::Error),
    /// The client certificate could not be made.
    #[error("the national connector's client certificate could not be made")]
    Certificate(#[source] rcgen::Error),
    /// An ITI-78 answer was no `searchset` the device reads.
    #[error("the PDQm Supplier's answer could not be read")]
    Json(#[source] serde_json::Error),
    /// A request could not be sent or its answer not read.
    #[error("{step} could not be sent")]
    Send {
        /// The call, method and path.
        step: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
}

/// The part of an ITI-78 `searchset` the confirmation reads.
#[derive(Deserialize)]
struct Searchset {
    #[serde(default)]
    entry: Vec<Entry>,
}

/// One entry of a [`Searchset`].
#[derive(Deserialize)]
struct Entry {
    resource: Matched,
}

/// The `active` flag of a matched `Patient`, absent unless deprecated.
#[derive(Deserialize)]
struct Matched {
    active: Option<bool>,
}

/// One answer the device read: its status, its headers and its body.
#[derive(Clone, PartialEq, Eq)]
pub struct Answer {
    /// The status.
    pub status: StatusCode,
    /// The headers.
    pub headers: HeaderMap,
    /// The body, as text.
    pub body: String,
    /// The correlation identifier the request carried.
    pub correlation: String,
}

impl Answer {
    /// Returns the `Location` header, when the answer carries one.
    #[must_use]
    pub fn location(&self) -> Option<&str> {
        self.headers
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
    }
}

impl fmt::Debug for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Answer")
            .field("status", &self.status)
            .field("bytes", &self.body.len())
            .field("correlation", &self.correlation)
            .finish_non_exhaustive()
    }
}

/// A patient's national identifier, as the contact point confirmed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NationalId<'a> {
    /// The identifier system, inside the example arc.
    pub system: &'a str,
    /// The identifier value.
    pub value: &'a str,
}

impl NationalId<'_> {
    /// Returns the identifier as a token search value, `system|value`,
    /// percent-encoded for a query string.
    fn token(&self) -> String {
        form_urlencoded::byte_serialize(format!("{}|{}", self.system, self.value).as_bytes())
            .collect()
    }
}

/// The harness national connector, calling the FHIR face at one base.
pub struct NationalConnector {
    issuer: Issuer,
    certificate: Vec<u8>,
    audience: String,
    fhir_base: String,
    client: reqwest::Client,
    sent: AtomicU64,
    treating: Mutex<Option<String>>,
}

impl fmt::Debug for NationalConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NationalConnector")
            .field("issuer", &self.issuer.name())
            .field("fhir_base", &self.fhir_base)
            .field("sent", &self.sent.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl NationalConnector {
    /// Creates a connector whose tokens `issuer` signs for `audience`, bound
    /// to a client certificate of its own, and which calls the FHIR face at
    /// `fhir_base`, with no trailing slash.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Client`] when the HTTP client cannot be
    /// built, and [`ConnectorError::Certificate`] when the certificate
    /// cannot be made.
    pub fn new(
        issuer: Issuer,
        audience: impl Into<String>,
        fhir_base: impl Into<String>,
    ) -> Result<Self, ConnectorError> {
        Ok(Self {
            issuer,
            certificate: client_certificate().map_err(ConnectorError::Certificate)?,
            audience: audience.into(),
            fhir_base: fhir_base.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::builder()
                .build()
                .map_err(ConnectorError::Client)?,
            sent: AtomicU64::new(0),
            treating: Mutex::new(None),
        })
    }

    /// Returns the issuer that signs the device's tokens, whose key set the
    /// gateway under test trusts.
    #[must_use]
    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    /// Returns the DER encoding of the client certificate the device's
    /// tokens are bound to, which the harness hands the gateway as the
    /// certificate of the connection, standing in for its TLS layer.
    #[must_use]
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }

    /// Returns the claims of the device's token: a client token, its `sub`
    /// its `client_id`, every scope, what a conforming eHDSI assertion
    /// provides with the purpose of use `TREATMENT` ([`asserted`]), bound to
    /// the device's certificate, an IUA `person_id`, and the TRC patient once
    /// the device treats one ([`Self::treating`]).
    #[must_use]
    pub fn claims(&self) -> Claims {
        let mut claims = Claims::new(self.issuer.name(), &self.audience);
        CLIENT_ID.clone_into(&mut claims.client_id);
        CLIENT_ID.clone_into(&mut claims.sub);
        if let Some(extensions) = claims.extensions.as_mut() {
            extensions.ihe_iua.person_id = Some(PERSON_ID.to_owned());
        }
        claims.ehdsi = asserted(HP_ID, "TREATMENT", claims.iat);
        claims.cnf = Some(cnf(&self.certificate));
        let treating = self
            .treating
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(patient) = treating {
            claims
                .ehdsi
                .insert(String::from("patient.id"), Value::String(patient));
        }
        claims
    }

    /// Makes every later token carry `patient` as the patient of a treatment
    /// relationship confirmation, as the contact point's authorization
    /// server writes the TRC's `subject-id` once the patient is identified
    /// (eHDSI SAML Profile 10.0.0 §4.3).
    pub fn treating(&self, patient: NationalId<'_>) {
        *self.treating.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(format!("{}|{}", patient.system, patient.value));
    }

    /// Returns a fresh token with [`Self::claims`].
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] when the issuer cannot sign.
    pub fn token(&self) -> Result<String, ConnectorError> {
        self.issuer
            .mint(&self.claims())
            .map_err(ConnectorError::Token)
    }

    /// Confirms `patient` at the PDQm Supplier whose FHIR base, with its
    /// trailing slash, is `pdq_base`: an ITI-78 search by the identifier
    /// (IHE PDQm 3.2.0 §2:3.78.4.1), answering whether exactly one active
    /// `Patient` carries it.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Send`] when the search cannot be sent or
    /// its answer not read, and [`ConnectorError::Json`] when a `200` answer
    /// is no `searchset`.
    pub async fn confirm(
        &self,
        pdq_base: &str,
        patient: NationalId<'_>,
    ) -> Result<bool, ConnectorError> {
        let step = String::from("ITI-78 POST Patient/_search");
        let body: String = form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "identifier",
                &format!("{}|{}", patient.system, patient.value),
            )
            .finish();
        let answer = self
            .client
            .post(format!("{pdq_base}Patient/_search"))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, FHIR_JSON)
            .body(body)
            .send()
            .await
            .map_err(|source| ConnectorError::Send {
                step: step.clone(),
                source,
            })?;
        let status = answer.status();
        let text = answer
            .text()
            .await
            .map_err(|source| ConnectorError::Send { step, source })?;
        if status != StatusCode::OK {
            return Ok(false);
        }
        let searchset: Searchset = serde_json::from_str(&text).map_err(ConnectorError::Json)?;
        let active = searchset
            .entry
            .iter()
            .filter(|entry| entry.resource.active != Some(false))
            .count();
        Ok(active == 1)
    }

    /// Looks `patient` up with `GET {fhir-base}/Patient?identifier=`, the
    /// patient lookup Xt-EHR D5.1 §3.4.6 `api-consumer-patient` has a
    /// consumer use before it asks for data, and then treats `patient`
    /// ([`Self::treating`]), as a contact point holds a treatment
    /// relationship confirmation once the patient is identified.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn lookup(&self, patient: NationalId<'_>) -> Result<Answer, ConnectorError> {
        let answer = self
            .get(&format!("/Patient?identifier={}", patient.token()))
            .await?;
        self.treating(patient);
        Ok(answer)
    }

    /// Asks `GET {fhir-base}/Patient/$summary` for `patient`.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn summary(&self, patient: NationalId<'_>) -> Result<Answer, ConnectorError> {
        self.get(&format!("/Patient/$summary?identifier={}", patient.token()))
            .await
    }

    /// Asks `GET {fhir-base}/DocumentReference` for `patient`'s documents.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn documents(&self, patient: NationalId<'_>) -> Result<Answer, ConnectorError> {
        self.get(&format!(
            "/DocumentReference?patient.identifier={}",
            patient.token()
        ))
        .await
    }

    /// Asks `GET {fhir-base}/MedicationRequest` for `patient`'s
    /// prescriptions.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn prescriptions(&self, patient: NationalId<'_>) -> Result<Answer, ConnectorError> {
        self.get(&format!(
            "/MedicationRequest?patient.identifier={}",
            patient.token()
        ))
        .await
    }

    /// Reads `GET {fhir-base}/MedicationRequest/{id}`, the id the face
    /// named the prescription by.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn prescription(&self, id: &str) -> Result<Answer, ConnectorError> {
        let id: String = form_urlencoded::byte_serialize(id.as_bytes()).collect();
        self.get(&format!("/MedicationRequest/{id}")).await
    }

    /// Posts the dispensation `body` to `{fhir-base}/MedicationDispense`.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn dispense(&self, body: &str) -> Result<Answer, ConnectorError> {
        let correlation = self.correlation();
        let step = String::from("POST MedicationDispense");
        let request = self
            .client
            .post(format!("{}/MedicationDispense", self.fhir_base))
            .header(CONTENT_TYPE, FHIR_JSON)
            .body(body.to_owned());
        self.send(request, step, correlation).await
    }

    /// Sends `GET {fhir-base}{path}`.
    async fn get(&self, path: &str) -> Result<Answer, ConnectorError> {
        let correlation = self.correlation();
        let step = format!("GET {}", path.split('?').next().unwrap_or(path));
        let request = self.client.get(format!("{}{path}", self.fhir_base));
        self.send(request, step, correlation).await
    }

    /// Sends `request` with a fresh token, `correlation` and the FHIR
    /// `Accept`, and reads the answer.
    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        step: String,
        correlation: String,
    ) -> Result<Answer, ConnectorError> {
        let token = self.token()?;
        let answer = request
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, FHIR_JSON)
            .header(CORRELATION_HEADER, &correlation)
            .send()
            .await
            .map_err(|source| ConnectorError::Send {
                step: step.clone(),
                source,
            })?;
        let status = answer.status();
        let headers = answer.headers().clone();
        let body = answer
            .text()
            .await
            .map_err(|source| ConnectorError::Send { step, source })?;
        Ok(Answer {
            status,
            headers,
            body,
            correlation,
        })
    }

    /// Returns the next correlation identifier, unique within the device.
    fn correlation(&self) -> String {
        let n = self.sent.fetch_add(1, Ordering::Relaxed);
        format!("Qz9-ncp-exchange-{n}")
    }
}
