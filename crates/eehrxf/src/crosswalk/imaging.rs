// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging crosswalks (feature `imaging`).
//!
//! Art 14(1)(d) names medical imaging studies and the related imaging
//! reports, and the Xt-EHR package models each on its own, so the category
//! carries two crosswalks. [`IMAGING_REPORT`] keys on the Xt-EHR
//! `EHDSImagingReport` 1.0.0 element paths and names the elements of the
//! HL7 Europe Imaging Report 1.0.0-ballot `CompositionEuImaging` profile
//! and its `DiagnosticReportEuImaging` companion; [`IMAGING_STUDY`] keys on
//! `EHDSImagingStudy` and names the elements of `ImagingStudyEuImaging`.
//! Both name the eHealth Network ids of the *Guideline on Medical imaging
//! studies and reports* Release 1.1: §4.1, the medical imaging report data
//! set, for the report (`A.`), and §4.2, the imaging study manifest data
//! set, for the study (`B.`). The header and the section narratives are
//! written by the document assembly, so no FHIRconnect context feeds them.
//! The report's study section is assembled
//! from the `ImagingStudyEuImaging` resources the study crosswalk feeds.
//! `ImagingStudyEuImaging` has no element for the study's custodian or its
//! authoring date, so those rows name the resource and its `meta`.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe Imaging Report composition profile.
pub const IMAGING_COMPOSITION: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging";

/// The canonical URL of the HL7 Europe Imaging Report `DiagnosticReport`
/// profile, which the composition names.
pub const IMAGING_DIAGNOSTIC_REPORT: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/DiagnosticReportEuImaging";

/// The canonical URL of the HL7 Europe Imaging Report `ImagingStudy`
/// profile.
pub const IMAGING_STUDY_PROFILE: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging";

/// The imaging report crosswalk.
pub static IMAGING_REPORT: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReport",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReportObligations",
    },
    profile: IMAGING_COMPOSITION,
    companions: &[IMAGING_DIAGNOSTIC_REPORT],
    sliced: "Composition.section",
    ehn: EHN,
    rows: REPORT_ROWS,
    gaps: &[],
};

/// The imaging study crosswalk.
pub static IMAGING_STUDY: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudy",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudyObligations",
    },
    profile: IMAGING_STUDY_PROFILE,
    companions: &[],
    sliced: "ImagingStudy.identifier",
    ehn: EHN,
    rows: STUDY_ROWS,
    gaps: &[],
};

/// The eHealth Network medical imaging dataset elements the rows name, from
/// the guideline's report data set (A.1 to A.8, §4.1) and imaging study
/// manifest data set (B.1, §4.2).
const EHN: &[EhnElement] = &[
    ehn("A.1", "Report header data elements"),
    ehn("A.1.1", "Identification of the patient/subject"),
    ehn("A.1.2", "Patient/subject related contact information"),
    ehn("A.1.4", "Information recipient"),
    ehn("A.1.5", "Author"),
    ehn("A.1.6", "Legal authenticator"),
    ehn("A.1.6.4", "Legal authentication date and time"),
    ehn("A.1.7", "Result validator"),
    ehn("A.1.7.4", "Validation date and time"),
    ehn("A.1.8.0", "Document ID"),
    ehn("A.1.8.1", "Document type"),
    ehn("A.1.8.2", "Document status"),
    ehn("A.1.8.3", "Report date and time"),
    ehn("A.1.8.4", "Document title"),
    ehn("A.1.8.5", "Imaging procedure"),
    ehn("A.1.8.6", "Report custodian"),
    ehn("A.1.8.8", "Language"),
    ehn("A.1.8.11", "Accession number"),
    ehn("A.2", "Order information"),
    ehn("A.2.1", "Order ID"),
    ehn("A.2.2", "Order date and time"),
    ehn("A.3", "Order reason"),
    ehn("A.3.3", "Clinical question"),
    ehn("A.4", "Specimen information"),
    ehn("A.5", "Examination Report"),
    ehn("A.5.1", "Imaging procedure"),
    ehn("A.5.1.1", "Modality"),
    ehn("A.5.1.6", "Anatomical focus"),
    ehn("A.5.2", "Medication"),
    ehn("A.5.3", "Adverse reaction"),
    ehn("A.5.4", "Results"),
    ehn("A.5.5", "Conclusion"),
    ehn("A.5.5.1", "Impression"),
    ehn("A.5.5.2", "Coded conclusions"),
    ehn("A.5.6", "Recommendation"),
    ehn("A.5.6.1", "Description"),
    ehn("A.5.6.2", "Care plan"),
    ehn("A.7", "Comparison study"),
    ehn("A.8", "Presented form"),
    ehn("B.1", "Imaging study Manifest Dataset"),
    ehn("B.1.1", "Study instance UID"),
    ehn("B.1.2", "Description"),
    ehn("B.1.3", "Study custodian"),
    ehn("B.1.4", "List of Series"),
    ehn("B.1.4.1", "Series Description"),
    ehn("B.1.4.2", "Series Unique Identifier"),
    ehn("B.1.4.3", "Modality"),
    ehn("B.1.4.5", "Other series information"),
    ehn("B.1.4.6", "Series endpoint"),
    ehn("B.1.4.7", "List of Instances in the series"),
    ehn("B.1.4.7.1", "Instance Globally Unique Identifier"),
    ehn("B.1.4.7.2", "Instance Class Globally Unique Identifier"),
    ehn("B.1.4.7.3", "Instance Number"),
];

/// HL7 Europe Imaging Report `ServiceRequestOrderEuImaging`.
const ORDER: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ServiceRequestOrderEuImaging";
/// HL7 Europe Imaging Report `ProcedureEuImaging`.
const PROCEDURE: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ProcedureEuImaging";
/// HL7 Europe Imaging Report `ObservationFindingEuImaging`.
const FINDING: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ObservationFindingEuImaging";
/// HL7 Europe Imaging Report `ObservationRadiationDoseEuImaging`.
const RADIATION_DOSE: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/ObservationRadiationDoseEuImaging";
/// FHIR R4 `Observation`.
const OBSERVATION: &str = "http://hl7.org/fhir/StructureDefinition/Observation";
/// FHIR R4 `Condition`.
const CONDITION: &str = "http://hl7.org/fhir/StructureDefinition/Condition";
/// FHIR R4 `Device`.
const DEVICE: &str = "http://hl7.org/fhir/StructureDefinition/Device";
/// FHIR R4 `MedicationAdministration`.
const MEDICATION_ADMINISTRATION: &str =
    "http://hl7.org/fhir/StructureDefinition/MedicationAdministration";
/// FHIR R4 `AdverseEvent`.
const ADVERSE_EVENT: &str = "http://hl7.org/fhir/StructureDefinition/AdverseEvent";
/// FHIR R4 `CarePlan`.
const CARE_PLAN: &str = "http://hl7.org/fhir/StructureDefinition/CarePlan";
/// FHIR R4 `Specimen`.
const SPECIMEN: &str = "http://hl7.org/fhir/StructureDefinition/Specimen";

/// No eHealth Network counterpart.
const NONE: &[&str] = &[];
/// No FHIRconnect context: the document assembly writes the element.
const ASSEMBLED: &[&str] = &[];
/// The order a row sits in, fed by a context of the imaging order.
const ORDERED: &[&str] = &[ORDER];
/// The study a row sits in, fed by a context of the imaging study.
const STUDY: &[&str] = &[IMAGING_STUDY_PROFILE];
/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// The imaging report rows: the header elements, then the body in model
/// order.
const REPORT_ROWS: &[Row] = &[
    row(
        "EHDSImagingReport.header",
        &["A.1"],
        SHALL,
        "Composition",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.subject",
        &["A.1.1", "A.1.2"],
        SHALL,
        "Composition.subject",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.identifier",
        &["A.1.8.0"],
        SHALL,
        "Composition.identifier",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.author[x]",
        &["A.1.5"],
        SHALL,
        "Composition.author",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.date",
        &["A.1.8.3"],
        SHALL,
        "Composition.date",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.status",
        &["A.1.8.2"],
        SHALL,
        "Composition.status",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.language",
        &["A.1.8.8"],
        SHALL,
        "Composition.language",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.documentType",
        &["A.1.8.1"],
        SHALL,
        "Composition.type",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.documentTitle",
        &["A.1.8.4"],
        SHALL,
        "Composition.title",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.period",
        NONE,
        SHOULD,
        "Composition.event.period",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.attestation",
        &["A.1.7"],
        SHALL,
        "Composition.attester:resultValidator",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.attestation.attester[x]",
        &["A.1.7"],
        SHALL,
        "Composition.attester:resultValidator.party",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.attestation.datetime",
        &["A.1.7.4"],
        SHALL,
        "Composition.attester:resultValidator.time",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.legalAuthentication",
        &["A.1.6"],
        SHALL,
        "Composition.attester:legalAuthenticator",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.legalAuthentication.legalAuthenticator[x]",
        &["A.1.6"],
        SHALL,
        "Composition.attester:legalAuthenticator.party",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.legalAuthentication.datetime",
        &["A.1.6.4"],
        SHALL,
        "Composition.attester:legalAuthenticator.time",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.eventType",
        &["A.1.8.5"],
        SHOULD,
        "Composition.event.code",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.serviceSpecialty",
        NONE,
        None,
        "Composition.category",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.custodian",
        &["A.1.8.6"],
        SHOULD,
        "Composition.custodian",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.header.accessionNumber",
        &["A.1.8.11"],
        SHALL,
        "DiagnosticReport.basedOn:ServiceRequestOrderEuImagingaccession",
        ORDERED,
    ),
    row(
        "EHDSImagingReport.header.intendedRecipient[x]",
        &["A.1.4"],
        SHOULD,
        "Composition.extension:informationRecipient",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.presentedForm",
        &["A.8"],
        SHOULD,
        "DiagnosticReport.presentedForm",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body",
        NONE,
        SHALL,
        "Composition.section",
        ASSEMBLED,
    ),
    // The orders, in the order section.
    row(
        "EHDSImagingReport.body.orderInformation",
        &["A.2"],
        SHOULD,
        "Composition.section:order",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.orderInformation.orderId",
        &["A.2.1"],
        SHALL,
        "Composition.section:order.entry:order",
        ORDERED,
    ),
    row(
        "EHDSImagingReport.body.orderInformation.orderDateAndTime",
        &["A.2.2"],
        None,
        "Composition.section:order.entry:order",
        ORDERED,
    ),
    row(
        "EHDSImagingReport.body.orderInformation.orderPlacer[x]",
        NONE,
        None,
        "Composition.section:order.entry:order",
        ORDERED,
    ),
    row(
        "EHDSImagingReport.body.orderInformation.orderReason[x]",
        &["A.3"],
        SHOULD,
        "Composition.section:order.entry:order",
        ORDERED,
    ),
    row(
        "EHDSImagingReport.body.orderInformation.clinicalQuestion",
        &["A.3.3"],
        SHALL,
        "Composition.section:order.entry:order",
        ORDERED,
    ),
    // The supporting information, in the history section.
    row(
        "EHDSImagingReport.body.supportingInformation",
        NONE,
        SHOULD,
        "Composition.section:history",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.observation",
        NONE,
        SHOULD,
        "Composition.section:history.entry:vitals",
        &[OBSERVATION],
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.condition",
        NONE,
        SHOULD,
        "Composition.section:history.entry:problemlist",
        &[CONDITION],
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.priorMedicationAdministration",
        NONE,
        SHOULD,
        "Composition.section:history.entry:medication",
        &[MEDICATION_ADMINISTRATION],
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.device",
        NONE,
        SHOULD,
        "Composition.section:history.entry:implants",
        &[DEVICE],
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.pregnancyStatus",
        NONE,
        SHOULD,
        "Composition.section:history.entry",
        &[OBSERVATION],
    ),
    row(
        "EHDSImagingReport.body.supportingInformation.sexForClinicalUse",
        NONE,
        SHOULD,
        "Composition.section:history.text",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.specimen",
        &["A.4"],
        SHOULD,
        "DiagnosticReport.specimen",
        &[SPECIMEN],
    ),
    row(
        "EHDSImagingReport.body.exposureInformation",
        NONE,
        SHOULD,
        "Composition.section:procedure.entry:radiation-dose",
        &[RADIATION_DOSE],
    ),
    // The examination, in the procedure, findings and impression sections.
    row(
        "EHDSImagingReport.body.examinationReport",
        &["A.5"],
        SHALL,
        "Composition.section:procedure",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.examinationReport.modality",
        &["A.5.1.1"],
        SHALL,
        "Composition.section:imagingstudy.entry:imagingstudy",
        STUDY,
    ),
    row(
        "EHDSImagingReport.body.examinationReport.bodySite",
        &["A.5.1.6"],
        SHALL,
        "DiagnosticReport.extension:anatomical-region",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.examinationReport.imagingProcedures",
        &["A.5.1"],
        SHALL,
        "Composition.section:procedure.entry:procedure",
        &[PROCEDURE],
    ),
    row(
        "EHDSImagingReport.body.examinationReport.medicationAdministration",
        &["A.5.2"],
        SHALL,
        "Composition.section:procedure.entry",
        &[MEDICATION_ADMINISTRATION],
    ),
    row(
        "EHDSImagingReport.body.examinationReport.adverseReaction",
        &["A.5.3"],
        SHALL,
        "Composition.section:procedure.entry:adverse-event",
        &[ADVERSE_EVENT],
    ),
    row(
        "EHDSImagingReport.body.examinationReport.results[x]",
        &["A.5.4"],
        SHALL,
        "Composition.section:findings.entry:finding",
        &[OBSERVATION],
    ),
    row(
        "EHDSImagingReport.body.examinationReport.conclusion",
        &["A.5.5"],
        SHALL,
        "Composition.section:impression",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.examinationReport.conclusion.impression",
        &["A.5.5.1"],
        SHALL,
        "Composition.section:impression.text",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.examinationReport.conclusion.conditionOrFinding[x]",
        &["A.5.5.2"],
        SHALL,
        "Composition.section:impression.entry",
        &[CONDITION, FINDING],
    ),
    // The recommendation, the comparison and the study.
    row(
        "EHDSImagingReport.body.recommendation",
        &["A.5.6"],
        SHOULD,
        "Composition.section:recommendation",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.recommendation.description",
        &["A.5.6.1"],
        SHALL,
        "Composition.section:recommendation.text",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.body.recommendation.carePlan",
        &["A.5.6.2"],
        SHOULD,
        "Composition.section:recommendation.entry:suggestion",
        &[CARE_PLAN],
    ),
    row(
        "EHDSImagingReport.body.comparisonStudy",
        &["A.7"],
        SHALL,
        "Composition.section:comparison.entry:comparedstudy",
        STUDY,
    ),
    row(
        "EHDSImagingReport.dicomStudyMetadata",
        &["B.1"],
        SHALL,
        "Composition.section:imagingstudy",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingReport.attachments",
        NONE,
        SHOULD,
        "Composition.section:report.entry",
        ASSEMBLED,
    ),
];

/// The imaging study rows, in model order.
const STUDY_ROWS: &[Row] = &[
    row(
        "EHDSImagingStudy.header",
        &["B.1"],
        SHALL,
        "ImagingStudy",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.subject",
        NONE,
        SHALL,
        "ImagingStudy.subject",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.identifier",
        &["B.1.1"],
        SHALL,
        "ImagingStudy.identifier:studyInstanceUid",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.author[x]",
        NONE,
        SHALL,
        "ImagingStudy.interpreter",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.date",
        NONE,
        SHALL,
        "ImagingStudy.meta",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.status",
        NONE,
        SHALL,
        "ImagingStudy.status",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.header.language",
        NONE,
        SHOULD,
        "ImagingStudy.language",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.modality",
        &["B.1.4.3"],
        SHALL,
        "ImagingStudy.modality",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.bodySite",
        NONE,
        SHALL,
        "ImagingStudy.extension:anatomical-region",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.encounter",
        NONE,
        SHOULD,
        "ImagingStudy.encounter",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.started",
        NONE,
        SHALL,
        "ImagingStudy.started",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.basedOn",
        NONE,
        SHOULD,
        "ImagingStudy.basedOn",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.numberOfSeries",
        &["B.1.4"],
        SHALL,
        "ImagingStudy.numberOfSeries",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.numberOfInstances",
        NONE,
        SHALL,
        "ImagingStudy.numberOfInstances",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.description",
        &["B.1.2"],
        SHALL,
        "ImagingStudy.description",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.studyCustodian",
        &["B.1.3"],
        SHALL,
        "ImagingStudy",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.studyEndpoint",
        NONE,
        SHALL,
        "ImagingStudy.endpoint",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series",
        &["B.1.4"],
        SHALL,
        "ImagingStudy.series",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.seriesUid",
        &["B.1.4.2"],
        SHALL,
        "ImagingStudy.series.uid",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.number",
        &["B.1.4.5"],
        SHOULD,
        "ImagingStudy.series.number",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.seriesModality",
        &["B.1.4.3"],
        SHALL,
        "ImagingStudy.series.modality",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.description",
        &["B.1.4.1"],
        SHALL,
        "ImagingStudy.series.description",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.numberOfInstances",
        &["B.1.4.7"],
        SHALL,
        "ImagingStudy.series.numberOfInstances",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.seriesEndpoint",
        &["B.1.4.6"],
        SHALL,
        "ImagingStudy.series.endpoint",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.bodySite",
        NONE,
        SHALL,
        "ImagingStudy.series.bodySite",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.started",
        NONE,
        SHALL,
        "ImagingStudy.series.started",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries",
        &["B.1.4.7"],
        SHALL,
        "ImagingStudy.series.instance",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.instanceTitle",
        NONE,
        SHALL,
        "ImagingStudy.series.instance.title",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.instanceUid",
        &["B.1.4.7.1"],
        SHALL,
        "ImagingStudy.series.instance.uid",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.sopClass",
        &["B.1.4.7.2"],
        SHALL,
        "ImagingStudy.series.instance.sopClass",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.instanceNumber",
        &["B.1.4.7.3"],
        SHALL,
        "ImagingStudy.series.instance.number",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.numberOfFrames",
        NONE,
        SHOULD,
        "ImagingStudy.series.instance.extension:number-of-frames",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.keyImage",
        NONE,
        SHOULD,
        "ImagingStudy.series.instance",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.keyImage.flag",
        NONE,
        SHOULD,
        "ImagingStudy.series.instance",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.keyImage.reason",
        NONE,
        SHOULD,
        "ImagingStudy.series.instance",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.keyImage.seriesUID",
        NONE,
        SHALL,
        "ImagingStudy.series.uid",
        ASSEMBLED,
    ),
    row(
        "EHDSImagingStudy.series.instancesInTheSeries.keyImage.instanceUID",
        NONE,
        SHALL,
        "ImagingStudy.series.instance.uid",
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
