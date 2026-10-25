// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report in the HL7 Europe Laboratory Report 2.0.0 form
//! (feature `laboratory`): a `Bundle-eu-lab` whose `Composition-eu-lab`
//! and `DiagnosticReport-eu-lab` each name the other.

use crate::crosswalk::laboratory::{LAB_COMPOSITION, LAB_DIAGNOSTIC_REPORT};
use crate::document::{Authorship, Code, Companion, EU_CORE_PATIENT, Form, Kind, LOINC, Slot};

/// The canonical URL of the HL7 Europe Laboratory Report `Bundle` profile.
pub const LAB_BUNDLE: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition/Bundle-eu-lab";

/// The LOINC code of the one concept the `lab-reportType-eu-lab` value set
/// holds, a laboratory report.
pub const LABORATORY_REPORT: &str = "11502-2";

/// The sections of a laboratory report: one per study type of the
/// `lab-studyType-eu-lab` value set, then the profile's `annotations` and
/// `attachment` slices.
///
/// `Composition-eu-lab` 2.0.0 slices only the two last; the results go in
/// sections coded by the study type they group (no specification governs
/// which codes: our own design, the value set the profile binds the report's
/// study type to).
pub const LAB_SLOTS: [Slot; 9] = [
    Slot::new("bloodBank", "18717-9", "Blood bank studies"),
    Slot::new("chemistry", "18719-5", "Chemistry studies"),
    Slot::new("fertility", "18722-9", "Fertility studies"),
    Slot::new("hematology", "18723-7", "Hematology studies"),
    Slot::new("microbiology", "18725-2", "Microbiology studies"),
    Slot::new("toxicology", "18728-6", "Toxicology studies"),
    Slot::new("laboratory", "26436-6", "Laboratory studies"),
    Slot::new("annotations", "48767-8", "Annotation comment"),
    Slot::new("attachment", "77599-9", "Additional documents"),
];

/// The laboratory report in the HL7 Europe Laboratory Report 2.0.0 form.
pub const LABORATORY_REPORT_FORM: Form = Form {
    bundle: LAB_BUNDLE,
    composition: LAB_COMPOSITION,
    patient: EU_CORE_PATIENT,
    kind: Kind::Fixed(Code {
        system: LOINC,
        code: LABORATORY_REPORT,
    }),
    categories: &[],
    authorship: Authorship::Gateway,
    slots: &LAB_SLOTS,
    report: Some(Companion {
        profile: LAB_DIAGNOSTIC_REPORT,
        link: "http://hl7.eu/fhir/extensions/StructureDefinition/composition-diagnosticReportReference",
        back: "http://hl7.org/fhir/5.0/StructureDefinition/extension-DiagnosticReport.composition",
    }),
    encounter: None,
    language: false,
};
