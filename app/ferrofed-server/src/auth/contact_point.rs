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
//! `[auth.issuer.national_contact_point]` names the string claim of each
//! other one. The IUA `person_id`, a patient identifier, is never read
//! (§5.4.1, N33).

use ferrofed_engine::conveyance::relayed::{HealthProfessional, HealthcareProvider, Relayed};
use http::HeaderMap;

use crate::auth::caller::Caller;
use crate::auth::claims::IuaAnnex;
use crate::auth::refusal::Refusal;
use crate::config::auth::IssuerSettings;
use crate::config::auth::contact_point::ContactPoint;

/// The most bytes a correlation identifier may carry.
// NOTE: no specification governs this: our own design; 128 holds a UUID, a URN or a
// trace id with room, and bounds what the access record stores from a header.
pub const MAX_CORRELATION: usize = 128;

/// Whom a token's issuer vouches for, as [`read`] finds it.
#[derive(Debug)]
pub(super) enum Vouched {
    /// An issuer not declared a contact point.
    Own,
    /// A contact point, its token relaying what [`relayed`] read, or `None`
    /// when it lacks an Annex attribute.
    ContactPoint(Option<Box<Relayed>>),
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
/// deployment's `auth.purpose_of_use.required` (Federation Tier §13.4
/// authn-purpose-of-use).
///
/// # Errors
///
/// Returns [`Refusal::PurposeOfUse`] when the token declares no purpose of
/// use, and [`Refusal::ContactPoint`] when it does not carry every Annex
/// attribute.
pub(super) fn relaying(caller: &Caller) -> Result<(), Refusal> {
    if !caller.is_contact_point() {
        return Ok(());
    }
    if caller.purposes().is_empty() {
        return Err(Refusal::PurposeOfUse);
    }
    if caller.relayed().is_none() {
        return Err(Refusal::ContactPoint);
    }
    Ok(())
}

/// What the contact point `issuer`, declared as `declared`, relays in a
/// token whose IUA extension states `iua` and whose string claims `text`
/// reads, when the token carries every Annex attribute.
///
/// `None` when any attribute is absent or empty, when no role has a code,
/// or when `country_code` is not two upper-case letters, the form of an
/// ISO 3166-1 alpha-2 code the Annex names.
fn relayed(
    (issuer, declared): (&str, &ContactPoint),
    iua: IuaAnnex,
    text: impl Fn(&str) -> Option<String>,
) -> Option<Box<Relayed>> {
    let claims = &declared.claims;
    let country_code = text(&claims.country_code).filter(|code| alpha_2(code))?;
    if iua.roles.is_empty() {
        return None;
    }
    Some(Box::new(Relayed {
        contact_point: issuer.to_owned(),
        country_code,
        professional: HealthProfessional {
            // NOTE: ITI TF-2 3.71.4.2.2.1.1: `subject_name` is "The user's name as String", which
            // cannot be split into the Annex family and given names, so both are configured claims.
            family_name: text(&claims.family_name)?,
            given_name: text(&claims.given_name)?,
            hp_identifier: iua.hp_identifier?,
            issuing_authority_name: text(&claims.professional_issuing_authority)?,
            hp_professional_role: iua.roles,
        },
        provider: HealthcareProvider {
            identifier: iua.provider_identifier?,
            issuing_authority_name: text(&claims.provider_issuing_authority)?,
            name: iua.provider_name?,
            address: text(&claims.provider_address)?,
        },
    }))
}

/// Whether `code` has the form of an ISO 3166-1 alpha-2 code: two
/// upper-case ASCII letters.
// NOTE: 2026/2099 Annex Table 1 `country_code`; the form is checked, and which Member
// State it names is the contact point's assertion (no specification lists the codes held).
fn alpha_2(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_uppercase())
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

#[cfg(test)]
mod tests {
    use super::alpha_2;

    #[test]
    fn only_two_upper_case_letters_have_the_alpha_2_form() {
        for code in ["NL", "XA", "BE"] {
            assert!(alpha_2(code), "{code}");
        }
        for code in ["", "N", "nl", "NLD", "N1", "ÑL"] {
            assert!(!alpha_2(code), "{code}");
        }
    }
}
