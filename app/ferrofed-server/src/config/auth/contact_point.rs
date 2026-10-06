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
}

impl ContactPointClaims {
    /// Resolves this table at `key`: every claim name set, and the
    /// correlation header, when declared, an HTTP field name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Missing`] for a claim name not set, and
    /// [`Error::Auth`] with [`AuthFault::HeaderName`] for a correlation
    /// header that is no field name.
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
        Ok(ContactPoint {
            claims: self.clone(),
            correlation_header,
        })
    }
}
