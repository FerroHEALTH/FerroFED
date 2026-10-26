// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored section queries of the patient summary.
//!
//! Each [`Section`] of the patient summary crosswalk that openEHR content
//! feeds has one stored query, held read-only under the reserved namespace
//! at [`VERSION`] (§12.7, N44), in the shape of [`section_query`]: every
//! whole composition that contains one of the section's archetypes, with
//! its uid and its template id.
//!
//! The archetypes of a section are the ones the openEHR International
//! Patient Summary template of the openEHR international Clinical Knowledge
//! Manager names in the template section, or sections, that carry it (CKM cid
//! `1013.26.376`, asset version 1).
//!
//! The crosswalk sections no query feeds are listed in [`UNSELECTED`], each
//! with the reason, for the clinical safety review.

use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use openehr_query::ast::SelectQuery;
use openehr_query::printer;

use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};

/// The stem of every section query's name, before the section.
pub const STEM: &str = "patient-summary-";

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

/// The medication order, in which a CDR commonly keeps the current
/// medication list (openEHR international CKM, cid 1013.1.5946, vendored
/// under `docs/specs/openehr-ckm-medication/`).
const MEDICATION_ORDER: Archetype =
    archetype!("INSTRUCTION", "openEHR-EHR-INSTRUCTION.medication_order.v3");

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

    /// Returns the archetypes this section selects by beyond those its
    /// template sections name, each a published CKM archetype.
    ///
    /// The medication summary also selects the medication order: a patient
    /// whose medicines a CDR records only as orders would otherwise get an
    /// empty medication summary, the harm of hazard I-03 of the clinical
    /// safety risk file, which the safety review of 2026-10-25 found
    /// (no specification governs the selection: our own design).
    #[must_use]
    pub const fn beyond_template(self) -> &'static [Archetype] {
        match self {
            Self::MedicationSummary => &[MEDICATION_ORDER],
            _ => &[],
        }
    }

    /// Returns the archetypes a composition of this section contains, at
    /// least one of them, in the template's order, then those of
    /// [`Section::beyond_template`].
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
            Self::MedicationSummary => &[MEDICATION, EXCLUSION, ABSENCE, MEDICATION_ORDER],
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
    ///
    /// # Panics
    ///
    /// Never: every section names at least one archetype.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "every arm of Section::archetypes names at least one archetype, which a test holds"
    )]
    pub fn query(self) -> SelectQuery {
        section_query::query(self.archetypes())
            .expect("a section should name at least one archetype")
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
}

/// Every section query as the registry holds it, in [`Section::ALL`] order.
///
/// # Errors
///
/// The [`QueryNameError`] of [`Section::name`].
pub fn definitions() -> Result<Vec<StoredDefinition>, QueryNameError> {
    Section::ALL.into_iter().map(Section::definition).collect()
}
