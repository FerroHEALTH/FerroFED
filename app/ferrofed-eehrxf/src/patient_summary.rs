// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored section queries of the patient summary.
//!
//! Each [`Section`] of the patient summary crosswalk that openEHR content
//! feeds has one stored query, held read-only under the reserved namespace
//! at [`VERSION`] (§12.7, N44). It selects, once
//! per member, every whole composition that contains one of the section's
//! archetypes, with its uid and its template id, because a FHIRconnect
//! mapping is chosen per template:
//!
//! ```text
//! SELECT DISTINCT c AS composition, c/uid/value AS uid,
//!   c/archetype_details/template_id/value AS template_id
//! FROM EHR e CONTAINS COMPOSITION c
//!   CONTAINS (EVALUATION a0[openEHR-EHR-EVALUATION.adverse_reaction_risk.v1] OR …)
//! WHERE e/ehr_status/subject/external_ref/id/value = $patient
//!   AND e/ehr_status/subject/external_ref/namespace = $namespace
//! ```
//!
//! The archetypes of a section are the ones the openEHR International
//! Patient Summary template of the openEHR international Clinical Knowledge
//! Manager names in the template section, or sections, that carry it (CKM cid
//! `1013.26.376`, asset version 1). `DISTINCT` returns a composition once
//! however many of its entries match (AQL §DISTINCT). The patient is a
//! parameter, so the text holds no identifier, and the rewrite replaces the
//! two predicates with each member's own `ehr_id` (§7.1, §5.4.1, N33).
//!
//! The crosswalk sections no query feeds are listed in [`UNSELECTED`], each
//! with the reason, for the clinical safety review.

use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use openehr_query::ast::{
    ArchetypePredicate, ClassExprOperand, ColumnExpr, CompareOperand, ContainsConstraint,
    ContainsExpr, IdentifiedExpr, IdentifiedPath, ObjectPath, PathPart, PathPredicate,
    SelectClause, SelectExpr, SelectQuery, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;
use openehr_query::printer;

use crate::reserved::{NAMESPACE, SAVED, VERSION};

/// The parameter that carries the patient identifier, as its
/// `query_parameters` key names it.
pub const PATIENT: &str = "patient";

/// The parameter that carries the namespace that issued the identifier
/// (§5.2), as its `query_parameters` key names it.
pub const ISSUER: &str = "namespace";

/// The stem of every section query's name, before the section.
pub const STEM: &str = "patient-summary-";

/// One archetype a section selects compositions by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Archetype {
    /// The RM class of the archetype's root, the class the `CONTAINS`
    /// expression names.
    pub class: &'static str,
    /// The archetype id.
    pub id: &'static str,
}

/// A patient summary section with a stored query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Section {
    /// Allergies and intolerances (eHN A.2.1.1).
    AllergiesAndIntolerances,
    /// Medical problems (eHN A.2.3.1).
    Problems,
    /// Medication summary (eHN A.2.4).
    MedicationSummary,
    /// Medical devices and implants (eHN A.2.3.2).
    MedicalDevicesAndImplants,
    /// Procedures (eHN A.2.3.3).
    Procedures,
    /// Immunisations (eHN A.2.2.1).
    Immunisations,
    /// Social history (eHN A.2.5).
    SocialHistory,
    /// Pregnancy history (eHN A.2.6).
    PregnancyHistory,
    /// Advance directives (eHN A.2.7.2).
    AdvanceDirectives,
    /// Observation results (eHN A.2.8).
    ObservationResults,
    /// Care plans (eHN A.2.9).
    CarePlans,
}

/// Why a crosswalk section has no stored query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Reason {
    /// The element is a gap the crosswalk lists for the clinical safety
    /// review: no openEHR content is named to feed it.
    Gap {
        /// The eHealth Network element id of the gap.
        ehn: &'static str,
    },
    /// The International Patient Summary template has no section for it.
    NotInTemplate,
    /// The section carries a note alone and no entry, so no composition
    /// feeds it.
    NoEntries,
}

/// A crosswalk section with no stored query, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unselected {
    /// The Xt-EHR element path of the section.
    pub element: &'static str,
    /// Why no query feeds it.
    pub reason: Reason,
}

/// The crosswalk sections no stored query feeds.
pub const UNSELECTED: &[Unselected] = &[
    Unselected {
        element: "EHDSPatientSummary.alerts",
        reason: Reason::Gap { ehn: "A.2.1.2" },
    },
    // NOTE: the template's Functional Status section names only the generic
    // problem_diagnosis.v1 and clinical_synopsis.v1, which do not tell a
    // functional status from any other problem or impression.
    Unselected {
        element: "EHDSPatientSummary.functionalStatus",
        reason: Reason::Gap { ehn: "A.2.3.4" },
    },
    Unselected {
        element: "EHDSPatientSummary.travelHistory",
        reason: Reason::NotInTemplate,
    },
    Unselected {
        element: "EHDSPatientSummary.patientStory",
        reason: Reason::NoEntries,
    },
];

/// Builds one archetype of the class `$class`.
macro_rules! archetype {
    ($class:literal, $id:literal) => {
        Archetype {
            class: $class,
            id: $id,
        }
    };
}

/// The global exclusion statement, which the template offers in several
/// sections.
const EXCLUSION: Archetype = archetype!("EVALUATION", "openEHR-EHR-EVALUATION.exclusion_global.v1");

/// The statement that information is absent, which the template offers in
/// several sections.
const ABSENCE: Archetype = archetype!("EVALUATION", "openEHR-EHR-EVALUATION.absence.v2");

/// The medication management action, which the template uses for both the
/// medication statement and the immunisation statement.
const MEDICATION: Archetype = archetype!("ACTION", "openEHR-EHR-ACTION.medication.v1");

/// The problem or diagnosis evaluation.
const PROBLEM: Archetype = archetype!("EVALUATION", "openEHR-EHR-EVALUATION.problem_diagnosis.v1");

impl Section {
    /// Every section with a stored query, in the crosswalk's order.
    pub const ALL: [Self; 11] = [
        Self::AllergiesAndIntolerances,
        Self::Problems,
        Self::MedicationSummary,
        Self::MedicalDevicesAndImplants,
        Self::Procedures,
        Self::Immunisations,
        Self::SocialHistory,
        Self::PregnancyHistory,
        Self::AdvanceDirectives,
        Self::ObservationResults,
        Self::CarePlans,
    ];

    /// Returns the Xt-EHR element path of the section, the crosswalk row it
    /// feeds.
    #[must_use]
    pub const fn element(self) -> &'static str {
        match self {
            Self::AllergiesAndIntolerances => "EHDSPatientSummary.allergiesAndIntolerances",
            Self::Problems => "EHDSPatientSummary.problems",
            Self::MedicationSummary => "EHDSPatientSummary.medicationSummary",
            Self::MedicalDevicesAndImplants => "EHDSPatientSummary.medicalDevicesAndImplants",
            Self::Procedures => "EHDSPatientSummary.procedures",
            Self::Immunisations => "EHDSPatientSummary.immunisations",
            Self::SocialHistory => "EHDSPatientSummary.socialHistory",
            Self::PregnancyHistory => "EHDSPatientSummary.pregnancyHistory",
            Self::AdvanceDirectives => "EHDSPatientSummary.advanceDirectives",
            Self::ObservationResults => "EHDSPatientSummary.observationResults",
            Self::CarePlans => "EHDSPatientSummary.carePlans",
        }
    }

    /// Returns the section's part of the query name, after [`STEM`].
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AllergiesAndIntolerances => "allergies-and-intolerances",
            Self::Problems => "problems",
            Self::MedicationSummary => "medication-summary",
            Self::MedicalDevicesAndImplants => "medical-devices-and-implants",
            Self::Procedures => "procedures",
            Self::Immunisations => "immunisations",
            Self::SocialHistory => "social-history",
            Self::PregnancyHistory => "pregnancy-history",
            Self::AdvanceDirectives => "advance-directives",
            Self::ObservationResults => "observation-results",
            Self::CarePlans => "care-plans",
        }
    }

    /// Returns the section whose [`Section::slug`] is `slug`, or `None` for
    /// any other text.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.slug() == slug)
    }

    /// Returns the names of the International Patient Summary template's
    /// sections whose archetypes this section selects by.
    ///
    /// The Xt-EHR observation results are "measurements, laboratory results,
    /// anatomic pathology results, radiology results or other imaging or
    /// clinical results" (`EHDSPatientSummary.observationResults`), so they
    /// take the template's Diagnostic Results and Vital Signs; the problems
    /// take its Problem List and Past History of Illnesses.
    #[must_use]
    pub const fn template_sections(self) -> &'static [&'static str] {
        match self {
            Self::AllergiesAndIntolerances => &["Allergies & Intolerances"],
            Self::Problems => &["Problem List", "Past History of Illnesses"],
            Self::MedicationSummary => &["Medication Summary"],
            Self::MedicalDevicesAndImplants => &["Medical Devices"],
            Self::Procedures => &["History of Procedures"],
            Self::Immunisations => &["Immunizations"],
            Self::SocialHistory => &["Social History"],
            Self::PregnancyHistory => &["Pregnancy"],
            Self::AdvanceDirectives => &["Advanced Directives"],
            Self::ObservationResults => &["Diagnostic Results", "Vital Signs"],
            Self::CarePlans => &["Plan of Care"],
        }
    }

    /// Returns the archetypes a composition of this section contains, at
    /// least one of them, in the template's order.
    #[must_use]
    pub const fn archetypes(self) -> &'static [Archetype] {
        match self {
            Self::AllergiesAndIntolerances => &[
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.adverse_reaction_risk.v1"
                ),
                EXCLUSION,
                ABSENCE,
            ],
            Self::Problems => &[PROBLEM, EXCLUSION, ABSENCE],
            Self::MedicationSummary => &[MEDICATION, EXCLUSION, ABSENCE],
            Self::MedicalDevicesAndImplants => &[archetype!(
                "EVALUATION",
                "openEHR-EHR-EVALUATION.device_summary.v0"
            )],
            Self::Procedures => &[
                archetype!("ACTION", "openEHR-EHR-ACTION.procedure.v1"),
                ABSENCE,
                EXCLUSION,
            ],
            Self::Immunisations => &[MEDICATION, ABSENCE],
            Self::SocialHistory => &[
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.tobacco_smoking_summary.v1"
                ),
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.alcohol_consumption_summary.v1"
                ),
            ],
            Self::PregnancyHistory => &[
                archetype!("EVALUATION", "openEHR-EHR-EVALUATION.pregnancy_summary.v0"),
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.estimated_date_delivery.v0"
                ),
                archetype!(
                    "OBSERVATION",
                    "openEHR-EHR-OBSERVATION.exclusion_pregnancy.v0"
                ),
            ],
            Self::AdvanceDirectives => &[
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.advance_care_directive.v1"
                ),
                archetype!(
                    "EVALUATION",
                    "openEHR-EHR-EVALUATION.limitation_of_treatment.v0"
                ),
            ],
            Self::ObservationResults => &[
                archetype!(
                    "OBSERVATION",
                    "openEHR-EHR-OBSERVATION.laboratory_test_result.v1"
                ),
                archetype!(
                    "OBSERVATION",
                    "openEHR-EHR-OBSERVATION.imaging_exam_result.v0"
                ),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.body_weight.v2"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.height.v2"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.respiration.v2"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.pulse.v2"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.body_temperature.v2"),
                archetype!(
                    "OBSERVATION",
                    "openEHR-EHR-OBSERVATION.head_circumference.v1"
                ),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.pulse_oximetry.v1"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.body_mass_index.v2"),
                archetype!("OBSERVATION", "openEHR-EHR-OBSERVATION.blood_pressure.v2"),
            ],
            Self::CarePlans => &[
                archetype!("ACTION", "openEHR-EHR-ACTION.care_plan.v0"),
                archetype!("INSTRUCTION", "openEHR-EHR-INSTRUCTION.service_request.v1"),
            ],
        }
    }

    /// Returns the section query's qualified name,
    /// `eu.ferrofed.eehrxf::patient-summary-{slug}`.
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of [`QueryName::new`], which no section's name
    /// meets.
    pub fn name(self) -> Result<QueryName, QueryNameError> {
        QueryName::new(&format!("{NAMESPACE}::{STEM}{}", self.slug()))
    }

    /// Returns the section query as an AQL syntax tree.
    #[must_use]
    pub fn query(self) -> SelectQuery {
        let column = |path: IdentifiedPath, alias: &str| SelectExpr {
            column: ColumnExpr::Path(path),
            alias: Some(alias.to_owned()),
        };
        SelectQuery {
            select: SelectClause {
                distinct: true,
                top: None,
                columns: vec![
                    column(identified("c", &[]), "composition"),
                    column(identified("c", &["uid", "value"]), "uid"),
                    column(
                        identified("c", &["archetype_details", "template_id", "value"]),
                        "template_id",
                    ),
                ],
            },
            from: contained(
                class("EHR", "e", None),
                Some(contained(
                    class("COMPOSITION", "c", None),
                    Some(self.entries()),
                )),
            ),
            where_: Some(WhereExpr::And(
                Box::new(subject(&["id", "value"], PATIENT)),
                Box::new(subject(&["namespace"], ISSUER)),
            )),
            order_by: Vec::new(),
            limit: None,
        }
    }

    /// Returns the section query as AQL text, printed from its syntax tree.
    #[must_use]
    pub fn aql(self) -> String {
        printer::to_aql(&self.query())
    }

    /// Returns the section query as the registry holds it: under its
    /// reserved name, at [`VERSION`], saved at [`SAVED`].
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of [`Section::name`].
    pub fn definition(self) -> Result<StoredDefinition, QueryNameError> {
        Ok(StoredDefinition::new(
            self.name()?,
            VERSION,
            self.aql(),
            SAVED,
        ))
    }

    /// The `CONTAINS` expression under the composition: one class expression
    /// per archetype, joined by `OR`.
    #[expect(
        clippy::expect_used,
        reason = "every arm of Section::archetypes names at least one archetype, which a test holds"
    )]
    fn entries(self) -> ContainsExpr {
        let (first, rest) = self
            .archetypes()
            .split_first()
            .expect("a section should name at least one archetype");
        rest.iter()
            .zip(1_usize..)
            .fold(leaf(first, 0), |joined, (archetype, index)| {
                ContainsExpr::Or(Box::new(joined), Box::new(leaf(archetype, index)))
            })
    }
}

/// The class expression of `archetype`, bound to the variable `a{index}`.
fn leaf(archetype: &Archetype, index: usize) -> ContainsExpr {
    let predicate = PathPredicate::Archetype(ArchetypePredicate::Hrid(archetype.id.to_owned()));
    contained(
        class(archetype.class, &format!("a{index}"), Some(predicate)),
        None,
    )
}

/// Every section query as the registry holds it, in [`Section::ALL`] order.
///
/// # Errors
///
/// The [`QueryNameError`] of [`Section::name`].
pub fn definitions() -> Result<Vec<StoredDefinition>, QueryNameError> {
    Section::ALL.into_iter().map(Section::definition).collect()
}

/// The path `root/part/…`, with no predicate.
fn identified(root: &str, parts: &[&str]) -> IdentifiedPath {
    let path = (!parts.is_empty()).then(|| ObjectPath {
        parts: parts
            .iter()
            .map(|name| PathPart {
                name: (*name).to_owned(),
                predicate: None,
            })
            .collect(),
    });
    IdentifiedPath::new(root.to_owned(), None, path)
}

/// The class expression `rm_type variable[predicate]`.
fn class(rm_type: &str, variable: &str, predicate: Option<PathPredicate>) -> ClassExprOperand {
    ClassExprOperand::Class {
        rm_type: rm_type.to_owned(),
        variable: Some(variable.to_owned()),
        predicate,
    }
}

/// `operand`, containing `inner` when given.
fn contained(operand: ClassExprOperand, inner: Option<ContainsExpr>) -> ContainsExpr {
    ContainsExpr::Contained {
        operand,
        contains: inner.map(|expr| {
            Box::new(ContainsConstraint {
                negated: false,
                expr,
            })
        }),
    }
}

/// `e/ehr_status/subject/external_ref/{tail} = ${parameter}`, the AST
/// holding a parameter with its `$` as the lexer reads it.
fn subject(tail: &[&str], parameter: &str) -> WhereExpr {
    let mut parts = vec!["ehr_status", "subject", "external_ref"];
    parts.extend_from_slice(tail);
    WhereExpr::identified(IdentifiedExpr::Compare {
        lhs: CompareOperand::Path(identified("e", &parts)),
        op: CompOp::Eq,
        rhs: Terminal::Parameter(format!("${parameter}")),
    })
}
