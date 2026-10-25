// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Probable duplicate medication entries across members (hazard I-02 of the
//! clinical safety risk file).
//!
//! Two members can record one medicine for one patient, and the summary
//! lists each member's entry beside the other's, never merged. A reader may
//! then take one medicine for two. The summary marks a probable duplicate:
//! two medication entries from two different members whose medicine code,
//! strength and route are the same. Both entries stay as mapped; the
//! section's narrative gains one line naming both members. Which entries are
//! probable duplicates is our own design: no specification governs it.

use std::collections::BTreeSet;

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::dosage::Dosage;
use fhir_types::r4::medication::Medication;
use fhir_types::r4::medication_request::MedicationRequestMedication;
use fhir_types::r4::medication_statement::MedicationStatementMedication;
use fhir_types::r4::resource::Resource;

/// What makes two medication entries probable duplicates: the codings of the
/// medicine, its strength and the codings of its route, each as the mapped
/// resources state them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    medicine: BTreeSet<(String, String)>,
    strength: BTreeSet<String>,
    route: BTreeSet<(String, String)>,
    shown: String,
}

impl Key {
    /// Returns the key of the medication entry among `resources`, the
    /// resources one mapping run wrote: a `MedicationStatement` or a
    /// `MedicationRequest` whose medicine carries at least one coding, read
    /// from its `medicationCodeableConcept` or from a `Medication` the run
    /// wrote beside it. `None` for any other run.
    #[must_use]
    pub fn of(resources: &[Resource]) -> Option<Self> {
        let medication = resources.iter().find_map(|resource| match resource {
            Resource::Medication(medication) => Some(&**medication),
            _ => None,
        });
        let (concept, dosages) = resources.iter().find_map(|resource| match resource {
            Resource::MedicationStatement(statement) => {
                let concept = match &statement.medication {
                    MedicationStatementMedication::CodeableConcept(concept) => Some(&**concept),
                    MedicationStatementMedication::Reference(_) => None,
                };
                Some((concept, statement.dosage.as_slice()))
            }
            Resource::MedicationRequest(request) => {
                let concept = match &request.medication {
                    MedicationRequestMedication::CodeableConcept(concept) => Some(&**concept),
                    MedicationRequestMedication::Reference(_) => None,
                };
                Some((concept, request.dosage_instruction.as_slice()))
            }
            _ => None,
        })?;
        let concept =
            concept.or_else(|| medication.and_then(|medication| medication.code.as_ref()))?;
        let medicine = codings(concept);
        if medicine.is_empty() {
            return None;
        }
        Some(Self {
            medicine,
            strength: strength(medication),
            route: routes(dosages),
            shown: shown(concept),
        })
    }

    /// Whether `other` names the same medicine, strength and route.
    fn same(&self, other: &Self) -> bool {
        self.medicine == other.medicine
            && self.strength == other.strength
            && self.route == other.route
    }
}

/// The system and code of every coding of `concept` that carries a code.
fn codings(concept: &CodeableConcept) -> BTreeSet<(String, String)> {
    concept
        .coding
        .iter()
        .filter_map(|coding| {
            let code = coding.code.as_ref()?.value.clone()?;
            let system = coding
                .system
                .as_ref()
                .and_then(|system| system.value.clone())
                .unwrap_or_default();
            Some((system, code))
        })
        .collect()
}

/// Every ingredient strength `medication` states, each as its `Debug`
/// rendering, which compares two strengths field by field.
fn strength(medication: Option<&Medication>) -> BTreeSet<String> {
    medication
        .into_iter()
        .flat_map(|medication| medication.ingredient.iter())
        .filter_map(|ingredient| ingredient.strength.as_ref())
        .map(|strength| format!("{strength:?}"))
        .collect()
}

/// The codings of every route `dosages` name.
fn routes(dosages: &[Dosage]) -> BTreeSet<(String, String)> {
    dosages
        .iter()
        .filter_map(|dosage| dosage.route.as_ref())
        .flat_map(codings)
        .collect()
}

/// The name a narrative shows for `concept`: its text, or the display or
/// code of its first coding.
fn shown(concept: &CodeableConcept) -> String {
    concept
        .text
        .as_ref()
        .and_then(|text| text.value.clone())
        .or_else(|| {
            concept.coding.first().and_then(|coding| {
                coding
                    .display
                    .as_ref()
                    .and_then(|display| display.value.clone())
                    .or_else(|| coding.code.as_ref().and_then(|code| code.value.clone()))
            })
        })
        .unwrap_or_default()
}

/// Returns the narrative line of every probable duplicate among `entries`.
///
/// Each entry is a member's display name and the key of one of its entries.
/// There is one line per medicine that two or more members record alike,
/// naming those members, in the order the first of them gave it.
#[must_use]
pub fn lines(entries: &[(String, Key)]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut marked: Vec<&Key> = Vec::new();
    for (index, (_, key)) in entries.iter().enumerate() {
        if marked.iter().any(|seen| seen.same(key)) {
            continue;
        }
        let mut members: Vec<&str> = Vec::new();
        for (member, other) in entries.iter().skip(index) {
            if other.same(key) && !members.contains(&member.as_str()) {
                members.push(member.as_str());
            }
        }
        if members.len() < 2 {
            continue;
        }
        marked.push(key);
        let medicine = if key.shown.is_empty() {
            String::from("one medicine")
        } else {
            format!("the medicine {}", key.shown)
        };
        lines.push(format!(
            "Probable duplicate: {medicine}, with the same strength and route, is recorded by {}. Both entries are listed as each member records them; check whether they are one medication.",
            join(&members)
        ));
    }
    lines
}

/// Joins `names` as a list in prose: "A and B", "A, B and C".
fn join(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, lines};

    fn key(code: &str, route: &str) -> Key {
        Key {
            medicine: [(String::from("http://www.whocc.no/atc"), code.to_owned())].into(),
            strength: std::collections::BTreeSet::new(),
            route: [(
                String::from("urn:oid:0.4.0.127.0.16.1.1.2.1"),
                route.to_owned(),
            )]
            .into(),
            shown: code.to_owned(),
        }
    }

    #[test]
    fn two_members_recording_one_medicine_alike_get_one_line() {
        let found = lines(&[
            (String::from("Synthetic org-a"), key("C09AA05", "20053000")),
            (String::from("Synthetic org-b"), key("C09AA05", "20053000")),
        ]);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].contains("Synthetic org-a and Synthetic org-b"),
            "{found:?}"
        );
    }

    #[test]
    fn one_member_twice_or_another_route_is_no_duplicate() {
        let none: Vec<String> = Vec::new();
        assert_eq!(
            lines(&[
                (String::from("Synthetic org-a"), key("C09AA05", "20053000")),
                (String::from("Synthetic org-a"), key("C09AA05", "20053000")),
            ]),
            none
        );
        assert_eq!(
            lines(&[
                (String::from("Synthetic org-a"), key("C09AA05", "20053000")),
                (String::from("Synthetic org-b"), key("C09AA05", "47625008")),
            ]),
            none
        );
    }
}
