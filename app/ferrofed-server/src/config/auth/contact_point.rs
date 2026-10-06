// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth.issuer.national_contact_point]`, as written and as resolved.
//!
//! The table declares an issuer as a national contact point for digital
//! health, whose national connector relays the requests of health
//! professionals of another Member State (Regulation (EU) 2025/327
//! Art 23; Implementing Regulation (EU) 2026/2099 Art 7). Its tokens carry
//! every attribute of the 2026/2099 Annex Tables 1 and 2: the ones IHE IUA
//! defines a claim for are read from it (ITI TF-2 3.71.4.2.2.1.1), and the
//! table names the string claim of each other one. The Annex defines no
//! carrier, so no claim name has a default: no specification governs this:
//! our own design.

use std::collections::BTreeSet;

use http::HeaderName;
use serde::Deserialize;

use crate::config::auth::{AuthFault, fault};
use crate::config::error::Error;

/// `[auth.issuer.national_contact_point]`: the names of the string claims
/// that carry the 2026/2099 Annex attributes IHE IUA defines no claim for,
/// and the header a correlation identifier travels in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactPointClaims {
    /// The claim carrying the professional's `family_name` (Table 1).
    pub family_name: String,
    /// The claim carrying the professional's `given_name` (Table 1).
    pub given_name: String,
    /// The claim carrying `country_code`, the ISO 3166-1 alpha-2 code of
    /// the Member State that issued the professional's identification data
    /// (Table 1).
    pub country_code: String,
    /// The claim carrying the `issuing_authority_name` of the professional's
    /// `hp_identifier` (Table 1).
    pub professional_issuing_authority: String,
    /// The claim carrying the `issuing_authority_name` of the
    /// `healthcare_provider_identifier` (Table 2).
    pub provider_issuing_authority: String,
    /// The claim carrying the `healthcare_provider_address` (Table 2).
    pub provider_address: String,
    /// The request header the contact point's national connector sends its
    /// correlation identifier in, recorded in the access record so it can be
    /// joined with the contact point's own log; absent by default, and then
    /// none is read.
    pub correlation_header: Option<String>,
    /// The ISO 3166-1 alpha-2 codes, beside the Member States
    /// ([`MEMBER_STATES`]), a `country_code` this contact point relays may
    /// name: a third country whose contact point the Commission connected
    /// to MyHealth@EU (Regulation (EU) 2025/327 Art 24(3)); none by default.
    pub additional_countries: Vec<String>,
}

/// The ISO 3166-1 alpha-2 codes of the EU Member States, in code order.
///
/// They are the countries a `country_code` names (Implementing Regulation
/// (EU) 2026/2099 Annex Table 1, "representing the Member State that issued
/// the health professional identification data").
pub const MEMBER_STATES: [&str; 27] = [
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];

/// Whether `code` has the form of an ISO 3166-1 alpha-2 code: two
/// upper-case ASCII letters.
#[must_use]
pub fn is_alpha_2(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())
}

/// The headers that carry a credential, which a correlation header may not
/// name, in the lower case `http` keeps a field name in.
const CREDENTIAL_HEADERS: [&str; 4] = ["authorization", "proxy-authorization", "cookie", "dpop"];

/// An issuer's declaration as a national contact point, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactPoint {
    /// The claim names, each set.
    pub claims: ContactPointClaims,
    /// The correlation header, when one is declared.
    pub correlation_header: Option<HeaderName>,
    /// The codes beside [`MEMBER_STATES`] a `country_code` may name.
    pub additional_countries: BTreeSet<String>,
}

impl ContactPoint {
    /// Whether a `country_code` this contact point relays may name `code`:
    /// a Member State, or a country the deployment adds for it.
    #[must_use]
    pub fn admits_country(&self, code: &str) -> bool {
        MEMBER_STATES.contains(&code) || self.additional_countries.contains(code)
    }
}

impl ContactPointClaims {
    /// Resolves this table at `key`: every claim name set, and the
    /// correlation header, when declared, an HTTP field name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Missing`] for a claim name not set, and
    /// [`Error::Auth`] with [`AuthFault::HeaderName`] or
    /// [`AuthFault::CorrelationCredential`] for a correlation header that is
    /// no field name or carries a credential, and with
    /// [`AuthFault::CountryCode`] for an additional country that is no
    /// alpha-2 code or is already a Member State.
    pub fn resolve(&self, key: &str) -> Result<ContactPoint, Error> {
        for (name, claim) in [
            ("family_name", &self.family_name),
            ("given_name", &self.given_name),
            ("country_code", &self.country_code),
            (
                "professional_issuing_authority",
                &self.professional_issuing_authority,
            ),
            (
                "provider_issuing_authority",
                &self.provider_issuing_authority,
            ),
            ("provider_address", &self.provider_address),
        ] {
            if claim.is_empty() {
                return Err(Error::Missing {
                    key: format!("{key}.{name}"),
                });
            }
        }
        let at = format!("{key}.correlation_header");
        let correlation_header = self
            .correlation_header
            .as_deref()
            .map(|header| {
                let name = HeaderName::from_bytes(header.as_bytes())
                    .map_err(|_name| fault(&at, AuthFault::HeaderName))?;
                // NOTE: RFC 9110 §11.6.2, RFC 6265 §5.4, RFC 9449 §4.1: these carry credentials,
                // which no record holds (no specification governs the refusal: our own design).
                if CREDENTIAL_HEADERS.contains(&name.as_str()) {
                    return Err(fault(&at, AuthFault::CorrelationCredential));
                }
                Ok(name)
            })
            .transpose()?;
        let mut additional_countries = BTreeSet::new();
        for code in &self.additional_countries {
            if !is_alpha_2(code) || MEMBER_STATES.contains(&code.as_str()) {
                return Err(fault(
                    &format!("{key}.additional_countries"),
                    AuthFault::CountryCode,
                ));
            }
            additional_countries.insert(code.clone());
        }
        Ok(ContactPoint {
            claims: self.clone(),
            correlation_header,
            additional_countries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MEMBER_STATES, is_alpha_2};

    #[test]
    fn only_two_upper_case_letters_have_the_alpha_2_form() {
        for code in ["NL", "XA", "BE"] {
            assert!(is_alpha_2(code), "{code}");
        }
        for code in ["", "N", "nl", "NLD", "N1", "ÑL"] {
            assert!(!is_alpha_2(code), "{code}");
        }
    }

    #[test]
    fn the_member_states_are_27_alpha_2_codes_in_order_once_each() {
        assert_eq!(27, MEMBER_STATES.len());
        assert!(MEMBER_STATES.iter().all(|code| is_alpha_2(code)));
        assert!(
            MEMBER_STATES.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted, with no code twice"
        );
        for not_a_member in ["GB", "NO", "IS", "LI", "CH", "EL", "EU"] {
            assert!(!MEMBER_STATES.contains(&not_a_member), "{not_a_member}");
        }
    }
}
