// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national-contact-point caller.
//!
//! An issuer the deployment declares as a national contact point for digital
//! health vouches for its national connector, which relays the request of a
//! health professional of another Member State. Implementing Regulation (EU) 2026/2099 Art 6(1) and (2) give the
//! identification, authentication and authorisation of that professional to
//! the entity their Member State lists, and Art 7 has the contact point
//! communicate the professional's and the provider's identification data,
//! the attributes of its Annex Tables 1 and 2. The gateway verifies the
//! contact point's token and reads those attributes from it as the contact
//! point asserts them: it never authenticates the professional, and the
//! Federation Tier lets the source "rely on the requester's assertion" (§13.4
//! authn-end-user). A request that reaches patient data needs every
//! attribute and a purpose of use (§13.4 authn-purpose-of-use).
//!
//! The IHE IUA extension carries some of the attributes (ITI TF-2
//! 3.71.4.2.2.1.1): `national_provider_identifier` is `hp_identifier`,
//! `subject_role` is `hp_professional_role`, `subject_organization_id` is
//! `healthcare_provider_identifier` and `subject_organization` is
//! `healthcare_provider_name`. The issuer's
//! `[trust.issuer.national_contact_point]` names the string claim of each
//! other one. The IUA `person_id`, a patient identifier, is never read
//! (§5.4.1, N33).

use ferrofed_engine::conveyance::relayed::{HealthProfessional, HealthcareProvider, Relayed};
use http::HeaderMap;

use crate::auth::caller::Caller;
use crate::auth::claims::IuaAnnex;
use crate::auth::refusal::{Refusal, Refused};
use crate::config::auth::IssuerSettings;
use crate::config::auth::contact_point::{ContactPoint, is_alpha_2};

/// The most bytes a correlation identifier may carry.
// NOTE: no specification governs this: our own design; 128 holds a UUID, a URN or a
// trace id with room, and bounds what the access record stores from a header.
pub const MAX_CORRELATION: usize = 128;

/// One attribute of Implementing Regulation (EU) 2026/2099 Annex Tables 1
/// and 2, in the tables' order.
const ANNEX: [&str; 10] = [
    "Table 1 family_name",
    "Table 1 given_name",
    "Table 1 hp_identifier",
    "Table 1 issuing_authority_name",
    "Table 1 hp_professional_role",
    "Table 1 country_code",
    "Table 2 healthcare_provider_identifier",
    "Table 2 issuing_authority_name",
    "Table 2 healthcare_provider_name",
    "Table 2 healthcare_provider_address",
];

/// The Annex attributes a contact point's token lacks or carries in a form
/// the gateway does not admit, named and never valued.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MissingAnnex(u16);

impl MissingAnnex {
    /// Returns this set with the attribute at `index` of [`ANNEX`] added.
    #[must_use]
    const fn with(self, index: usize) -> Self {
        Self(self.0 | (1 << index))
    }

    /// Returns whether no attribute is missing.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The Annex names of the missing attributes, in the tables' order.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        ANNEX
            .into_iter()
            .enumerate()
            .filter(move |(index, _)| self.0 & (1 << index) != 0)
            .map(|(_, name)| name)
    }
}

/// Whom a token's issuer vouches for, as [`read`] finds it.
#[derive(Debug)]
pub(super) enum Vouched {
    /// An issuer not declared a contact point.
    Own,
    /// A contact point, its token relaying what [`relayed`] read, or the
    /// Annex attributes it lacks.
    ContactPoint(Result<Box<Relayed>, MissingAnnex>),
}

/// Whom a token of `issuer` vouches for: a contact point's caller with what
/// [`relayed`] reads from the IUA extension's `iua` and the string claims
/// `text` reads, when the issuer is declared one.
pub(super) fn read(
    issuer: &IssuerSettings,
    iua: IuaAnnex,
    text: impl Fn(&str) -> Option<String>,
) -> Vouched {
    match &issuer.national_contact_point {
        Some(declared) => {
            Vouched::ContactPoint(relayed((issuer.issuer.as_str(), declared), iua, text))
        }
        None => Vouched::Own,
    }
}

/// Returns `caller` as [`read`] found it: vouched for by a contact point,
/// with what its token relays, or as it is.
pub(super) fn vouched(caller: Caller, read: Vouched) -> Caller {
    match read {
        Vouched::ContactPoint(relayed) => caller.relayed_by_contact_point(relayed),
        Vouched::Own => caller,
    }
}

/// Holds a caller a national contact point vouched for, admitted to patient
/// data, to what Implementing Regulation (EU) 2026/2099 Art 7 has the
/// contact point communicate, and to a purpose of use, whatever the
/// deployment's `trust.purpose_of_use.required` (Federation Tier §13.4
/// authn-purpose-of-use).
///
/// # Errors
///
/// Returns [`Refusal::PurposeOfUse`] when the token declares no purpose of
/// use, and [`Refusal::ContactPoint`] with the attributes it lacks when it
/// does not carry every Annex attribute.
pub(super) fn relaying(caller: &Caller) -> Result<(), Refused> {
    if !caller.is_contact_point() {
        return Ok(());
    }
    if caller.purposes().is_empty() {
        return Err(Refusal::PurposeOfUse.into());
    }
    if caller.relayed().is_none() {
        return Err(Refused::missing(caller.annex_missing()));
    }
    Ok(())
}

/// What the contact point `issuer`, declared as `declared`, relays in a
/// token whose IUA extension states `iua` and whose string claims `text`
/// reads, when the token carries every Annex attribute.
///
/// # Errors
///
/// Returns every Annex attribute that is absent or empty, the role when no
/// role has a code, and `country_code` when it is not the ISO 3166-1
/// alpha-2 code of a Member State, or of a country the deployment adds for
/// this contact point ([`ContactPoint::admits_country`]).
fn relayed(
    (issuer, declared): (&str, &ContactPoint),
    iua: IuaAnnex,
    text: impl Fn(&str) -> Option<String>,
) -> Result<Box<Relayed>, MissingAnnex> {
    let claims = &declared.claims;
    // NOTE: 2026/2099 Annex Table 1 names "the Member State that issued" the data, and 2025/327
    // Art 24(3) lets the Commission connect a third country's contact point, which the deployment adds.
    let country_code =
        text(&claims.country_code).filter(|code| is_alpha_2(code) && declared.admits_country(code));
    let roles = Some(iua.roles).filter(|roles| !roles.is_empty());
    // NOTE: ITI TF-2 3.71.4.2.2.1.1: `subject_name` is "The user's name as String", which
    // cannot be split into the Annex family and given names, so both are configured claims.
    let read = (
        text(&claims.family_name),
        text(&claims.given_name),
        iua.hp_identifier,
        text(&claims.professional_issuing_authority),
        roles,
        country_code,
        iua.provider_identifier,
        text(&claims.provider_issuing_authority),
        iua.provider_name,
        text(&claims.provider_address),
    );
    match read {
        (
            Some(family_name),
            Some(given_name),
            Some(hp_identifier),
            Some(professional_authority),
            Some(hp_professional_role),
            Some(country_code),
            Some(identifier),
            Some(provider_authority),
            Some(name),
            Some(address),
        ) => Ok(Box::new(Relayed {
            contact_point: issuer.to_owned(),
            country_code,
            professional: HealthProfessional {
                family_name,
                given_name,
                hp_identifier,
                issuing_authority_name: professional_authority,
                hp_professional_role,
            },
            provider: HealthcareProvider {
                identifier,
                issuing_authority_name: provider_authority,
                name,
                address,
            },
        })),
        read => {
            let absent = [
                read.0.is_none(),
                read.1.is_none(),
                read.2.is_none(),
                read.3.is_none(),
                read.4.is_none(),
                read.5.is_none(),
                read.6.is_none(),
                read.7.is_none(),
                read.8.is_none(),
                read.9.is_none(),
            ];
            Err(absent
                .into_iter()
                .enumerate()
                .filter(|(_, absent)| *absent)
                .fold(MissingAnnex::default(), |missing, (index, _)| {
                    missing.with(index)
                }))
        }
    }
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
