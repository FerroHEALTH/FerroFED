// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national-contact-point caller: an issuer declared a national contact
//! point for digital health, whose connector relays the request of a health
//! professional of another Member State (Regulation (EU) 2025/327 Art 23;
//! Implementing Regulation (EU) 2026/2099 Art 6 and 7).
//!
//! A patient-data request from it needs every attribute of the 2026/2099
//! Annex Tables 1 and 2 and a purpose of use, or it is refused with nothing
//! sent ([`admission`]); its requests are served with consent exclusions
//! withheld whatever the deployment's setting (Regulation (EU) 2025/327
//! Art 8, Art 11(5); [`withheld`]); the attributes reach the node and the
//! access record marked as asserted by the contact point, and no IUA
//! `person_id` reaches a node (§13.4 authn-end-user, N24, N33; [`relayed`]);
//! and `OPTIONS {base}/` declares the setting ([`declared`]).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

mod admission;
mod config;
mod declared;
mod relayed;
mod withheld;

use ferrofed_server::config::auth::AuthSettings;
use ferrofed_server::config::auth::contact_point::ContactPointClaims;
use ferrofed_testkit::issuer::{Claims, Coding};

use crate::support;

/// The header the test connector sends its correlation identifier in.
pub(crate) const CORRELATION_HEADER: &str = "ncp-correlation-id";

/// A synthetic correlation identifier.
pub(crate) const CORRELATION: &str = "Qz7-ncp-exchange-0815";

/// A synthetic IHE IUA `person_id`, the patient identifier a contact point's
/// token may carry and the gateway never reads.
pub(crate) const PERSON_ID: &str = "SENTINEL-PERSON-ID-7q3z";

/// The synthetic attribute values the test contact point relays, each
/// unlike any other text.
pub(crate) const FAMILY: &str = "Qz7-family-61";
pub(crate) const GIVEN: &str = "Qz7-given-62";
pub(crate) const COUNTRY: &str = "LU";
pub(crate) const HP_ID: &str = "Qz7-hp-63";
pub(crate) const HP_AUTHORITY: &str = "Qz7-hp-authority-64";
pub(crate) const ROLE_SYSTEM: &str = "urn:oid:2.999.9";
pub(crate) const ROLE: &str = "Qz7-role-65";
pub(crate) const HCP_ID: &str = "urn:oid:2.999.9.66";
pub(crate) const HCP_AUTHORITY: &str = "Qz7-hcp-authority-67";
pub(crate) const HCP_NAME: &str = "Qz7-hcp-name-68";
pub(crate) const HCP_ADDRESS: &str = "Qz7-hcp-address-69";

/// The `[auth.issuer.national_contact_point]` of the test contact point:
/// the claim each Annex attribute IUA carries no claim for is read from,
/// and the correlation header.
pub(crate) fn claim_names() -> ContactPointClaims {
    ContactPointClaims {
        family_name: String::from("ncp_family_name"),
        given_name: String::from("ncp_given_name"),
        country_code: String::from("ncp_country_code"),
        professional_issuing_authority: String::from("ncp_hp_issuing_authority"),
        provider_issuing_authority: String::from("ncp_hcp_issuing_authority"),
        provider_address: String::from("ncp_hcp_address"),
        correlation_header: Some(CORRELATION_HEADER.to_owned()),
        additional_countries: Vec::new(),
    }
}

/// The suite's `[auth]`, its test issuer declared a national contact point.
pub(crate) fn declared() -> Result<AuthSettings, Box<dyn std::error::Error>> {
    let mut auth = support::auth();
    let resolved = claim_names().resolve("auth.issuer[0].national_contact_point")?;
    for issuer in &mut auth.issuers {
        issuer.national_contact_point = Some(resolved.clone());
    }
    Ok(auth)
}

/// The claims of the test connector's token: a client token, its `sub` its
/// `client_id`, carrying every Annex attribute and a purpose of use, and an
/// IUA `person_id`.
pub(crate) fn relaying() -> Claims {
    let mut claims = support::claims();
    claims.sub.clone_from(&claims.client_id);
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
    let names = claim_names();
    for (claim, value) in [
        (names.family_name, FAMILY),
        (names.given_name, GIVEN),
        (names.country_code, COUNTRY),
        (names.professional_issuing_authority, HP_AUTHORITY),
        (names.provider_issuing_authority, HCP_AUTHORITY),
        (names.provider_address, HCP_ADDRESS),
    ] {
        claims.other.insert(claim, value.to_owned());
    }
    claims
}
