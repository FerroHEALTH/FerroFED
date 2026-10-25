// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report in the HL7 Europe Imaging Report 1.0.0-ballot form
//! (feature `imaging`).
//!
//! A `BundleReportEuImaging` whose `CompositionEuImaging` and
//! `DiagnosticReportEuImaging` each name the other. The composition's type is
//! the report's code, as the guide's examples write it, and its
//! `Organization` authors are the report's performing organizations, which
//! `dr-comp-author-org` requires; the operator is its custodian.

use crate::crosswalk::imaging::{IMAGING_COMPOSITION, IMAGING_DIAGNOSTIC_REPORT};
use crate::document::{Authorship, Code, Companion, EU_CORE_PATIENT, Form, Kind, LOINC, Slot};

/// The canonical URL of the HL7 Europe Imaging Report `Bundle` profile.
pub const IMAGING_BUNDLE: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging";

/// The code system of the EHDS priority categories of the HL7 Europe Health
/// Data API, which codes the imaging category.
pub const PRIORITY_CATEGORY: &str =
    "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";

/// The `Composition.type` of a report whose `DiagnosticReport.code` carries
/// none: LOINC `18726-0`, the concept the `ImagingReportTypesEuVSEuImaging`
/// value set includes with every descendant (`is-a`).
pub const IMAGING_REPORT: &str = "18726-0";

/// The two categories `CompositionEuImaging` and `DiagnosticReportEuImaging`
/// require: the imaging report (LOINC `85430-7`) and the `Medical-Imaging`
/// priority category.
pub const IMAGING_CATEGORIES: [Code; 2] = [
    Code {
        system: LOINC,
        code: "85430-7",
    },
    Code {
        system: PRIORITY_CATEGORY,
        code: "Medical-Imaging",
    },
];

/// Every section slice of `CompositionEuImaging`, in the profile's order;
/// the first four are required.
pub const IMAGING_SLOTS: [Slot; 10] = [
    Slot::new("imagingstudy", "18726-0", "Imaging study"),
    Slot::new("order", "55115-0", "Order"),
    Slot::new("history", "11329-0", "History"),
    Slot::new("procedure", "55111-9", "Procedure"),
    Slot::new("comparison", "18834-2", "Comparison"),
    Slot::new("findings", "59776-5", "Findings"),
    Slot::new("impression", "19005-8", "Impressions"),
    Slot::new("recommendation", "18783-1", "Recommendations"),
    Slot::new("communication", "73568-8", "Communications"),
    Slot::new("report", "LP173421-1", "Report"),
];

/// The imaging report in the HL7 Europe Imaging Report 1.0.0-ballot form.
pub const IMAGING_REPORT_FORM: Form = Form {
    bundle: IMAGING_BUNDLE,
    composition: IMAGING_COMPOSITION,
    patient: EU_CORE_PATIENT,
    kind: Kind::Report {
        fallback: Code {
            system: LOINC,
            code: IMAGING_REPORT,
        },
    },
    categories: &IMAGING_CATEGORIES,
    authorship: Authorship::ReportPerformers,
    slots: &IMAGING_SLOTS,
    report: Some(Companion {
        profile: IMAGING_DIAGNOSTIC_REPORT,
        link: "http://hl7.eu/fhir/extensions/StructureDefinition/composition-diagnosticReportReference",
        back: "http://hl7.org/fhir/5.0/StructureDefinition/extension-DiagnosticReport.composition",
    }),
    encounter: None,
    language: false,
};
