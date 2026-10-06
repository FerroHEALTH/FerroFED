// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a national contact point for digital health relays with a request.
//!
//! The health professional and the healthcare provider, as the contact point
//! asserts them (Implementing Regulation (EU) 2026/2099 Art 7 and its Annex
//! Tables 1 and 2).
//!
//! The gateway verified the contact point's token, and never the
//! professional: Art 6(1) and (2) give the identification and
//! authentication of the professional to the entity the professional's
//! Member State lists, and Federation Tier §13.4 (authn-end-user) lets the
//! source rely on the requester's assertion. Every value here is the
//! contact point's, and travels to a node marked as its assertion.

use std::fmt;

use serde::Serialize;

/// The professional and the provider a contact point relays, with the
/// contact point that asserted them.
///
/// `Debug` shows the country and no other value.
#[derive(Clone, PartialEq, Eq)]
pub struct Relayed {
    /// The contact point that asserted them, by the issuer of its token.
    pub contact_point: String,
    /// `country_code`: the ISO 3166-1 alpha-2 code of the Member State that
    /// issued the professional's identification data (Annex Table 1).
    pub country_code: String,
    /// The health professional (Annex Table 1).
    pub professional: HealthProfessional,
    /// The healthcare provider (Annex Table 2).
    pub provider: HealthcareProvider,
}

impl fmt::Debug for Relayed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Relayed")
            .field("country_code", &self.country_code)
            .finish_non_exhaustive()
    }
}

/// The health professional a contact point relays (2026/2099 Annex
/// Table 1); the provider identifier the table repeats is the
/// [`HealthcareProvider`]'s.
#[derive(Clone, PartialEq, Eq)]
pub struct HealthProfessional {
    /// `family_name`.
    pub family_name: String,
    /// `given_name`.
    pub given_name: String,
    /// `hp_identifier`, unique among the identifiers the Member State where
    /// the professional is registered issues.
    pub hp_identifier: String,
    /// `issuing_authority_name`: the agency that issued `hp_identifier`.
    pub issuing_authority_name: String,
    /// `hp_professional_role`, each a code the contact point states.
    pub hp_professional_role: Vec<Role>,
}

/// The healthcare provider a contact point relays (2026/2099 Annex
/// Table 2): where the professional provides the treatment.
#[derive(Clone, PartialEq, Eq)]
pub struct HealthcareProvider {
    /// `healthcare_provider_identifier`, unique in the Member State that
    /// issued it.
    pub identifier: String,
    /// `issuing_authority_name`: the agency that issued the identifier.
    pub issuing_authority_name: String,
    /// `healthcare_provider_name`.
    pub name: String,
    /// `healthcare_provider_address`: the official registered full address.
    pub address: String,
}

impl fmt::Debug for HealthProfessional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthProfessional")
            .field("roles", &self.hp_professional_role.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for HealthcareProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HealthcareProvider").finish_non_exhaustive()
    }
}

/// A professional role: a code and the system that defines it, as an IHE
/// IUA `subject_role` FHIR `Coding` carries it (ITI TF-2 3.71.4.2.2.1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Role {
    /// The code system, when the contact point names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

impl Relayed {
    /// Every value the contact point asserted, which the outbound gate reads
    /// as it reads every other caller claim.
    #[must_use]
    pub fn carried(&self) -> Vec<&str> {
        let professional = &self.professional;
        let provider = &self.provider;
        let mut carried = vec![
            self.contact_point.as_str(),
            self.country_code.as_str(),
            professional.family_name.as_str(),
            professional.given_name.as_str(),
            professional.hp_identifier.as_str(),
            professional.issuing_authority_name.as_str(),
            provider.identifier.as_str(),
            provider.issuing_authority_name.as_str(),
            provider.name.as_str(),
            provider.address.as_str(),
        ];
        for role in &professional.hp_professional_role {
            carried.extend(role.system.as_deref());
            carried.push(&role.code);
        }
        carried
    }

    /// The claim a node is told this in, every attribute under its Annex
    /// data identifier and the contact point under `asserted_by`.
    pub(super) fn claim(&self) -> Claim<'_> {
        let professional = &self.professional;
        let provider = &self.provider;
        Claim {
            asserted_by: &self.contact_point,
            health_professional: ProfessionalClaim {
                family_name: &professional.family_name,
                given_name: &professional.given_name,
                country_code: &self.country_code,
                hp_identifier: &professional.hp_identifier,
                issuing_authority_name: &professional.issuing_authority_name,
                hp_professional_role: &professional.hp_professional_role,
                healthcare_provider_identifier: &provider.identifier,
            },
            healthcare_provider: ProviderClaim {
                healthcare_provider_identifier: &provider.identifier,
                issuing_authority_name: &provider.issuing_authority_name,
                healthcare_provider_name: &provider.name,
                healthcare_provider_address: &provider.address,
            },
        }
    }
}

/// The `national_contact_point` claim: Annex Table 1 and Table 2 as the
/// Annex lists them, and who asserted them.
#[derive(Serialize)]
pub(super) struct Claim<'a> {
    asserted_by: &'a str,
    health_professional: ProfessionalClaim<'a>,
    healthcare_provider: ProviderClaim<'a>,
}

/// Annex Table 1, every row.
#[derive(Serialize)]
struct ProfessionalClaim<'a> {
    family_name: &'a str,
    given_name: &'a str,
    country_code: &'a str,
    hp_identifier: &'a str,
    issuing_authority_name: &'a str,
    hp_professional_role: &'a [Role],
    healthcare_provider_identifier: &'a str,
}

/// Annex Table 2, every row.
#[derive(Serialize)]
struct ProviderClaim<'a> {
    healthcare_provider_identifier: &'a str,
    issuing_authority_name: &'a str,
    healthcare_provider_name: &'a str,
    healthcare_provider_address: &'a str,
}
