// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report crosswalk (feature `laboratory`).
//!
//! The rows key on the Xt-EHR `EHDSLaboratoryReport` 1.0.0 element paths,
//! name the eHealth Network ids of the *Guideline on Laboratory Results*
//! Release 1.1, §4.1 and §4.2, and the elements of the HL7 Europe Laboratory
//! Report 2.0.0 profiles that carry them. A laboratory report is a document
//! whose `Composition-eu-lab` names the header and the `DiagnosticReport-eu-lab`
//! carries the body: the orders it answers in `basedOn`, the specimens in
//! `specimen` and the results in `result`. The rows of the order information
//! and the supporting information name `basedOn`, because the
//! `ServiceRequest-eu-lab` it references carries them. The header and the
//! narrative are written by the document assembly, so no FHIRconnect context
//! feeds them.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe Laboratory Report composition profile.
pub const LAB_COMPOSITION: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab";

/// The canonical URL of the HL7 Europe Laboratory Report `DiagnosticReport`
/// profile, the body of the report.
pub const LAB_DIAGNOSTIC_REPORT: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab";

/// The laboratory report crosswalk.
pub static LABORATORY: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryReport",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryReportObligations",
    },
    profile: LAB_COMPOSITION,
    companions: &[LAB_DIAGNOSTIC_REPORT],
    sliced: "Composition.section",
    ehn: EHN,
    rows: ROWS,
    gaps: &[],
};

/// The eHealth Network laboratory result report dataset elements the rows
/// name, from the guideline's §4.1 (header) and §4.2 (body).
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
    ehn("A.1.8.0", "Document Id"),
    ehn("A.1.8.1", "Document type"),
    ehn("A.1.8.2", "Document status"),
    ehn("A.1.8.3", "Report date and time"),
    ehn("A.1.8.4", "Document title"),
    ehn("A.1.8.5", "Study type"),
    ehn("A.1.8.6", "Report custodian"),
    ehn("A.1.8.8", "Language"),
    ehn("A.2", "Order information"),
    ehn("A.2.1", "Order Id"),
    ehn("A.2.2", "Order date and time"),
    ehn("A.2.3", "Order placer identifier"),
    ehn("A.2.4", "Order placer name"),
    ehn("A.2.5", "Order placer contact details"),
    ehn("A.2.6", "Order placer organization"),
    ehn("A.3", "Order reason"),
    ehn("A.3.1", "Problem / diagnosis / condition description"),
    ehn("A.4", "Specimen information"),
    ehn("A.5", "Results data elements"),
    ehn("A.5.1.1", "Narrative report"),
    ehn("A.5.1.2", "Comments, interpretation and recommendations"),
    ehn("A.5.2", "Observation details"),
];

/// HL7 Europe Laboratory Report `ServiceRequest-eu-lab`.
const SERVICE_REQUEST: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/ServiceRequest-eu-lab";
/// HL7 Europe Laboratory Report `Specimen-eu-lab`.
const SPECIMEN: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition/Specimen-eu-lab";
/// HL7 Europe Laboratory Report `Observation-resultslab-eu-lab`.
const RESULT: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab";

/// No eHealth Network counterpart.
const NONE: &[&str] = &[];
/// No FHIRconnect context: the document assembly writes the element.
const ASSEMBLED: &[&str] = &[];
/// The order a row sits in, fed by a context of the laboratory service
/// request.
const ORDER: &[&str] = &[SERVICE_REQUEST];
/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// The rows: the header elements, then the body in model order.
const ROWS: &[Row] = &[
    row(
        "EHDSLaboratoryReport.header",
        &["A.1"],
        SHALL,
        "Composition",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.subject",
        &["A.1.1", "A.1.2"],
        SHALL,
        "Composition.subject",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.identifier",
        &["A.1.8.0"],
        SHALL,
        "Composition.identifier",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.author[x]",
        &["A.1.5"],
        SHALL,
        "Composition.author",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.date",
        &["A.1.8.3"],
        SHALL,
        "Composition.date",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.status",
        &["A.1.8.2"],
        SHALL,
        "Composition.status",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.language",
        &["A.1.8.8"],
        SHALL,
        "Composition.language",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.documentType",
        &["A.1.8.1"],
        SHALL,
        "Composition.type",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.documentTitle",
        &["A.1.8.4"],
        SHALL,
        "Composition.title",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.period",
        NONE,
        SHALL,
        "Composition.event.period",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.attestation",
        &["A.1.7"],
        SHALL,
        "Composition.attester:validator",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.attestation.attester[x]",
        &["A.1.7"],
        SHALL,
        "Composition.attester:validator.party",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.attestation.datetime",
        &["A.1.7.4"],
        SHALL,
        "Composition.attester:validator.time",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.legalAuthentication",
        &["A.1.6"],
        SHALL,
        "Composition.attester:legalAuthenticator",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.legalAuthentication.legalAuthenticator[x]",
        &["A.1.6"],
        SHALL,
        "Composition.attester:legalAuthenticator.party",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.legalAuthentication.datetime",
        &["A.1.6.4"],
        SHALL,
        "Composition.attester:legalAuthenticator.time",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.eventType",
        &["A.1.8.5"],
        SHALL,
        "Composition.category:studyType",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.serviceSpecialty",
        NONE,
        SHALL,
        "Composition.category:specialty",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.custodian",
        &["A.1.8.6"],
        SHALL,
        "Composition.custodian",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.header.intendedRecipient[x]",
        &["A.1.4"],
        None,
        "Composition.extension:informationRecipient",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.presentedForm",
        &["A.5.1.1"],
        SHALL,
        "DiagnosticReport.presentedForm",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.body",
        NONE,
        SHALL,
        "Composition.section",
        ASSEMBLED,
    ),
    // The orders the report answers, carried by the service requests.
    row(
        "EHDSLaboratoryReport.body.orderInformation",
        &["A.2"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.orderInformation.orderId",
        &["A.2.1"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.orderInformation.orderDateAndTime",
        &["A.2.2"],
        None,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.orderInformation.orderPlacer[x]",
        &["A.2.3", "A.2.4", "A.2.5", "A.2.6"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.orderInformation.orderReason[x]",
        &["A.3", "A.3.1"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.orderInformation.clinicalQuestion",
        NONE,
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    // The supporting information of the orders.
    row(
        "EHDSLaboratoryReport.body.supportingInformation",
        NONE,
        SHOULD,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.observation",
        NONE,
        SHOULD,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.condition",
        &["A.3.1"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.medicationAdministration",
        NONE,
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.sexForClinicalUse",
        NONE,
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.vaccination",
        NONE,
        SHOULD,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.supportingInformation.otherSupportingInformation",
        NONE,
        SHOULD,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    // The specimens, the service requests and the results.
    row(
        "EHDSLaboratoryReport.body.specimen",
        &["A.4"],
        SHALL,
        "DiagnosticReport.specimen",
        &[SPECIMEN],
    ),
    row(
        "EHDSLaboratoryReport.body.serviceRequest",
        &["A.2"],
        SHALL,
        "DiagnosticReport.basedOn",
        ORDER,
    ),
    row(
        "EHDSLaboratoryReport.body.resultData",
        &["A.5"],
        SHALL,
        "DiagnosticReport.result",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.body.resultData.commentsInterpretationAndRecommendations",
        &["A.5.1.2"],
        SHALL,
        "DiagnosticReport.conclusion",
        ASSEMBLED,
    ),
    row(
        "EHDSLaboratoryReport.body.resultData.laboratoryTestResults",
        &["A.5.2"],
        SHALL,
        "DiagnosticReport.result",
        &[RESULT],
    ),
    row(
        "EHDSLaboratoryReport.attachments",
        NONE,
        SHOULD,
        "Composition.section:attachment.entry",
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
