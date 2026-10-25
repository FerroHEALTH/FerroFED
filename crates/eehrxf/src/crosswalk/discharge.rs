// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report crosswalk (feature `discharge`).
//!
//! The rows key on the Xt-EHR `EHDSDischargeReport` 1.0.0 element paths,
//! name the eHealth Network ids of the *Guideline on Hospital Discharge
//! Report* Release 1.1, §4.1 to §4.3, and the elements of the HL7 Europe
//! Hospital Discharge Report `composition-eu-hdr` profile, 1.0.0. Every
//! section of the model has a row, with its narrative, its note and its
//! entries, and so has every header element a producer is able to populate.
//! The encounter is the composition's `encounter`, which the profile
//! requires, rather than a section. The header, the section narratives and
//! notes are written by the document assembly, so no FHIRconnect context
//! feeds them; each entry list admits the profiles its HDR section entry
//! targets, and a section whose entries target any resource admits the
//! profiles of the model element it carries.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe Hospital Discharge Report composition
/// profile.
pub const HDR_COMPOSITION: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/composition-eu-hdr";

/// The discharge report crosswalk.
pub static DISCHARGE: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSDischargeReport",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSDischargeReportObligations",
    },
    profile: HDR_COMPOSITION,
    companions: &[],
    sliced: "Composition.section",
    ehn: EHN,
    rows: ROWS,
    gaps: &[],
};

/// The eHealth Network discharge report dataset elements the rows name, from
/// the guideline's §4.1 (header) and §4.3 (the complete body dataset).
const EHN: &[EhnElement] = &[
    ehn("A.1", "Hospital Discharge Report header data element"),
    ehn("A.1.1", "Identification of the patient/subject"),
    ehn("A.1.2", "Patient/subject related contact information"),
    ehn("A.1.5", "Author"),
    ehn("A.1.6", "Attester"),
    ehn("A.1.6.5", "Approval date and time"),
    ehn("A.1.7", "Legal authenticator"),
    ehn("A.1.7.5", "Authentication date and time"),
    ehn("A.1.8.1", "Document ID"),
    ehn("A.1.8.2", "Document type"),
    ehn("A.1.8.3", "Document status"),
    ehn("A.1.8.4", "Report date and time"),
    ehn("A.1.8.5", "Document title"),
    ehn("A.1.8.6", "Report custodian"),
    ehn("A.1.8.8", "Language"),
    ehn("A.2.0", "Hospital Discharge Report in its narrative form"),
    ehn("A.2.2", "Alerts"),
    ehn(
        "A.2.2.2",
        "Medical alerts (relevant for the respective hospital stay)",
    ),
    ehn("A.2.3", "Encounter"),
    ehn("A.2.3.2", "Encounter note"),
    ehn("A.2.4", "Admission evaluation"),
    ehn("A.2.4.1", "Objective findings"),
    ehn("A.2.4.2", "Functional status"),
    ehn("A.2.5", "Patient history"),
    ehn("A.2.5.1.1", "History of past illness"),
    ehn("A.2.5.1.2", "Devices and Implants"),
    ehn("A.2.5.1.3", "History of procedures"),
    ehn("A.2.6", "Course of hospitalisation (Hospital stay)"),
    ehn("A.2.6.1", "Diagnostic summary"),
    ehn("A.2.6.2", "Significant procedures"),
    ehn("A.2.6.3", "Medical devices and implants"),
    ehn("A.2.6.5", "Pharmacotherapy"),
    ehn("A.2.6.6", "Significant results"),
    ehn("A.2.6.7", "Synthesis"),
    ehn("A.2.7", "Discharge details"),
    ehn("A.2.7.1", "Objective findings"),
    ehn("A.2.7.2", "Functional status"),
    ehn("A.2.7.3", "Discharge note"),
    ehn(
        "A.2.8",
        "Care plan and other recommendations after discharge",
    ),
    ehn("A.2.8.1", "Care plan"),
    ehn("A.2.8.2", "Medication summary"),
    ehn("A.2.8.3", "Other recommendations"),
];

/// HL7 Europe Base `condition-eu-core`.
const CONDITION: &str = "http://hl7.eu/fhir/base/StructureDefinition/condition-eu-core";
/// HL7 Europe Base `flag-patient-eu-core`.
const FLAG: &str = "http://hl7.eu/fhir/base/StructureDefinition/flag-patient-eu-core";
/// HL7 Europe Base `procedure-eu-core`.
const PROCEDURE: &str = "http://hl7.eu/fhir/base/StructureDefinition/procedure-eu-core";
/// HL7 Europe Base `medicationStatement-eu-core`.
const MEDICATION_STATEMENT: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/medicationStatement-eu-core";
/// HL7 Europe Base `medicationRequest-eu-core`.
const MEDICATION_REQUEST: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/medicationRequest-eu-core";
/// HL7 Europe Hospital Discharge Report `encounter-eu-hdr`.
const ENCOUNTER: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/encounter-eu-hdr";
/// HL7 Europe Hospital Discharge Report `deviceUseStatement-eu-hdr`.
const DEVICE_USE: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/deviceUseStatement-eu-hdr";
/// HL7 Europe Hospital Discharge Report `medicationDispense-eu-hdr`.
const MEDICATION_DISPENSE: &str =
    "http://hl7.eu/fhir/hdr/StructureDefinition/medicationDispense-eu-hdr";
/// HL7 Europe Hospital Discharge Report `medicationAdministration-eu-hdr`.
const MEDICATION_ADMINISTRATION: &str =
    "http://hl7.eu/fhir/hdr/StructureDefinition/medicationAdministration-eu-hdr";
/// HL7 Europe Hospital Discharge Report `carePlan-eu-hdr`.
const CARE_PLAN: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/carePlan-eu-hdr";
/// FHIR R4 `Observation`.
const OBSERVATION: &str = "http://hl7.org/fhir/StructureDefinition/Observation";

/// No eHealth Network counterpart.
const NONE: &[&str] = &[];
/// No FHIRconnect context: the document assembly writes the element.
const ASSEMBLED: &[&str] = &[];
/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// Builds the row of the section `$name`, carried by the HDR section slice
/// `$slice`.
macro_rules! section {
    ($name:literal, $ehn:expr, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSDischargeReport.body.", $name),
            $ehn,
            $producer,
            concat!("Composition.section:", $slice),
            ASSEMBLED,
        )
    };
}

/// Builds the row of the generated narrative of the section `$name`, the
/// section text of its HDR slice.
macro_rules! narrative {
    ($name:literal, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSDischargeReport.body.", $name, ".generatedNarrative"),
            NONE,
            $producer,
            concat!("Composition.section:", $slice, ".text"),
            ASSEMBLED,
        )
    };
}

/// Builds the row of the note of the section `$name`, the section note
/// extension of its HDR slice.
macro_rules! note {
    ($name:literal, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSDischargeReport.body.", $name, ".note"),
            NONE,
            $producer,
            concat!("Composition.section:", $slice, ".extension:section-note"),
            ASSEMBLED,
        )
    };
}

/// The rows: the header elements a producer is able to populate, then every
/// section in model order with its children.
const ROWS: &[Row] = &[
    row(
        "EHDSDischargeReport.header",
        &["A.1"],
        SHALL,
        "Composition",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.subject",
        &["A.1.1", "A.1.2"],
        SHALL,
        "Composition.subject",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.identifier",
        &["A.1.8.1"],
        SHALL,
        "Composition.identifier",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.author[x]",
        &["A.1.5"],
        SHALL,
        "Composition.author",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.date",
        &["A.1.8.4"],
        SHALL,
        "Composition.date",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.status",
        &["A.1.8.3"],
        SHALL,
        "Composition.status",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.language",
        &["A.1.8.8"],
        SHALL,
        "Composition.language",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.documentType",
        &["A.1.8.2"],
        SHALL,
        "Composition.type",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.documentTitle",
        &["A.1.8.5"],
        SHALL,
        "Composition.title",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.period",
        NONE,
        SHALL,
        "Composition.event.period",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.attestation",
        &["A.1.6"],
        SHALL,
        "Composition.attester",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.attestation.attester[x]",
        &["A.1.6"],
        SHALL,
        "Composition.attester.party",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.attestation.datetime",
        &["A.1.6.5"],
        SHALL,
        "Composition.attester.time",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.legalAuthentication",
        &["A.1.7"],
        SHALL,
        "Composition.attester",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.legalAuthentication.legalAuthenticator[x]",
        &["A.1.7"],
        SHALL,
        "Composition.attester.party",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.legalAuthentication.datetime",
        &["A.1.7.5"],
        SHALL,
        "Composition.attester.time",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.eventType",
        NONE,
        SHALL,
        "Composition.event.code",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.serviceSpecialty",
        NONE,
        SHALL,
        "Composition.category",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.header.custodian",
        &["A.1.8.6"],
        SHALL,
        "Composition.custodian",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.presentedForm",
        &["A.2.0"],
        SHALL,
        "Composition.section:sectionAttachments.entry",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.body",
        NONE,
        SHALL,
        "Composition.section",
        ASSEMBLED,
    ),
    // Alerts.
    section!("alerts", &["A.2.2"], SHOULD, "sectionAlert"),
    narrative!("alerts", SHOULD, "sectionAlert"),
    row(
        "EHDSDischargeReport.body.alerts.medicalAlert",
        &["A.2.2.2"],
        SHOULD,
        "Composition.section:sectionAlert.entry:flag",
        &[FLAG],
    ),
    // Encounter information, the composition's encounter.
    row(
        "EHDSDischargeReport.body.encounterInformation",
        &["A.2.3"],
        SHALL,
        "Composition.encounter",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.body.encounterInformation.generatedNarrative",
        NONE,
        SHALL,
        "Composition.encounter",
        ASSEMBLED,
    ),
    row(
        "EHDSDischargeReport.body.encounterInformation.encounter",
        &["A.2.3"],
        SHALL,
        "Composition.encounter",
        &[ENCOUNTER],
    ),
    row(
        "EHDSDischargeReport.body.encounterInformation.note",
        &["A.2.3.2"],
        SHOULD,
        "Composition.encounter",
        ASSEMBLED,
    ),
    // Admission evaluation.
    section!(
        "admissionEvaluation",
        &["A.2.4"],
        SHALL,
        "sectionAdmissionEvaluation",
    ),
    narrative!("admissionEvaluation", SHOULD, "sectionAdmissionEvaluation"),
    row(
        "EHDSDischargeReport.body.admissionEvaluation.objectiveFindings",
        &["A.2.4.1"],
        SHOULD,
        "Composition.section:sectionAdmissionEvaluation.entry",
        &[OBSERVATION],
    ),
    row(
        "EHDSDischargeReport.body.admissionEvaluation.functionalStatus[x]",
        &["A.2.4.2"],
        SHOULD,
        "Composition.section:sectionAdmissionEvaluation.entry",
        &[CONDITION, OBSERVATION],
    ),
    note!("admissionEvaluation", SHOULD, "sectionAdmissionEvaluation"),
    // Patient history.
    section!("patientHistory", &["A.2.5"], SHOULD, "sectionPatientHx"),
    narrative!("patientHistory", SHOULD, "sectionPatientHx"),
    row(
        "EHDSDischargeReport.body.patientHistory.pastProblems",
        &["A.2.5.1.1"],
        SHOULD,
        "Composition.section:sectionPatientHx.entry",
        &[CONDITION],
    ),
    row(
        "EHDSDischargeReport.body.patientHistory.devicesAndImplants",
        &["A.2.5.1.2"],
        SHOULD,
        "Composition.section:sectionMedicalDevices.entry:deviceStatement",
        &[DEVICE_USE],
    ),
    row(
        "EHDSDischargeReport.body.patientHistory.procedures",
        &["A.2.5.1.3"],
        SHOULD,
        "Composition.section:sectionProceduresHx.entry:procedure",
        &[PROCEDURE],
    ),
    note!("patientHistory", SHOULD, "sectionPatientHx"),
    // Course of the encounter.
    section!(
        "courseOfEncounter",
        &["A.2.6"],
        SHALL,
        "sectionHospitalCourse",
    ),
    narrative!("courseOfEncounter", SHOULD, "sectionHospitalCourse"),
    row(
        "EHDSDischargeReport.body.courseOfEncounter.diagnosticSummary",
        &["A.2.6.1"],
        SHOULD,
        "Composition.section:sectionDiagnosticSummary.entry",
        &[CONDITION],
    ),
    row(
        "EHDSDischargeReport.body.courseOfEncounter.procedures",
        &["A.2.6.2"],
        SHOULD,
        "Composition.section:sectionSignificantProcedures.entry",
        &[PROCEDURE],
    ),
    row(
        "EHDSDischargeReport.body.courseOfEncounter.medicalDevicesAndImplants",
        &["A.2.6.3"],
        SHOULD,
        "Composition.section:sectionImplantedDevices.entry",
        &[DEVICE_USE, PROCEDURE],
    ),
    row(
        "EHDSDischargeReport.body.courseOfEncounter.pharmacotherapy",
        &["A.2.6.5"],
        SHOULD,
        "Composition.section:sectionPharmacotherapy.entry",
        &[
            MEDICATION_STATEMENT,
            MEDICATION_REQUEST,
            MEDICATION_DISPENSE,
            MEDICATION_ADMINISTRATION,
        ],
    ),
    row(
        "EHDSDischargeReport.body.courseOfEncounter.testResults[x]",
        &["A.2.6.6"],
        SHOULD,
        "Composition.section:sectionSignificantResults.entry",
        &[OBSERVATION],
    ),
    note!("courseOfEncounter", SHOULD, "sectionHospitalCourse"),
    // Discharge details.
    section!(
        "dischargeDetails",
        &["A.2.7"],
        SHOULD,
        "sectionDischargeDetails",
    ),
    narrative!("dischargeDetails", SHOULD, "sectionDischargeDetails"),
    row(
        "EHDSDischargeReport.body.dischargeDetails.objectiveFindings",
        &["A.2.7.1"],
        SHOULD,
        "Composition.section:sectionDischargeDetails.entry",
        &[OBSERVATION],
    ),
    row(
        "EHDSDischargeReport.body.dischargeDetails.functionalStatus[x]",
        &["A.2.7.2"],
        SHOULD,
        "Composition.section:sectionFunctionalStatus.entry",
        &[CONDITION, OBSERVATION],
    ),
    row(
        "EHDSDischargeReport.body.dischargeDetails.note",
        &["A.2.7.3"],
        SHOULD,
        "Composition.section:sectionDischargeDetails.extension:section-note",
        ASSEMBLED,
    ),
    // Medication summary.
    section!(
        "medicationSummary",
        &["A.2.8.2"],
        SHALL,
        "sectionDischargeMedications",
    ),
    narrative!("medicationSummary", SHOULD, "sectionDischargeMedications"),
    row(
        "EHDSDischargeReport.body.medicationSummary.medicationUse",
        &["A.2.8.2"],
        SHALL,
        "Composition.section:sectionDischargeMedications.entry",
        &[
            MEDICATION_REQUEST,
            MEDICATION_DISPENSE,
            MEDICATION_STATEMENT,
        ],
    ),
    note!("medicationSummary", SHOULD, "sectionDischargeMedications"),
    // Care plan and other recommendations.
    section!("carePlan", &["A.2.8"], SHOULD, "sectionPlanOfCare"),
    narrative!("carePlan", SHOULD, "sectionPlanOfCare"),
    row(
        "EHDSDischargeReport.body.carePlan.carePlan",
        &["A.2.8.1"],
        None,
        "Composition.section:sectionPlanOfCare.entry",
        &[CARE_PLAN],
    ),
    row(
        "EHDSDischargeReport.body.carePlan.otherRecommendations",
        &["A.2.8.3"],
        SHOULD,
        "Composition.section:sectionPlanOfCare.text",
        ASSEMBLED,
    ),
    // Synthesis.
    row(
        "EHDSDischargeReport.body.synthesis",
        &["A.2.6.7"],
        SHOULD,
        "Composition.section:sectionSynthesis.text",
        ASSEMBLED,
    ),
    // Attachments.
    row(
        "EHDSDischargeReport.attachments",
        NONE,
        None,
        "Composition.section:sectionAttachments.entry",
        ASSEMBLED,
    ),
];

/// Builds one eHealth Network catalogue entry.
const fn ehn(id: &'static str, name: &'static str) -> EhnElement {
    EhnElement { id, name }
}

/// Builds one row.
const fn row(
    path: &'static str,
    ehn: &'static [&'static str],
    producer: Option<Populate>,
    eps: &'static str,
    contexts: &'static [&'static str],
) -> Row {
    Row {
        path,
        ehn,
        producer,
        eps,
        contexts,
    }
}
