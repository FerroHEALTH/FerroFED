// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The claims of an `openEHR-federation-client` token as a node reads them,
//! and the searches of what a node received.

use std::error::Error as StdError;

use serde::Deserialize;

use crate::support::is_minted_form;

/// One purpose of use of a conveyed token, as a node reads it.
#[derive(Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConveyedPurpose {
    /// The code system.
    pub(crate) system: Option<String>,
    /// The code.
    pub(crate) code: String,
}

/// The claims of an `openEHR-federation-client` token as a node reads them;
/// a claim not named here, `person_id` among them, fails the read (§5.4.1,
/// N33).
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Conveyed {
    pub(crate) iss: String,
    pub(crate) aud: String,
    pub(crate) iat: i64,
    pub(crate) exp: i64,
    pub(crate) jti: String,
    pub(crate) sub: String,
    pub(crate) iss_upstream: Option<String>,
    pub(crate) verified_by: Option<String>,
    pub(crate) subject_organization_id: Option<String>,
    #[serde(default)]
    pub(crate) purpose_of_use: Vec<ConveyedPurpose>,
    pub(crate) scope: Option<String>,
    /// The confined patient's own `ehr_id` at the node, under a confined
    /// `patient/` grant only.
    #[serde(default, rename = "ehrId", skip_serializing_if = "Option::is_none")]
    pub(crate) ehr_id: Option<String>,
    /// The professional's name, IHE IUA `subject_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subject_name: Option<String>,
    /// The professional's identifier, IHE IUA
    /// `national_provider_identifier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) national_provider_identifier: Option<String>,
    /// The agency that issued that identifier (Implementing Regulation (EU)
    /// 2026/2099 Annex Table 1 `issuing_authority_name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) national_provider_identifier_authority: Option<String>,
    /// The professional's roles, IHE IUA `subject_role`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) subject_role: Vec<ConveyedPurpose>,
    /// `person` or `client`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) acting: Option<String>,
    /// `low`, `substantial` or `high`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) assurance_level: Option<String>,
    /// What a national contact point relays, marked as its assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) national_contact_point: Option<ConveyedContactPoint>,
}

/// The `national_contact_point` claim as a node reads it: Implementing
/// Regulation (EU) 2026/2099 Annex Tables 1 and 2, and who asserted them.
#[derive(Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConveyedContactPoint {
    pub(crate) asserted_by: String,
    pub(crate) health_professional: ConveyedProfessional,
    pub(crate) healthcare_provider: ConveyedProvider,
}

/// 2026/2099 Annex Table 1 as a node reads it.
#[derive(Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConveyedProfessional {
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) country_code: String,
    pub(crate) hp_identifier: String,
    pub(crate) issuing_authority_name: String,
    pub(crate) hp_professional_role: Vec<ConveyedPurpose>,
    pub(crate) healthcare_provider_identifier: String,
}

/// 2026/2099 Annex Table 2 as a node reads it.
#[derive(Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConveyedProvider {
    pub(crate) healthcare_provider_identifier: String,
    pub(crate) issuing_authority_name: String,
    pub(crate) healthcare_provider_name: String,
    pub(crate) healthcare_provider_address: String,
}

/// The claims `token` carries, read without verifying it.
pub(crate) fn conveyed_claims(token: &str) -> Result<Conveyed, Box<dyn StdError>> {
    Ok(jsonwebtoken::dangerous::insecure_decode_claims::<Conveyed>(
        token,
    )?)
}

/// What a search reads in place of a conveyed token's `aud`.
pub(crate) const CONVEYED_AUDIENCE: &str = "<audience>";

/// What a search reads in place of a conveyed token's minted `jti`.
pub(crate) const MINTED_TOKEN_ID: &str = "<minted-token-id>";

/// The claims of `token` as JSON, for a search of what a node received, its
/// `aud` read as [`CONVEYED_AUDIENCE`], its `iat` and `exp` as `0` and its
/// `jti` as [`MINTED_TOKEN_ID`].
///
/// The `aud` is the endpoint id the registry gives the node it is sent to,
/// composed from no request, as the minted request id is. The gateway mints
/// `iat` and `exp` from its clock and `jti` at random, from no request
/// either, and the outbound gate exempts them for that reason: a clock
/// reading such as `1791234567` or a random UUID holds a short synthetic
/// identifier such as `12345` by chance. Every other claim stays as the node
/// reads it, so a client value in one is still found.
///
/// # Errors
///
/// When the token does not decode, or its `jti` is not in the form the
/// gateway mints, so no other value is ever masked.
pub(crate) fn searched_claims(token: &str) -> Result<String, Box<dyn StdError>> {
    let mut claims = conveyed_claims(token)?;
    if !is_minted_form(&claims.jti) {
        return Err(format!("{:?} is not a token id the gateway minted", claims.jti).into());
    }
    CONVEYED_AUDIENCE.clone_into(&mut claims.aud);
    (claims.iat, claims.exp) = (0, 0);
    MINTED_TOKEN_ID.clone_into(&mut claims.jti);
    Ok(serde_json::to_string(&claims)?)
}

/// The claims of `token` as JSON with the values minted per token, `iat`,
/// `exp` and `jti`, cleared, so two requests the gateway sent for the same
/// caller compare equal.
pub(crate) fn stable_claims(token: &str) -> Result<String, Box<dyn StdError>> {
    let mut claims = conveyed_claims(token)?;
    (claims.iat, claims.exp) = (0, 0);
    claims.jti.clear();
    Ok(serde_json::to_string(&claims)?)
}

// A wire search for the synthetic `12345` never reads the clock or the random
// `jti`, which the outbound gate exempts, and still reads a caller's claim.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "test assertions in a test that returns its setup errors"
)]
fn a_search_reads_no_minted_claim_and_every_caller_claim() -> Result<(), Box<dyn StdError>> {
    let claims = Conveyed {
        iss: "example-federation".to_owned(),
        aud: "node_1".to_owned(),
        iat: 1_791_234_567,
        exp: 1_791_234_627,
        jti: "0f123451-2345-4234-9123-451234512345".to_owned(),
        sub: "synthetic-caller".to_owned(),
        iss_upstream: None,
        verified_by: None,
        subject_organization_id: None,
        purpose_of_use: Vec::new(),
        scope: None,
        ehr_id: None,
        subject_name: None,
        national_provider_identifier: None,
        national_provider_identifier_authority: None,
        subject_role: Vec::new(),
        acting: None,
        assurance_level: None,
        national_contact_point: None,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"synthetic"),
    )?;
    let searched = searched_claims(&token)?;
    assert!(!searched.contains("12345"), "{searched}");
    assert!(searched.contains("synthetic-caller"), "{searched}");
    Ok(())
}
