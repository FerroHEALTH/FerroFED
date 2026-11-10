// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national-contact-point caller.
//!
//! A national contact point for digital health vouches for its national
//! connector, which relays the request of a health professional of another
//! Member State. Implementing Regulation (EU) 2026/2099 Art 6(1) and (2)
//! give the identification, authentication and authorisation of that
//! professional to the entity their Member State lists, and Art 7 has the
//! contact point communicate the professional's and the provider's
//! identification data. The gateway verifies the contact point's token and
//! never authenticates the professional: the Federation Tier lets the
//! source "rely on the requester's assertion" (§13.4 authn-end-user).
//!
//! `ferrohealth-smart` holds a `contact_point = true` issuer's token to the
//! MyHealth@EU contact-point profile, what the eHDSI HP Identity Assertion
//! and Treatment Relationship Confirmation carry (eHDSI SAML Profile 10.0.0
//! §2.1, §2.3, §4), and refuses one that falls short; the gateway checks no
//! field itself. It reads the crate's outcome ([`External`]) into what it
//! relays to a node ([`relayed`]): the professional by the assertion's
//! `NameID`, their role and organisation, the country and the purpose of
//! use, with no name (GDPR Art 5(1)(c)). The access record ([`record`]) also
//! names the professional by the assertion's `subject-id`, which
//! Implementing Regulation (EU) 2026/2099 Annex Table 1 makes mandatory, and
//! lists every further field the token asserts.
//!
//! The patient the treatment relationship confirmation binds travels in the
//! token's `patient.id` attribute, as one `system|value` token
//! ([`TrcPatient`]): the authorization server acting for the contact point
//! copies it from the TRC assertion's `subject-id` (eHDSI SAML Profile
//! 10.0.0 §4.3). The FHIR face and the ITS-REST face hold every patient-data
//! request to it.

use std::collections::BTreeMap;
use std::fmt;

use ehds_logging::record::{
    AuthenticationMethod, Coded, Gateways, NameId, RelayedError, RelayedParts, RelayedProfessional,
    RelayedProvider, Via,
};
use ferrofed_engine::conveyance::relayed::{HealthProfessional, HealthcareProvider, Relayed, Role};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrohealth_smart::federation::contact::TrcPatientId;
use ferrohealth_smart::federation::{Attribute, External, Field, Source};
use http::HeaderMap;
use secrecy::{ExposeSecret, SecretString};

use crate::auth::caller::PurposeOfUse;
use crate::auth::refusal::Refusal;
use crate::config::auth::AuthSettings;
use crate::config::auth::contact_point::ContactPoint;
use crate::config::emergency::EmergencyLimit;
use crate::federation::describe::NATIONAL_CONTACT_POINT;
use openehr_federation::error::WireError;
use openehr_federation::options::OptionsRoot;

/// The most bytes a correlation identifier may carry.
// NOTE: no specification governs this: our own design; 128 holds a UUID, a URN or a
// trace id with room, and bounds what the access record stores from a header.
pub const MAX_CORRELATION: usize = 128;

/// The patient a contact point's treatment relationship confirmation binds:
/// the namespace and the value of its `subject-id`, as the token's
/// `patient.id` attribute names them (eHDSI SAML Profile 10.0.0 §4.3).
///
/// The value is a [`SecretString`] and `Debug` shows the namespace alone, so
/// no rendering shows the identifier (§5.4.1, N33).
#[derive(Clone)]
pub struct TrcPatient {
    namespace: IdentifierNamespace,
    value: SecretString,
}

impl TrcPatient {
    /// The patient `id` names, or `None` when its system is no identifier
    /// namespace the cross-reference resolves.
    #[must_use]
    pub fn of(id: &TrcPatientId) -> Option<Self> {
        // NOTE: no specification governs this: our own design; a system the cross-reference
        // cannot name binds no patient it can resolve, so the request is refused.
        let namespace = IdentifierNamespace::new(id.system()).ok()?;
        Some(Self {
            namespace,
            value: SecretString::from(id.value().to_owned()),
        })
    }

    /// Whether `system`|`value` is this patient's identifier, compared on the
    /// assigning authority and the value (eHDSI XCA Profile 8.0.1 §4.2.1:
    /// the request's identifier "equals" the TRC's).
    #[must_use]
    pub fn is(&self, system: &str, value: &str) -> bool {
        self.namespace.as_str() == system && self.value.expose_secret() == value
    }

    /// The namespace of the identifier, the TRC `subject-id`'s system.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.namespace.as_str()
    }

    /// The value of the identifier.
    #[must_use]
    pub fn value(&self) -> &SecretString {
        &self.value
    }

    /// The identifier as `system|value`, for the pseudonym a security record
    /// names it by and never for a log line.
    #[must_use]
    pub fn token(&self) -> SecretString {
        SecretString::from(format!(
            "{}|{}",
            self.namespace.as_str(),
            self.value.expose_secret()
        ))
    }

    /// The patient reference the cross-reference resolves.
    ///
    /// # Errors
    ///
    /// [`PatientRefError`] for an empty value, which [`TrcPatientId`] never
    /// holds.
    pub fn patient(&self) -> Result<PatientRef, PatientRefError> {
        PatientRef::new(self.namespace.clone(), self.value.clone())
    }
}

impl fmt::Debug for TrcPatient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrcPatient")
            .field("namespace", &self.namespace.as_str())
            .finish_non_exhaustive()
    }
}

impl PartialEq for TrcPatient {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.value.expose_secret() == other.value.expose_secret()
    }
}

impl Eq for TrcPatient {}

/// The text of `field` in `external`, when the token asserts it as text.
fn text(external: &External, field: Field) -> Option<String> {
    external
        .field(field)
        .and_then(Attribute::text)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The codes of `field` in `external`: each coded value with its system, or
/// the text as a code with none.
fn codes(external: &External, field: Field) -> Vec<Role> {
    match external.field(field) {
        Some(Attribute::Codes(codes)) => codes
            .iter()
            .filter(|code| !code.code.is_empty())
            .map(|code| Role {
                system: code.system.clone().filter(|system| !system.is_empty()),
                code: code.code.clone(),
            })
            .collect(),
        Some(Attribute::Text(text)) if !text.is_empty() => vec![Role {
            system: None,
            code: text.clone(),
        }],
        _ => Vec::new(),
    }
}

/// What the contact point `issuer` relays in a token that asserts
/// `external`.
///
/// `None` when the token lacks the professional's identifier, its issuing
/// authority, a role, the country, or the provider's identifier and issuing
/// authority.
#[must_use]
pub fn relayed(issuer: &str, external: &External) -> Option<Relayed> {
    let hp_professional_role = codes(external, Field::ProfessionalHpProfessionalRole);
    if hp_professional_role.is_empty() {
        return None;
    }
    Some(Relayed {
        contact_point: issuer.to_owned(),
        country_code: text(external, Field::ProfessionalCountryCode)?,
        professional: HealthProfessional {
            hp_identifier: text(external, Field::ProfessionalHpIdentifier)?,
            issuing_authority_name: text(external, Field::ProfessionalIssuingAuthorityName)?,
            hp_professional_role,
            on_behalf_of: codes(external, Field::ProfessionalOnBehalfOf)
                .into_iter()
                .next(),
        },
        provider: HealthcareProvider {
            identifier: text(external, Field::ProviderHealthcareProviderIdentifier)?,
            issuing_authority_name: text(external, Field::ProviderIssuingAuthorityName)?,
        },
    })
}

/// The purposes of use a contact point's token asserts.
#[must_use]
pub fn purposes(external: &External) -> Vec<PurposeOfUse> {
    let mut purposes: Vec<PurposeOfUse> = codes(external, Field::PurposeOfUse)
        .into_iter()
        .map(|role| PurposeOfUse {
            system: role.system,
            code: role.code,
        })
        .collect();
    purposes.sort();
    purposes.dedup();
    purposes
}

/// Whether a contact point's token that asserts `external` carries the
/// permission `code`, as the whole value or its last `:`-separated part,
/// or `None` when it asserts no permission at all.
///
/// eHDSI SAML Profile 10.0.0 §2.5 lists the permissions an identity
/// assertion may carry and the least each operation needs.
#[must_use]
pub fn permits(external: &External, code: &str) -> Option<bool> {
    let permissions = codes(external, Field::ProfessionalPermissions);
    if permissions.is_empty() {
        return None;
    }
    Some(permissions.iter().any(|permission| {
        permission.code == code || permission.code.rsplit(':').next() == Some(code)
    }))
}

/// The patient a contact point's token binds, its `patient.id` in the
/// `system|value` form, when it names one.
#[must_use]
pub fn trc_patient(external: &External) -> Option<TrcPatient> {
    external.trc_patient().as_ref().and_then(TrcPatient::of)
}

/// The fields the access record holds at a member of its own, which
/// [`record`] leaves out of what it lists as asserted beside them.
///
/// The purposes of use are the accessor's, and the patient fields name the
/// record's subject, which the record holds apart from its accessor.
const RECORDED: [Field; 25] = [
    Field::ProfessionalName,
    Field::ProfessionalFamilyName,
    Field::ProfessionalGivenName,
    Field::ProfessionalCountryCode,
    Field::ProfessionalHpIdentifier,
    Field::ProfessionalIssuingAuthorityName,
    Field::ProfessionalHpProfessionalRole,
    Field::ProfessionalHealthcareProviderIdentifier,
    Field::ProfessionalAuthenticationMethod,
    Field::ProfessionalOnBehalfOf,
    Field::ProfessionalPermissions,
    Field::ProviderHealthcareProviderIdentifier,
    Field::ProviderIssuingAuthorityName,
    Field::ProviderHealthcareProviderName,
    Field::ProviderHealthcareProviderAddress,
    Field::ProviderFacilityType,
    Field::ProviderHomeCommunityId,
    Field::PointOfCare,
    Field::AssertionIssuer,
    Field::AssertionId,
    Field::TrcId,
    Field::PurposeOfUse,
    Field::PatientId,
    Field::PatientIdSource,
    Field::PatientPidIssuerCountry,
];

/// Every string `attribute` carries: its text, or each code as
/// `system|code`, or the code alone when it names no system.
fn strings(attribute: &Attribute) -> Vec<String> {
    match attribute {
        Attribute::Text(text) => vec![text.clone()],
        Attribute::Codes(codes) => codes
            .iter()
            .map(|code| match code.system.as_deref() {
                Some(system) if !system.is_empty() => format!("{system}|{}", code.code),
                _ => code.code.clone(),
            })
            .collect(),
    }
}

/// What a contact point relays, as the access record names it.
///
/// It is marked as relayed by a contact point, with each Implementing
/// Regulation (EU) 2026/2099 Annex field and eHDSI SAML Profile 10.0.0 §2.3
/// attribute its token asserts at its member, and every other field it
/// asserts by its name.
///
/// The professional's name is the assertion's `subject-id`, one full-name
/// string, which Annex Table 1 makes mandatory; their given and family names
/// are recorded only when the token states them apart.
///
/// # Errors
///
/// Returns the [`RelayedError`] of a token that asserts no name for the
/// professional, of a provider identifier that is no `urn:oid:` or
/// `urn:hl7ii:` identifier, and of an ISCO `44` role relayed without whom it
/// acts for (eHDSI SAML Profile 10.0.0 §2.3).
pub fn record(
    relayed: &Relayed,
    external: &External,
) -> Result<ehds_logging::record::Relayed, RelayedError> {
    let professional = &relayed.professional;
    let provider = &relayed.provider;
    let coded = |role: &Role| Coded {
        system: role.system.clone(),
        code: role.code.clone(),
    };
    let name_id = text(external, Field::AssertionIssuer).map(|issuer| NameId {
        value: professional.hp_identifier.clone(),
        sp_provided_id: None,
        issuer,
    });
    let asserted = external
        .fields()
        .iter()
        .filter(|(field, _)| !RECORDED.contains(field))
        .map(|(field, attribute)| (field.as_str().to_owned(), strings(attribute)))
        .filter(|(_, values)| !values.is_empty())
        .collect();
    ehds_logging::record::Relayed::try_from(RelayedParts {
        via: Via::ContactPoint {
            issuer: relayed.contact_point.clone(),
        },
        country_code: Some(relayed.country_code.clone()),
        professional: Some(RelayedProfessional {
            name: text(external, Field::ProfessionalName),
            family_name: text(external, Field::ProfessionalFamilyName),
            given_name: text(external, Field::ProfessionalGivenName),
            identifier: Some(professional.hp_identifier.clone()),
            issuing_authority: Some(professional.issuing_authority_name.clone()),
            roles: professional
                .hp_professional_role
                .iter()
                .map(coded)
                .collect(),
            // NOTE: eHDSI SAML Profile 10.0.0 §2.3: the assertion's organization-id names
            // where the professional treats, so it stands for Annex Table 1's identifier.
            healthcare_provider_identifier: text(
                external,
                Field::ProfessionalHealthcareProviderIdentifier,
            )
            .or_else(|| Some(provider.identifier.clone())),
            name_id,
            authentication_method: text(external, Field::ProfessionalAuthenticationMethod)
                .and_then(|uri| AuthenticationMethod::from_uri(&uri)),
            on_behalf_of: professional.on_behalf_of.as_ref().map(coded),
            permissions: codes(external, Field::ProfessionalPermissions)
                .into_iter()
                .map(|permission| permission.code)
                .collect(),
        }),
        provider: Some(RelayedProvider {
            identifier: provider.identifier.clone(),
            issuing_authority: Some(provider.issuing_authority_name.clone()),
            name: text(external, Field::ProviderHealthcareProviderName),
            address: text(external, Field::ProviderHealthcareProviderAddress),
            facility_type: codes(external, Field::ProviderFacilityType)
                .first()
                .map(coded),
            home_community_id: text(external, Field::ProviderHomeCommunityId),
            organisation_type: None,
        }),
        agreement: None,
        point_of_care: text(external, Field::PointOfCare),
        identity_assertion_id: text(external, Field::AssertionId),
        trc_assertion_id: text(external, Field::TrcId),
        message_id: None,
        gateways: Gateways::default(),
        asserted,
    })
}

/// The correlation identifier `headers` carry in the header `declared`
/// names, when it names one and the request sends it.
///
/// # Errors
///
/// Returns [`Refusal::Correlation`] when the header is sent more than once,
/// or its value is empty, longer than [`MAX_CORRELATION`] bytes, or holds a
/// byte outside visible ASCII.
pub(super) fn correlation(
    headers: &HeaderMap,
    declared: &ContactPoint,
) -> Result<Option<String>, Refusal> {
    let Some(name) = &declared.correlation_header else {
        return Ok(None);
    };
    let mut values = headers.get_all(name).iter();
    let (first, second) = (values.next(), values.next());
    let value = match (first, second) {
        (None, _) => return Ok(None),
        (Some(value), None) => value.as_bytes(),
        (Some(_), Some(_)) => return Err(Refusal::Correlation),
    };
    // NOTE: RFC 9110 §5.5: VCHAR is %x21-7E; no specification governs the identifier's
    // form: our own design, held to visible ASCII so the record stores what was sent.
    if value.is_empty()
        || value.len() > MAX_CORRELATION
        || !value.iter().all(|byte| matches!(byte, 0x21..=0x7E))
    {
        return Err(Refusal::Correlation);
    }
    String::from_utf8(value.to_vec())
        .map(Some)
        .map_err(|_unreachable| Refusal::Correlation)
}
/// The issuers a deployment declares as national contact points, as
/// `OPTIONS {base}/` declares them under `federation.national_contact_point`.
///
/// The member is present only when some issuer is declared one. It says that
/// such a request is served with `disclose: false`, whatever
/// `federation.consent.disclose` declares (Regulation (EU) 2025/327 Art 8,
/// Art 11(5)); that the professional is authenticated by the contact point's
/// side and asserted to the gateway, never authenticated by it (Implementing
/// Regulation (EU) 2026/2099 Art 6(1) and (2); §13.4 authn-end-user, CP-39);
/// that a patient-data request needs a purpose of use; the emergency access
/// limit in effect, the `threshold` of accesses one relayed professional is
/// granted in any `window` seconds (eHDSI Section II Security Services
/// 10.0.0 §2.5); and, per issuer, the claim each attribute is read from and
/// the correlation header.
// NOTE: §7a.2 leaves the `federation` object open and names no such member (N30 requires
// none), so the member is our own design.
#[derive(Debug, Clone, Default)]
pub struct ContactPoints(Vec<DeclaredContactPoint>);

/// One issuer declared a national contact point, as `OPTIONS {base}/`
/// declares it.
#[derive(Debug, Clone, serde::Serialize)]
struct DeclaredContactPoint {
    issuer: String,
    claims: BTreeMap<&'static str, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correlation_header: Option<String>,
}

impl ContactPoints {
    /// The national contact points `auth` declares.
    #[must_use]
    pub fn of(auth: &AuthSettings) -> Self {
        Self(
            auth.issuers
                .iter()
                .filter_map(|issuer| {
                    let declared = issuer.national_contact_point.as_ref()?;
                    let section = issuer.verification.federation()?;
                    // NOTE: no specification governs this: our own design; a constant is the
                    // deployment's own value, so only where a claim carries a field is declared.
                    let claims = section
                        .sources()
                        .into_iter()
                        .filter_map(|(field, sources)| {
                            sources.into_iter().find_map(|source| match source {
                                Source::Claim(path) => Some((field.as_str(), path)),
                                _ => None,
                            })
                        })
                        .collect();
                    Some(DeclaredContactPoint {
                        issuer: issuer.issuer.clone(),
                        claims,
                        correlation_header: declared
                            .correlation_header
                            .as_ref()
                            .map(|header| header.as_str().to_owned()),
                    })
                })
                .collect(),
        )
    }

    /// `body`, with the `national_contact_point` member when some issuer is
    /// declared one, declaring `emergency` as the emergency access limit in
    /// effect.
    ///
    /// # Errors
    ///
    /// Returns the [`WireError`] when the member cannot be written.
    pub fn declared_in(
        &self,
        mut body: OptionsRoot,
        emergency: EmergencyLimit,
    ) -> Result<OptionsRoot, WireError> {
        #[derive(serde::Serialize)]
        struct Member<'a> {
            disclose: bool,
            professional: &'static str,
            purpose_of_use: &'static str,
            emergency: Emergency,
            issuers: &'a [DeclaredContactPoint],
        }
        #[derive(serde::Serialize)]
        struct Emergency {
            threshold: u32,
            window_s: u64,
        }
        if self.0.is_empty() {
            return Ok(body);
        }
        let member = Member {
            disclose: false,
            professional: "asserted-by-contact-point",
            purpose_of_use: "required",
            emergency: Emergency {
                threshold: emergency.threshold.get(),
                window_s: emergency.window.as_secs(),
            },
            issuers: &self.0,
        };
        body.federation
            .extra
            .insert_serialized(NATIONAL_CONTACT_POINT, &member)?;
        Ok(body)
    }
}

/// The mark an answer to a national contact point carries, set by the gate
/// that admitted it, for the layers outside the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactPointAnswer;
