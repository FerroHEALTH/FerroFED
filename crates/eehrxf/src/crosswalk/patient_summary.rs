// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary crosswalk (feature `patient-summary`).
//!
//! The rows key on the Xt-EHR `EHDSPatientSummary` 1.0.0 element paths, name
//! the eHealth Network ids of the *Guidelines on Patient Summary* Release
//! 3.4, §4 (Tables 2 and 3), and the elements of the HL7 Europe Patient
//! Summary `composition-eu-eps` profile, 1.0.0-ballot. Every section of the
//! model has a row, with its narrative, empty reason, note and entries, and
//! so has every header element a producer is able to populate. The section
//! narrative, the empty reason and the note are written by the document
//! assembly, as is the header, so no FHIRconnect context feeds them; each
//! entry list admits the profiles of its EPS entry slice.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Gap;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe Patient Summary composition profile.
pub const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The patient summary crosswalk.
pub static PATIENT_SUMMARY: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummary",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummaryObligations",
    },
    profile: EPS_COMPOSITION,
    sliced: "Composition.section",
    ehn: EHN,
    rows: ROWS,
    gaps: GAPS,
};

/// The elements open for the clinical safety review: the medical alert
/// (A.2.1.2) and the functional status (A.2.3.4), for which no openEHR
/// content is named to feed them.
const GAPS: &[Gap] = &[
    Gap {
        path: "EHDSPatientSummary.alerts.medicalAlert",
        ehn: "A.2.1.2",
    },
    Gap {
        path: "EHDSPatientSummary.functionalStatus",
        ehn: "A.2.3.4",
    },
];

/// The eHealth Network patient summary dataset elements the rows name, from
/// the guideline's Tables 2 and 3.
const EHN: &[EhnElement] = &[
    ehn("A.1", "Patient summary header data elements"),
    ehn("A.1.1", "Identification of the patient/subject"),
    ehn("A.1.2", "Contact information"),
    ehn("A.1.2.1", "Patient address"),
    ehn("A.1.2.2", "Preferred HP to contact"),
    ehn("A.1.2.3", "Contact person/ legal guardian"),
    ehn("A.1.3", "Insurance information"),
    ehn("A.1.4", "Document data"),
    ehn("A.1.4.1", "Date created"),
    ehn("A.1.4.2", "Date of last update"),
    ehn("A.1.4.3", "Nature of the PS"),
    ehn("A.1.5", "Author and Organisation"),
    ehn("A.1.5.1", "Author organisation"),
    ehn("A.1.5.2", "Legal authenticator"),
    ehn("A.1.6", "Additional information / Knowledge resources"),
    ehn("A.2", "Patient summary body data elements"),
    ehn("A.2.1", "Alerts"),
    ehn("A.2.1.1", "Allergy"),
    ehn(
        "A.2.1.2",
        "Medical alert information (other alerts not included in allergies)",
    ),
    ehn("A.2.2", "Medical history"),
    ehn("A.2.2.1", "Vaccination/ prophylaxis information"),
    ehn("A.2.2.2", "Resolved, closed or inactive problems"),
    ehn("A.2.2.3", "Medical history"),
    ehn("A.2.3", "Medical problems"),
    ehn("A.2.3.1", "Current problems"),
    ehn("A.2.3.2", "Medical devices and implants"),
    ehn("A.2.3.3", "Procedures"),
    ehn("A.2.3.4", "Functional status"),
    ehn("A.2.4", "Medication summary"),
    ehn("A.2.4.1", "Current and relevant past medicines"),
    ehn("A.2.5", "Social history"),
    ehn("A.2.6", "Pregnancy history"),
    ehn("A.2.6.1", "Current pregnancy status"),
    ehn("A.2.6.2", "History of previous pregnancies"),
    ehn("A.2.7", "Patient provided data"),
    ehn("A.2.7.1", "Travel history"),
    ehn("A.2.7.2", "Advance Directive"),
    ehn("A.2.8", "Results"),
    ehn("A.2.8.1", "Result observations"),
    ehn("A.2.9", "Plan of Care"),
    ehn("A.2.9.1", "Plan of care"),
];

/// HL7 Europe Base `allergyIntolerance-eu-core`.
const ALLERGY: &str = "http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core";
/// HL7 Europe Base `condition-eu-core`.
const CONDITION: &str = "http://hl7.eu/fhir/base/StructureDefinition/condition-eu-core";
/// HL7 Europe Base `flag-patient-eu-core`.
const FLAG: &str = "http://hl7.eu/fhir/base/StructureDefinition/flag-patient-eu-core";
/// HL7 Europe Base `immunization-eu-core`.
const IMMUNIZATION: &str = "http://hl7.eu/fhir/base/StructureDefinition/immunization-eu-core";
/// HL7 Europe Patient Summary `MedicationStatement-eu-eps`.
const MEDICATION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/MedicationStatement-eu-eps";
/// HL7 Europe Patient Summary `deviceUseStatement-eu-eps`.
const DEVICE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/deviceUseStatement-eu-eps";
/// HL7 Europe Patient Summary `procedure-eu-eps`.
const PROCEDURE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/procedure-eu-eps";
/// HL7 Europe Patient Summary `consent-eu-eps`.
const CONSENT: &str = "http://hl7.eu/fhir/eps/StructureDefinition/consent-eu-eps";
/// HL7 Europe Patient Summary `observation-travel-eu-eps`.
const TRAVEL: &str = "http://hl7.eu/fhir/eps/StructureDefinition/observation-travel-eu-eps";
/// HL7 International Patient Summary `Observation-pregnancy-status-uv-ips`.
const PREGNANCY_STATUS: &str =
    "http://hl7.org/fhir/uv/ips/StructureDefinition/Observation-pregnancy-status-uv-ips";
/// HL7 International Patient Summary `Observation-pregnancy-outcome-uv-ips`.
const PREGNANCY_OUTCOME: &str =
    "http://hl7.org/fhir/uv/ips/StructureDefinition/Observation-pregnancy-outcome-uv-ips";
/// FHIR R4 `Observation`.
const OBSERVATION: &str = "http://hl7.org/fhir/StructureDefinition/Observation";
/// FHIR R4 `ClinicalImpression`.
const CLINICAL_IMPRESSION: &str = "http://hl7.org/fhir/StructureDefinition/ClinicalImpression";
/// FHIR R4 `CarePlan`.
const CARE_PLAN: &str = "http://hl7.org/fhir/StructureDefinition/CarePlan";

/// No eHealth Network counterpart.
const NONE: &[&str] = &[];
/// No FHIRconnect context: the document assembly writes the element.
const ASSEMBLED: &[&str] = &[];

/// Builds the row of the section `$name`, carried by the EPS section slice
/// `$slice`.
macro_rules! section {
    ($name:literal, $ehn:expr, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSPatientSummary.", $name),
            $ehn,
            $producer,
            concat!("Composition.section:", $slice),
            ASSEMBLED,
        )
    };
}

/// Builds the row of the generated narrative of the section `$name`, the
/// section text of its EPS slice.
macro_rules! narrative {
    ($name:literal, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSPatientSummary.", $name, ".generatedNarrative"),
            NONE,
            $producer,
            concat!("Composition.section:", $slice, ".text"),
            ASSEMBLED,
        )
    };
}

/// Builds the row of the empty reason of the section `$name`, which a
/// producer shall be able to populate.
macro_rules! empty {
    ($name:literal, $slice:literal $(,)?) => {
        row(
            concat!("EHDSPatientSummary.", $name, ".emptyReason"),
            NONE,
            Some(Populate::Shall),
            concat!("Composition.section:", $slice, ".emptyReason"),
            ASSEMBLED,
        )
    };
}

/// Builds the row of the note of the section `$name`, the section note
/// extension of its EPS slice.
macro_rules! note {
    ($name:literal, $producer:expr, $slice:literal $(,)?) => {
        row(
            concat!("EHDSPatientSummary.", $name, ".note"),
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
        "EHDSPatientSummary.header",
        &["A.1"],
        Some(Populate::Shall),
        "Composition",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.subject",
        &["A.1.1", "A.1.2.1"],
        Some(Populate::Shall),
        "Composition.subject",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.identifier",
        NONE,
        Some(Populate::Shall),
        "Composition.identifier",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.author[x]",
        &["A.1.5.1"],
        Some(Populate::Shall),
        "Composition.author",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.date",
        &["A.1.4.1"],
        Some(Populate::Shall),
        "Composition.date",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.status",
        NONE,
        Some(Populate::Shall),
        "Composition.status",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.language",
        NONE,
        Some(Populate::Should),
        "Composition.language",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.documentType",
        NONE,
        Some(Populate::Shall),
        "Composition.type",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.documentTitle",
        NONE,
        Some(Populate::Shall),
        "Composition.title",
        ASSEMBLED,
    ),
    row(
        "EHDSPatientSummary.header.legalAuthentication",
        &["A.1.5.2"],
        Some(Populate::Shall),
        "Composition.attester:legalAuthenticator",
        ASSEMBLED,
    ),
    // Alerts.
    section!("alerts", &["A.2.1"], Some(Populate::Should), "sectionAlert"),
    narrative!("alerts", Some(Populate::Should), "sectionAlert"),
    row(
        "EHDSPatientSummary.alerts.medicalAlert",
        &["A.2.1.2"],
        Some(Populate::Shall),
        "Composition.section:sectionAlert.entry:flag",
        &[FLAG],
    ),
    note!("alerts", Some(Populate::Should), "sectionAlert"),
    // Allergies and intolerances.
    section!(
        "allergiesAndIntolerances",
        &["A.2.1.1"],
        Some(Populate::Shall),
        "sectionAllergies",
    ),
    narrative!(
        "allergiesAndIntolerances",
        Some(Populate::Should),
        "sectionAllergies",
    ),
    row(
        "EHDSPatientSummary.allergiesAndIntolerances.allergyIntolerance",
        &["A.2.1.1"],
        Some(Populate::Shall),
        "Composition.section:sectionAllergies.entry:allergyOrIntolerance",
        &[ALLERGY],
    ),
    empty!("allergiesAndIntolerances", "sectionAllergies"),
    note!(
        "allergiesAndIntolerances",
        Some(Populate::Should),
        "sectionAllergies",
    ),
    // Medical problems.
    section!(
        "problems",
        &["A.2.3.1"],
        Some(Populate::Shall),
        "sectionProblems"
    ),
    narrative!("problems", Some(Populate::Should), "sectionProblems"),
    empty!("problems", "sectionProblems"),
    row(
        "EHDSPatientSummary.problems.problem",
        &["A.2.3.1"],
        Some(Populate::Shall),
        "Composition.section:sectionProblems.entry:problem",
        &[CONDITION],
    ),
    note!("problems", Some(Populate::Should), "sectionProblems"),
    // Medication summary.
    section!(
        "medicationSummary",
        &["A.2.4"],
        Some(Populate::Shall),
        "sectionMedications",
    ),
    narrative!(
        "medicationSummary",
        Some(Populate::Should),
        "sectionMedications"
    ),
    empty!("medicationSummary", "sectionMedications"),
    row(
        "EHDSPatientSummary.medicationSummary.medicationUse",
        &["A.2.4.1"],
        Some(Populate::Shall),
        "Composition.section:sectionMedications.entry:medicationStatementOrRequest",
        &[MEDICATION],
    ),
    note!(
        "medicationSummary",
        Some(Populate::Should),
        "sectionMedications"
    ),
    // Medical devices and implants.
    section!(
        "medicalDevicesAndImplants",
        &["A.2.3.2"],
        Some(Populate::Shall),
        "sectionMedicalDevices",
    ),
    narrative!(
        "medicalDevicesAndImplants",
        Some(Populate::Should),
        "sectionMedicalDevices",
    ),
    empty!("medicalDevicesAndImplants", "sectionMedicalDevices"),
    row(
        "EHDSPatientSummary.medicalDevicesAndImplants.deviceUse",
        &["A.2.3.2"],
        Some(Populate::Shall),
        "Composition.section:sectionMedicalDevices.entry:deviceStatement",
        &[DEVICE],
    ),
    note!(
        "medicalDevicesAndImplants",
        Some(Populate::Should),
        "sectionMedicalDevices",
    ),
    // Procedures.
    section!(
        "procedures",
        &["A.2.3.3"],
        Some(Populate::Shall),
        "sectionProceduresHx",
    ),
    narrative!("procedures", Some(Populate::Should), "sectionProceduresHx"),
    empty!("procedures", "sectionProceduresHx"),
    row(
        "EHDSPatientSummary.procedures.procedure",
        &["A.2.3.3"],
        Some(Populate::Shall),
        "Composition.section:sectionProceduresHx.entry:procedure",
        &[PROCEDURE],
    ),
    note!("procedures", Some(Populate::Should), "sectionProceduresHx"),
    // Immunisations.
    section!(
        "immunisations",
        &["A.2.2.1"],
        Some(Populate::Should),
        "sectionImmunizations",
    ),
    narrative!(
        "immunisations",
        Some(Populate::Should),
        "sectionImmunizations"
    ),
    row(
        "EHDSPatientSummary.immunisations.immunisation",
        &["A.2.2.1"],
        Some(Populate::Shall),
        "Composition.section:sectionImmunizations.entry:immunization",
        &[IMMUNIZATION],
    ),
    note!(
        "immunisations",
        Some(Populate::Should),
        "sectionImmunizations"
    ),
    // Functional status.
    section!(
        "functionalStatus",
        &["A.2.3.4"],
        None,
        "sectionFunctionalStatus",
    ),
    narrative!("functionalStatus", None, "sectionFunctionalStatus"),
    row(
        "EHDSPatientSummary.functionalStatus.condition",
        &["A.2.3.4"],
        None,
        "Composition.section:sectionFunctionalStatus.entry:disability",
        &[CONDITION],
    ),
    row(
        "EHDSPatientSummary.functionalStatus.assessment",
        &["A.2.3.4"],
        None,
        "Composition.section:sectionFunctionalStatus.entry:functionalAssessment",
        &[CLINICAL_IMPRESSION],
    ),
    note!("functionalStatus", None, "sectionFunctionalStatus"),
    // Social history.
    section!("socialHistory", &["A.2.5"], None, "sectionSocialHistory",),
    narrative!("socialHistory", None, "sectionSocialHistory"),
    row(
        "EHDSPatientSummary.socialHistory.observation",
        &["A.2.5"],
        None,
        "Composition.section:sectionSocialHistory.entry",
        &[OBSERVATION],
    ),
    note!("socialHistory", None, "sectionSocialHistory"),
    // Pregnancy history.
    section!(
        "pregnancyHistory",
        &["A.2.6"],
        Some(Populate::Should),
        "sectionPregnancyHx",
    ),
    narrative!(
        "pregnancyHistory",
        Some(Populate::Should),
        "sectionPregnancyHx"
    ),
    row(
        "EHDSPatientSummary.pregnancyHistory.currentPregnancyStatus",
        &["A.2.6.1"],
        Some(Populate::Shall),
        "Composition.section:sectionPregnancyHx.entry:pregnancyStatus",
        &[PREGNANCY_STATUS],
    ),
    row(
        "EHDSPatientSummary.pregnancyHistory.previousPregnancies",
        &["A.2.6.2"],
        None,
        "Composition.section:sectionPregnancyHx.entry:pregnancyOutcome",
        &[PREGNANCY_OUTCOME],
    ),
    note!(
        "pregnancyHistory",
        Some(Populate::Should),
        "sectionPregnancyHx"
    ),
    // Travel history.
    section!("travelHistory", &["A.2.7.1"], None, "sectionTravelHx"),
    narrative!("travelHistory", None, "sectionTravelHx"),
    row(
        "EHDSPatientSummary.travelHistory.travelHistory",
        &["A.2.7.1"],
        None,
        "Composition.section:sectionTravelHx.entry:travelObservation",
        &[TRAVEL],
    ),
    note!("travelHistory", None, "sectionTravelHx"),
    // Patient story.
    section!("patientStory", NONE, None, "sectionPatientStory"),
    note!("patientStory", None, "sectionPatientStory"),
    // Advance directives.
    section!(
        "advanceDirectives",
        &["A.2.7.2"],
        None,
        "sectionAdvanceDirectives",
    ),
    narrative!("advanceDirectives", None, "sectionAdvanceDirectives"),
    row(
        "EHDSPatientSummary.advanceDirectives.advanceDirective",
        &["A.2.7.2"],
        None,
        "Composition.section:sectionAdvanceDirectives.entry:advanceDirectivesConsent",
        &[CONSENT],
    ),
    note!("advanceDirectives", None, "sectionAdvanceDirectives"),
    // Observation results.
    section!("observationResults", &["A.2.8"], None, "sectionResults"),
    narrative!("observationResults", None, "sectionResults"),
    row(
        "EHDSPatientSummary.observationResults.result",
        &["A.2.8.1"],
        None,
        "Composition.section:sectionResults.entry",
        &[OBSERVATION],
    ),
    note!("observationResults", None, "sectionResults"),
    // Care plans.
    section!("carePlans", &["A.2.9"], None, "sectionPlanOfCare"),
    narrative!("carePlans", None, "sectionPlanOfCare"),
    row(
        "EHDSPatientSummary.carePlans.carePlan",
        &["A.2.9.1"],
        None,
        "Composition.section:sectionPlanOfCare.entry:carePlan",
        &[CARE_PLAN],
    ),
    note!("carePlans", None, "sectionPlanOfCare"),
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
