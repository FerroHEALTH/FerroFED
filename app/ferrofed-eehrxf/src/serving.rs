// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members that serve electronic prescriptions and electronic
//! dispensations (Regulation (EU) 2025/327 Art 14(1)(b) and (c)) on the
//! FHIR face.
//!
//! A member serves a category resource by resource, on its own
//! `hl7-fhir-rest` endpoint in the HL7 Europe Medication Prescription and
//! Dispense 1.0.0 profiles, and the face passes through what the member
//! that owns a prescription says: the member decides dispensability and
//! counts the remaining dispensations, and the gateway holds no lock (eHealth
//! Network ePrescription and eDispensation guideline Release 3.1 A.1.5.9 and
//! Supporting information Art 7). The deployment declares which members meet
//! that contract. A category no member is declared for is not served, and the
//! face says so on every route of it, never with an empty list, which the
//! information sheet's categories (Art 38(2)(d)) then match. Which members
//! are declared, and the rule that a dispensation member also serves
//! prescriptions, are our own design: no specification governs them.

use std::collections::{BTreeMap, BTreeSet};

use eehrxf::category::Category;
use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use fhir_types::r4::capability_statement::{
    CapabilityStatementRestResource, CapabilityStatementRestResourceInteraction,
    CapabilityStatementRestResourceSearchParam,
};

/// The categories a member serves resource by resource, in the order of
/// Art 14(1).
pub const SERVABLE: [Category; 2] = [Category::Prescription, Category::Dispensation];

/// Returns the canonical URL of the HL7 Europe MPD profile a member serves
/// `category` in, or `None` for a category no member serves resource by
/// resource.
#[must_use]
pub fn profile(category: Category) -> Option<&'static str> {
    match category {
        Category::Prescription => Some(MPD_MEDICATION_REQUEST),
        Category::Dispensation => Some(MPD_MEDICATION_DISPENSE),
        _ => None,
    }
}

/// One declaration as written: a category and the members that serve it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    /// The category, by its code ([`Category::code`]), such as
    /// `Electronic-Prescriptions`.
    pub category: String,
    /// The node ids of the members that serve it.
    pub members: Vec<String>,
}

/// Why a declaration is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ServingError {
    /// The code names no category a member serves resource by resource.
    #[error(
        "fhir.serve names the category {category}; a member serves Electronic-Prescriptions and Electronic-Dispensations alone"
    )]
    Unservable {
        /// The code as written.
        category: String,
    },
    /// The category is declared twice.
    #[error("fhir.serve declares the category {category} twice")]
    Twice {
        /// The category's code.
        category: &'static str,
    },
    /// The category names no member, or an empty one.
    #[error("fhir.serve for the category {category} names no member, or an empty one")]
    NoMember {
        /// The category's code.
        category: &'static str,
    },
    /// The category names one member twice.
    #[error("fhir.serve for the category {category} names the member {member} twice")]
    MemberTwice {
        /// The category's code.
        category: &'static str,
        /// The member.
        member: String,
    },
    /// A member serves dispensations and not prescriptions, so no
    /// dispensation could ever reach it.
    #[error(
        "fhir.serve names {member} for Electronic-Dispensations and not for Electronic-Prescriptions; a dispensation is written only at the member that owns its prescription"
    )]
    DispensingWithoutPrescribing {
        /// The member.
        member: String,
    },
}

/// The members each category is served by, as the deployment declares
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Serving {
    by: BTreeMap<Category, BTreeSet<String>>,
}

impl Serving {
    /// Reads the declarations `sources`.
    ///
    /// # Errors
    ///
    /// The [`ServingError`] of the first declaration that is refused.
    pub fn compile(sources: &[Source]) -> Result<Self, ServingError> {
        let mut by = BTreeMap::new();
        for source in sources {
            let category = Category::of_code(&source.category)
                .filter(|category| SERVABLE.contains(category))
                .ok_or_else(|| ServingError::Unservable {
                    category: source.category.clone(),
                })?;
            let code = category.code();
            if by.contains_key(&category) {
                return Err(ServingError::Twice { category: code });
            }
            if source.members.is_empty() || source.members.iter().any(String::is_empty) {
                return Err(ServingError::NoMember { category: code });
            }
            let mut members = BTreeSet::new();
            for member in &source.members {
                if !members.insert(member.clone()) {
                    return Err(ServingError::MemberTwice {
                        category: code,
                        member: member.clone(),
                    });
                }
            }
            by.insert(category, members);
        }
        // NOTE: no specification governs this: our own design; a dispensation goes to the
        // member owning its prescription, found among those serving prescriptions.
        let prescribing = by.get(&Category::Prescription);
        if let Some(dispensing) = by.get(&Category::Dispensation)
            && let Some(member) = dispensing
                .iter()
                .find(|member| !prescribing.is_some_and(|held| held.contains(*member)))
        {
            return Err(ServingError::DispensingWithoutPrescribing {
                member: member.clone(),
            });
        }
        Ok(Self { by })
    }

    /// Returns whether a member is declared for `category`.
    #[must_use]
    pub fn serves(&self, category: Category) -> bool {
        self.by.contains_key(&category)
    }

    /// Returns the members declared for `category`, ordered by node id, or
    /// `None` when none is.
    #[must_use]
    pub fn members_of(&self, category: Category) -> Option<&BTreeSet<String>> {
        self.by.get(&category)
    }

    /// Returns each served category with its members, in the order of Art
    /// 14(1), the members ordered by node id.
    pub fn members(&self) -> impl Iterator<Item = (Category, &BTreeSet<String>)> {
        self.by
            .iter()
            .map(|(category, members)| (*category, members))
    }

    /// Returns every category a member could serve that none is declared
    /// for, in the order of Art 14(1).
    pub fn unserved(&self) -> impl Iterator<Item = Category> {
        SERVABLE
            .into_iter()
            .filter(|category| !self.by.contains_key(category))
    }

    /// Returns whether no category is served.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }

    /// Returns the resources of the capability statement for the served
    /// categories: `MedicationRequest` when prescriptions are served,
    /// `MedicationDispense` when dispensations are, and neither otherwise.
    ///
    /// The interactions are those the face serves for each: the search by
    /// the patient's identifier and the read of `MedicationRequest`, after
    /// the HL7 Europe EU Health Data API `EEHRxF-ResourceAccessProvider`, the
    /// create and the read of `MedicationDispense`, and for both the vread
    /// of the version the member holds now (FHIR R4 `http.html` §3.1.0.3).
    #[must_use]
    pub fn resources(&self) -> Vec<CapabilityStatementRestResource> {
        let mut resources = Vec::new();
        if self.serves(Category::Prescription) {
            resources.push(prescriptions());
        }
        if self.serves(Category::Dispensation) {
            resources.push(dispensations());
        }
        resources
    }
}

/// The `MedicationRequest` resource of the capability statement.
fn prescriptions() -> CapabilityStatementRestResource {
    let interaction = |code: &str| CapabilityStatementRestResourceInteraction {
        code: code.into(),
        ..CapabilityStatementRestResourceInteraction::default()
    };
    CapabilityStatementRestResource {
        r#type: "MedicationRequest".into(),
        supported_profile: vec![MPD_MEDICATION_REQUEST.into()],
        documentation: Some(
            "A patient's prescriptions as the members that hold them state them, each passed through as the member states it; the members decide dispensability and the remaining dispensations.".into(),
        ),
        interaction: vec![
            interaction("search-type"),
            interaction("read"),
            interaction("vread"),
        ],
        search_param: vec![CapabilityStatementRestResourceSearchParam {
            name: "patient.identifier".into(),
            r#type: "token".into(),
            documentation: Some("The patient, as system|value; required, and one patient alone.".into()),
            ..CapabilityStatementRestResourceSearchParam::default()
        }],
        ..CapabilityStatementRestResource::default()
    }
}

/// The `MedicationDispense` resource of the capability statement.
fn dispensations() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: "MedicationDispense".into(),
        supported_profile: vec![MPD_MEDICATION_DISPENSE.into()],
        documentation: Some(
            "A dispensation is written to the one member that owns the prescription it names; the answer is 201 with a Location naming it by the face's id, which the read takes, and an OperationOutcome naming that member.".into(),
        ),
        interaction: ["create", "read", "vread"]
            .into_iter()
            .map(|code| CapabilityStatementRestResourceInteraction {
                code: code.into(),
                ..CapabilityStatementRestResourceInteraction::default()
            })
            .collect(),
        ..CapabilityStatementRestResource::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Category, Serving, ServingError, Source};

    fn source(category: &str, members: &[&str]) -> Source {
        Source {
            category: category.to_owned(),
            members: members.iter().map(|&member| member.to_owned()).collect(),
        }
    }

    #[test]
    fn no_declaration_serves_no_category_and_lists_no_resource() {
        let serving = Serving::compile(&[]).unwrap();
        assert!(serving.is_empty());
        assert_eq!(
            vec![Category::Prescription, Category::Dispensation],
            serving.unserved().collect::<Vec<_>>()
        );
        assert_eq!(
            serving.resources(),
            Vec::<fhir_types::r4::capability_statement::CapabilityStatementRestResource>::new()
        );
    }

    #[test]
    fn a_declared_category_lists_its_resource_and_members() {
        let serving = Serving::compile(&[
            source("Electronic-Prescriptions", &["node-b", "node-a"]),
            source("Electronic-Dispensations", &["node-a"]),
        ])
        .unwrap();
        let members: Vec<(Category, Vec<&str>)> = serving
            .members()
            .map(|(category, members)| (category, members.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            vec![
                (Category::Prescription, vec!["node-a", "node-b"]),
                (Category::Dispensation, vec!["node-a"]),
            ],
            members
        );
        let types: Vec<String> = serving
            .resources()
            .into_iter()
            .map(|resource| resource.r#type.value.unwrap_or_default())
            .collect();
        assert_eq!(vec!["MedicationRequest", "MedicationDispense"], types);
    }

    #[test]
    fn a_category_no_member_serves_resource_by_resource_is_refused() {
        let refused = Serving::compile(&[source("Patient-Summaries", &["node-a"])]);
        assert_eq!(
            Err(ServingError::Unservable {
                category: "Patient-Summaries".to_owned()
            }),
            refused
        );
    }

    #[test]
    fn a_category_declared_twice_is_refused() {
        let refused = Serving::compile(&[
            source("Electronic-Prescriptions", &["node-a"]),
            source("Electronic-Prescriptions", &["node-b"]),
        ]);
        assert_eq!(
            Err(ServingError::Twice {
                category: "Electronic-Prescriptions"
            }),
            refused
        );
    }

    #[test]
    fn a_category_with_no_member_or_an_empty_one_is_refused() {
        for members in [&[][..], &[""][..]] {
            let refused = Serving::compile(&[source("Electronic-Prescriptions", members)]);
            assert_eq!(
                Err(ServingError::NoMember {
                    category: "Electronic-Prescriptions"
                }),
                refused
            );
        }
    }

    #[test]
    fn a_member_named_twice_is_refused() {
        let refused =
            Serving::compile(&[source("Electronic-Prescriptions", &["node-a", "node-a"])]);
        assert_eq!(
            Err(ServingError::MemberTwice {
                category: "Electronic-Prescriptions",
                member: "node-a".to_owned()
            }),
            refused
        );
    }

    #[test]
    fn a_dispensation_member_that_serves_no_prescription_is_refused() {
        for sources in [
            vec![source("Electronic-Dispensations", &["node-a"])],
            vec![
                source("Electronic-Prescriptions", &["node-a"]),
                source("Electronic-Dispensations", &["node-b"]),
            ],
        ] {
            let refused = Serving::compile(&sources);
            assert!(
                matches!(
                    refused,
                    Err(ServingError::DispensingWithoutPrescribing { .. })
                ),
                "{refused:?}"
            );
        }
    }
}
