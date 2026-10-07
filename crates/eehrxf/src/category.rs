// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The priority categories of Art 14(1) this build carries.
//!
//! Each category is a Cargo feature, so a caller compiles only the categories
//! it exchanges. A category names the Xt-EHR logical model of its dataset and
//! the obligations profile that constrains it, by canonical URL, so the
//! [`DatasetModel`](crate::dataset::DatasetModel) read from the package is
//! the one place the dataset's elements live.

use crate::crosswalk::Crosswalk;

/// The canonical base of the Xt-EHR *EHDS Logical Information Models* 1.0.0.
///
/// The package manifest declares it as `canonical`, and every
/// `StructureDefinition` of the package carries its `url` under it.
pub const XTEHR_CANONICAL: &str = "http://www.xt-ehr.eu/fhir/models";

/// The canonical URL of HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`, the
/// code system [`Category::code`] names each category in
/// (`hl7.fhir.eu.health-data-api` 1.0.0-ballot).
pub const PRIORITY_SYSTEM: &str =
    "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";

/// One priority category of Art 14(1), as this build carries it.
///
/// The set grows with the enabled features, so a match over it needs a
/// wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Category {
    /// Art 14(1)(a), patient summaries (feature `patient-summary`).
    #[cfg(feature = "patient-summary")]
    PatientSummary,
    /// Art 14(1)(b), electronic prescriptions (feature `prescription`).
    #[cfg(feature = "prescription")]
    Prescription,
    /// Art 14(1)(c), electronic dispensations (feature `dispensation`).
    #[cfg(feature = "dispensation")]
    Dispensation,
    /// Art 14(1)(d), medical imaging studies and related imaging reports
    /// (feature `imaging`).
    #[cfg(feature = "imaging")]
    Imaging,
    /// Art 14(1)(e), medical test results, including laboratory and other
    /// diagnostic results and related reports (feature `laboratory`).
    #[cfg(feature = "laboratory")]
    Laboratory,
    /// Art 14(1)(f), discharge reports (feature `discharge`).
    #[cfg(feature = "discharge")]
    Discharge,
}

/// The logical model of one dataset and the obligations profile over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Root {
    /// The canonical URL of the logical model.
    pub model: &'static str,
    /// The canonical URL of the profile carrying its producer and consumer
    /// obligations.
    pub obligations: &'static str,
}

impl Category {
    /// Every category this build carries, in the order of Art 14(1).
    pub const ENABLED: &'static [Self] = &[
        #[cfg(feature = "patient-summary")]
        Self::PatientSummary,
        #[cfg(feature = "prescription")]
        Self::Prescription,
        #[cfg(feature = "dispensation")]
        Self::Dispensation,
        #[cfg(feature = "imaging")]
        Self::Imaging,
        #[cfg(feature = "laboratory")]
        Self::Laboratory,
        #[cfg(feature = "discharge")]
        Self::Discharge,
    ];

    /// Returns the code of the category in HL7 Europe's
    /// `EEHRxFDocumentPriorityCategoryCS`, such as `Patient-Summaries` for
    /// Art 14(1)(a).
    ///
    /// The code system is [`PRIORITY_SYSTEM`] of `hl7.fhir.eu.health-data-api`
    /// 1.0.0-ballot, and its codes are case-sensitive.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => "Patient-Summaries",
            #[cfg(feature = "prescription")]
            Self::Prescription => "Electronic-Prescriptions",
            #[cfg(feature = "dispensation")]
            Self::Dispensation => "Electronic-Dispensations",
            #[cfg(feature = "imaging")]
            Self::Imaging => "Medical-Imaging",
            #[cfg(feature = "laboratory")]
            Self::Laboratory => "Laboratory-Reports",
            #[cfg(feature = "discharge")]
            Self::Discharge => "Discharge-Reports",
        }
    }

    /// Returns the category this build carries that `code` names, a code
    /// [`Category::code`] gives, compared exactly.
    #[must_use]
    pub fn of_code(code: &str) -> Option<Self> {
        Self::ENABLED
            .iter()
            .copied()
            .find(|category| category.code() == code)
    }

    /// Returns the crosswalk of the category's dataset, when this build
    /// carries one; only the patient summary has one.
    #[must_use]
    pub const fn crosswalk(self) -> Option<&'static Crosswalk<'static>> {
        match self {
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => Some(&crate::crosswalk::patient_summary::PATIENT_SUMMARY),
            #[cfg_attr(
                not(any(
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "imaging",
                    feature = "laboratory",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "every category this build carries has a crosswalk"
                )
            )]
            _ => None,
        }
    }

    /// Returns the logical models of the category's dataset, each with its
    /// obligations profile.
    ///
    /// Imaging carries two, the imaging report and the imaging study, because
    /// Art 14(1)(d) names both and the Xt-EHR package models each on its own.
    #[must_use]
    pub const fn roots(self) -> &'static [Root] {
        match self {
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => &[Root {
                model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummary",
                obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummaryObligations",
            }],
            #[cfg(feature = "prescription")]
            Self::Prescription => &[Root {
                model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationPrescription",
                obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationPrescriptionObligations",
            }],
            #[cfg(feature = "dispensation")]
            Self::Dispensation => &[Root {
                model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationDispense",
                obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationDispenseObligations",
            }],
            #[cfg(feature = "imaging")]
            Self::Imaging => &[
                Root {
                    model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReport",
                    obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReportObligations",
                },
                Root {
                    model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudy",
                    obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudyObligations",
                },
            ],
            #[cfg(feature = "laboratory")]
            Self::Laboratory => &[Root {
                model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryReport",
                obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryReportObligations",
            }],
            #[cfg(feature = "discharge")]
            Self::Discharge => &[Root {
                model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSDischargeReport",
                obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSDischargeReportObligations",
            }],
        }
    }
}
