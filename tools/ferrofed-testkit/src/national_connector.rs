// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness national connector, a test device on the gateway's FHIR face.
//!
//! It plays a national contact point's national connector (Regulation (EU)
//! 2025/327 Art 23; Implementing Regulation (EU) 2026/2099 Art 7).
//!
//! **This is a test device, not a national connector.** It issues its own
//! access tokens, each carrying every attribute of the 2026/2099 Annex
//! Tables 1 and 2 and a purpose of use, synthetic and inside the
//! `urn:oid:2.999` example arc, and calls the face the way a national
//! connector reaches a Member State's national infrastructure: it confirms
//! the patient's national identifier at the harness PDQm Supplier over ITI-78
//! (IHE PDQm 3.2.0 §2:3.78), then looks the patient up, asks
//! `Patient/$summary`, the document list, the prescription list and read,
//! and posts a dispensation, each with a correlation identifier of its own.
//! The `NCPeH` API IG 9.1.1-ci puts the interface with the national connector
//! out of scope, so no specification governs the device: our own design.
//!
//! The Annex attributes IHE IUA defines a claim for travel in its `ihe_iua`
//! extension (ITI TF-2 3.71.4.2.2.1.1); each other one travels in the string
//! claim [`CLAIMS`] names, which the gateway's
//! `[trust.issuer.national_contact_point]` declares. Its `Debug` shows the
//! issuer and the base it calls, and nothing here logs a value.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, LOCATION};
use http::{HeaderMap, StatusCode};
use serde::Deserialize;
use url::form_urlencoded;

use crate::issuer::{Claims, Coding, Issuer, IssuerError};

/// The media type of every FHIR body the device sends and accepts.
const FHIR_JSON: &str = "application/fhir+json";

/// The `client_id` and `sub` of the device's tokens: a client token, the
/// connector acting for the professional it relays.
pub const CLIENT_ID: &str = "synthetic-national-connector";

/// The request header the device sends each correlation identifier in.
pub const CORRELATION_HEADER: &str = "ncp-correlation-id";

/// The professional's `family_name` (2026/2099 Annex Table 1).
pub const FAMILY: &str = "Qz9-ncp-family";

/// The professional's `given_name` (Annex Table 1).
pub const GIVEN: &str = "Qz9-ncp-given";

/// The `country_code` of the Member State that issued the professional's
/// identification data (Annex Table 1), a Member State.
pub const COUNTRY: &str = "LU";

/// The professional's `hp_identifier` (Annex Table 1).
pub const HP_ID: &str = "urn:oid:2.999.9.71";

/// The `issuing_authority_name` of [`HP_ID`] (Annex Table 1).
pub const HP_AUTHORITY: &str = "Qz9-ncp-hp-authority";

/// The code system of [`ROLE`].
pub const ROLE_SYSTEM: &str = "urn:oid:2.999.9";

/// The professional's `hp_professional_role` (Annex Table 1).
pub const ROLE: &str = "Qz9-ncp-role";

/// The `healthcare_provider_identifier` (Annex Table 2).
pub const HCP_ID: &str = "urn:oid:2.999.9.72";

/// The `issuing_authority_name` of [`HCP_ID`] (Annex Table 2).
pub const HCP_AUTHORITY: &str = "Qz9-ncp-hcp-authority";

/// The `healthcare_provider_name` (Annex Table 2).
pub const HCP_NAME: &str = "Qz9-ncp-hcp-name";

/// The `healthcare_provider_address` (Annex Table 2).
pub const HCP_ADDRESS: &str = "Qz9-ncp-hcp-address";

/// A synthetic IHE IUA `person_id`, a patient identifier the device's token
/// carries so a test can show the gateway never reads it (§5.4.1, N33).
pub const PERSON_ID: &str = "Qz9-ncp-person-id";

/// The names of the string claims that carry the Annex attributes IHE IUA
/// defines no claim for, in the order of `[trust.issuer.national_contact_point]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimNames {
    /// The claim of [`FAMILY`].
    pub family_name: &'static str,
    /// The claim of [`GIVEN`].
    pub given_name: &'static str,
    /// The claim of [`COUNTRY`].
    pub country_code: &'static str,
    /// The claim of [`HP_AUTHORITY`].
    pub professional_issuing_authority: &'static str,
    /// The claim of [`HCP_AUTHORITY`].
    pub provider_issuing_authority: &'static str,
    /// The claim of [`HCP_ADDRESS`].
    pub provider_address: &'static str,
}

/// The claim names the device writes, which the gateway under test declares.
pub const CLAIMS: ClaimNames = ClaimNames {
    family_name: "ncp_family_name",
    given_name: "ncp_given_name",
    country_code: "ncp_country_code",
    professional_issuing_authority: "ncp_hp_issuing_authority",
    provider_issuing_authority: "ncp_hcp_issuing_authority",
    provider_address: "ncp_hcp_address",
};

impl ClaimNames {
    /// Returns the `[trust.issuer.national_contact_point]` table that
    /// declares these names and [`CORRELATION_HEADER`], as configuration
    /// text for the trust entry it follows.
    #[must_use]
    pub fn toml(&self) -> String {
        format!(
            "\n[trust.issuer.national_contact_point]\nfamily_name = \"{}\"\ngiven_name = \"{}\"\n\
             country_code = \"{}\"\nprofessional_issuing_authority = \"{}\"\n\
             provider_issuing_authority = \"{}\"\nprovider_address = \"{}\"\n\
             correlation_header = \"{CORRELATION_HEADER}\"\n",
            self.family_name,
            self.given_name,
            self.country_code,
            self.professional_issuing_authority,
            self.provider_issuing_authority,
            self.provider_address
        )
    }
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
    audience: String,
    fhir_base: String,
    client: reqwest::Client,
    sent: AtomicU64,
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
    /// Creates a connector whose tokens `issuer` signs for `audience`, and
    /// which calls the FHIR face at `fhir_base`, with no trailing slash.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Client`] when the HTTP client cannot be
    /// built.
    pub fn new(
        issuer: Issuer,
        audience: impl Into<String>,
        fhir_base: impl Into<String>,
    ) -> Result<Self, ConnectorError> {
        Ok(Self {
            issuer,
            audience: audience.into(),
            fhir_base: fhir_base.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::builder()
                .build()
                .map_err(ConnectorError::Client)?,
            sent: AtomicU64::new(0),
        })
    }

    /// Returns the issuer that signs the device's tokens, whose key set the
    /// gateway under test trusts.
    #[must_use]
    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    /// Returns the claims of the device's token: a client token, its `sub`
    /// its `client_id`, every scope, the purpose of use `TREAT`, every Annex
    /// attribute, and an IUA `person_id`.
    #[must_use]
    pub fn claims(&self) -> Claims {
        let mut claims = Claims::new(self.issuer.name(), &self.audience);
        CLIENT_ID.clone_into(&mut claims.client_id);
        CLIENT_ID.clone_into(&mut claims.sub);
        if let Some(extensions) = claims.extensions.as_mut() {
            let iua = &mut extensions.ihe_iua;
            iua.national_provider_identifier = Some(HP_ID.to_owned());
            iua.subject_role = vec![Coding {
                system: ROLE_SYSTEM.to_owned(),
                code: ROLE.to_owned(),
            }];
            iua.subject_organization_id = Some(HCP_ID.to_owned());
            iua.subject_organization = Some(HCP_NAME.to_owned());
            iua.person_id = Some(PERSON_ID.to_owned());
        }
        for (claim, value) in [
            (CLAIMS.family_name, FAMILY),
            (CLAIMS.given_name, GIVEN),
            (CLAIMS.country_code, COUNTRY),
            (CLAIMS.professional_issuing_authority, HP_AUTHORITY),
            (CLAIMS.provider_issuing_authority, HCP_AUTHORITY),
            (CLAIMS.provider_address, HCP_ADDRESS),
        ] {
            claims.other.insert(claim.to_owned(), value.to_owned());
        }
        claims
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
    /// consumer use before it asks for data.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Token`] or [`ConnectorError::Send`].
    pub async fn lookup(&self, patient: NationalId<'_>) -> Result<Answer, ConnectorError> {
        self.get(&format!("/Patient?identifier={}", patient.token()))
            .await
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
