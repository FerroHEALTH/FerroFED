// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The owner of the prescriptions a dispensation names, found among what the
//! members holding its patient answered.
//!
//! Each prescription named is matched at the gateway, never in a request to
//! a member: by its business identifier, a system and a value, or a value
//! alone when the dispensation names no system (FHIR R4 `Identifier.system`
//! 0..1), and by the id the face names a prescription by
//! ([`crate::fhir::locator`]), which matches only the member, the `ehr_id`
//! and the member's own id it names. A reference and an identifier in one
//! `authorizingPrescription` must name the same prescription. The matching
//! and its outcomes are our own design: no specification governs them.

use ferrofed_eehrxf::dispensations::Named;
use ferrofed_eehrxf::prescriptions::RESOURCE;
use ferrofed_eehrxf::prescriptions::page::Prescription;
use ferrofed_registry::id::EndpointId;

use crate::facade::prescriptions::{Held, Member};
use crate::fhir::locator::Locator;

/// Where the prescriptions a dispensation names are held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Owned {
    /// Every one is held by the member at this position of the members
    /// asked, once.
    At(usize),
    /// A reference names no prescription of the face's own form.
    Foreign,
    /// A reference and an identifier of one `authorizingPrescription` name
    /// different prescriptions, or one of them names none.
    Disagree,
    /// No member holds one of them for the patient.
    Unowned,
    /// More than one member holds one of them, each by its own endpoint.
    Claimed(Vec<EndpointId>),
    /// The member of this own endpoint holds one of them twice.
    Several(EndpointId),
    /// They are held by different members.
    Spanning,
}

/// The owner of every prescription of `named` among `members`, under the
/// face whose absolute base is `base`.
pub(crate) fn owner(named: &[Named], members: &[Member], base: &str) -> Owned {
    let serving: Vec<EndpointId> = members
        .iter()
        .map(|member| member.chosen.native.clone())
        .collect();
    let mut owners = Vec::new();
    for one in named {
        let matched = match matched(one, members, (&serving, base)) {
            Ok(matched) => matched,
            Err(refused) => return refused,
        };
        let mut holders: Vec<usize> = matched.iter().map(|(member, _)| *member).collect();
        holders.dedup();
        match holders.as_slice() {
            [] => return Owned::Unowned,
            [only] if matched.len() == 1 => owners.push(*only),
            [only] => {
                return members.get(*only).map_or(Owned::Unowned, |member| {
                    Owned::Several(member.chosen.native.clone())
                });
            }
            _ => {
                return Owned::Claimed(
                    holders
                        .iter()
                        .filter_map(|at| members.get(*at))
                        .map(|member| member.chosen.native.clone())
                        .collect(),
                );
            }
        }
    }
    owners.dedup();
    match owners.as_slice() {
        [only] => Owned::At(*only),
        [] => Owned::Unowned,
        _ => Owned::Spanning,
    }
}

/// Every `(member, prescription)` position `one` names among `members`,
/// in member order, or the refusal its parts give.
fn matched(
    one: &Named,
    members: &[Member],
    (serving, base): (&[EndpointId], &str),
) -> Result<Vec<(usize, usize)>, Owned> {
    let by_identifier = one
        .identifier
        .as_ref()
        .map(|identifier| every(members, |_, prescription| carries(prescription, identifier)));
    let by_reference = match &one.reference {
        None => None,
        Some(reference) => {
            let id = face_id(reference, base).ok_or(Owned::Foreign)?;
            Some(match Locator::read(id, serving) {
                Ok(located) => every(members, |member, prescription| {
                    member.chosen.native == *located.endpoint()
                        && member.chosen.ehr_id == *located.ehr_id()
                        && prescription.kept.id() == Some(located.resource())
                }),
                // NOTE: no specification governs this: our own design; an id the face did not
                // write, or that names no member asked, names no prescription held for the patient.
                Err(_unlocated) => Vec::new(),
            })
        }
    };
    match (by_identifier, by_reference) {
        (Some(identified), Some(referenced)) if identified != referenced => Err(Owned::Disagree),
        (Some(matched), _) | (None, Some(matched)) => Ok(matched),
        (None, None) => Ok(Vec::new()),
    }
}

/// Every `(member, prescription)` position of `members` whose prescription
/// `picks` admits.
fn every(
    members: &[Member],
    picks: impl Fn(&Member, &Prescription) -> bool,
) -> Vec<(usize, usize)> {
    members
        .iter()
        .enumerate()
        .flat_map(|(at, member)| {
            let held: &[_] = match &member.held {
                Held::Found(pages) => pages,
                Held::Unusable { .. } | Held::Silent(_) => &[],
            };
            held.iter()
                .flat_map(|page| page.prescriptions.iter())
                .enumerate()
                .filter(|(_, prescription)| picks(member, prescription))
                .map(move |(position, _)| (at, position))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `prescription` carries `identifier`, by its system and value,
/// or by its value alone when it names no system.
fn carries(prescription: &Prescription, (system, value): &(Option<String>, String)) -> bool {
    prescription
        .identifiers
        .iter()
        .any(|(held_system, held_value)| {
            held_value == value && system.as_ref().is_none_or(|system| system == held_system)
        })
}

/// The id of `reference` when it names a prescription of the face whose
/// absolute base is `base`, relative to it or absolute under it.
fn face_id<'r>(reference: &'r str, base: &str) -> Option<&'r str> {
    let base = base.trim_end_matches('/');
    let relative = reference
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(reference);
    let id = relative.strip_prefix(RESOURCE)?.strip_prefix('/')?;
    crate::fhir::locator::is_fhir_id(id).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::face_id;

    #[test]
    fn a_reference_names_a_prescription_of_the_face_relative_or_under_its_base() {
        let base = "https://gw.example.org/fhir";
        assert_eq!(Some("abc"), face_id("MedicationRequest/abc", base));
        assert_eq!(
            Some("abc"),
            face_id("https://gw.example.org/fhir/MedicationRequest/abc", base)
        );
        for foreign in [
            "https://other.example.org/fhir/MedicationRequest/abc",
            "MedicationDispense/abc",
            "MedicationRequest/abc/_history/1",
            "#rx",
        ] {
            assert_eq!(None, face_id(foreign, base), "{foreign}");
        }
    }
}
