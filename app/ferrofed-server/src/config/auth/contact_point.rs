// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[[federation.contact_point]]`, as written and as resolved.
//!
//! A national contact point for digital health is an `[[auth.issuer]]` of
//! kind `federation` whose `[[auth.federation]]` table sets
//! `contact_point = true`: `ferrohealth-smart` holds its tokens to the
//! MyHealth@EU contact-point profile (eHDSI SAML Profile 10.0.0 §2.1, §2.3)
//! and reads what they assert. Its connector relays the requests of health
//! professionals of another Member State (Regulation (EU) 2025/327 Art 23;
//! Implementing Regulation (EU) 2026/2099 Art 7). One
//! `[[federation.contact_point]]` per such issuer holds what the gateway
//! itself needs of it: the header its correlation identifier travels in,
//! and the countries beside the Member States its professionals may come
//! from. No specification governs the keys: our own design.

use ferrohealth_secret::secret_url::SecretUrl;
use std::collections::BTreeSet;

use ferrohealth_smart::federation::{FederationSection, Field};
use http::HeaderName;
use serde::Deserialize;

use crate::config::auth::{AuthFault, IssuerSettings, fault};
use crate::config::error::Error;

/// One `[[federation.contact_point]]` as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactPointTable {
    /// The `[[auth.issuer]]` the table describes, a `federation` issuer
    /// whose `[[auth.federation]]` table sets `contact_point = true`; a
    /// password in its userinfo reaches no rendering.
    pub issuer: SecretUrl,
    /// The request header the contact point's national connector sends its
    /// correlation identifier in, recorded in the access record so it can be
    /// joined with the contact point's own log; absent by default, and then
    /// none is read.
    pub correlation_header: Option<String>,
    /// The ISO 3166-1 alpha-2 codes, beside the Member States
    /// ([`MEMBER_STATES`]), the `[[auth.federation]] countries` of this
    /// contact point may name: a third country whose contact point the
    /// Commission connected to MyHealth@EU (Regulation (EU) 2025/327
    /// Art 24(3)); none by default.
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
    /// The correlation header, when one is declared.
    pub correlation_header: Option<HeaderName>,
    /// The codes beside [`MEMBER_STATES`] the issuer's `countries` may name.
    pub additional_countries: BTreeSet<String>,
}

impl ContactPoint {
    /// Whether this contact point's professionals may come from `code`: a
    /// Member State, or a country the deployment adds for it.
    #[must_use]
    pub fn admits_country(&self, code: &str) -> bool {
        MEMBER_STATES.contains(&code) || self.additional_countries.contains(code)
    }
}

impl ContactPointTable {
    /// Resolves this table at `key`: the correlation header, when declared,
    /// an HTTP field name that carries no credential, and each additional
    /// country an alpha-2 code of no Member State.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Auth`] with [`AuthFault::HeaderName`] or
    /// [`AuthFault::CorrelationCredential`] for a correlation header that is
    /// no field name or carries a credential, and with
    /// [`AuthFault::CountryCode`] for an additional country that is no
    /// alpha-2 code or is already a Member State.
    pub fn resolve(&self, key: &str) -> Result<ContactPoint, Error> {
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
            correlation_header,
            additional_countries,
        })
    }
}

/// Declares each issuer of `issuers` that `tables` names a national contact
/// point, as its table resolves.
///
/// # Errors
///
/// Returns [`Error::Missing`] for a table naming no issuer, or a
/// contact-point table of `[[auth.federation]]` naming no source of
/// `provider.issuing_authority_name`, which the access record names the
/// provider's identifier by; and [`Error::Auth`] for a table that names an
/// issuer twice or names no contact-point issuer
/// ([`AuthFault::NotContactPoint`]), a contact-point issuer with no table
/// ([`AuthFault::ContactPointUndeclared`]), a `countries` code that is
/// neither a Member State nor one of the table's additional countries
/// ([`AuthFault::CountryCode`]), and the errors of
/// [`ContactPointTable::resolve`].
pub fn declare(issuers: &mut [IssuerSettings], tables: &[ContactPointTable]) -> Result<(), Error> {
    let mut named: Vec<&str> = Vec::new();
    for (index, table) in tables.iter().enumerate() {
        let key = format!("federation.contact_point[{index}]");
        if table.issuer.is_empty() {
            return Err(Error::Missing {
                key: format!("{key}.issuer"),
            });
        }
        if named.contains(&table.issuer.expose()) {
            return Err(fault(&format!("{key}.issuer"), AuthFault::DuplicateIssuer));
        }
        named.push(table.issuer.expose());
        let issuer = issuers
            .iter_mut()
            .find(|issuer| {
                issuer.issuer == table.issuer.expose() && issuer.verification.is_contact_point()
            })
            .ok_or_else(|| fault(&format!("{key}.issuer"), AuthFault::NotContactPoint))?;
        let declared = table.resolve(&key)?;
        if let Some(section) = issuer.verification.federation() {
            checked(section, &declared)?;
        }
        issuer.national_contact_point = Some(declared);
    }
    if let Some(index) = issuers.iter().position(|issuer| {
        issuer.verification.is_contact_point() && issuer.national_contact_point.is_none()
    }) {
        return Err(fault(
            &format!("auth.issuer[{index}]"),
            AuthFault::ContactPointUndeclared,
        ));
    }
    Ok(())
}

/// Holds the `[[auth.federation]]` table of a contact point to what the
/// gateway reads from it: a Member State or a declared additional country
/// for every `countries` code, and a source of the provider's issuing
/// authority.
fn checked(section: &FederationSection, declared: &ContactPoint) -> Result<(), Error> {
    // NOTE: 2026/2099 Annex Table 1 names "the Member State that issued" the data, and 2025/327
    // Art 24(3) lets the Commission connect a third country's contact point, which the deployment adds.
    if section
        .countries
        .iter()
        .any(|code| !declared.admits_country(code))
    {
        return Err(fault("auth.federation.countries", AuthFault::CountryCode));
    }
    if !section
        .sources()
        .contains_key(&Field::ProviderIssuingAuthorityName)
    {
        return Err(Error::Missing {
            key: format!(
                "auth.federation.fields.\"{}\"",
                Field::ProviderIssuingAuthorityName.as_str()
            ),
        });
    }
    Ok(())
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
