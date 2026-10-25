// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription crosswalk (feature `prescription`).
//!
//! The rows key on the Xt-EHR `EHDSMedicationPrescription` 1.0.0 element
//! paths, name the eHealth Network ids of the *ePrescription and
//! eDispensation of Authorised Medicinal Products* guidelines Release 3.1,
//! §4, and the elements of the HL7 Europe Medication Prescription and
//! Dispense `MedicationRequest-eu-mpd` profile, 1.0.0. That profile carries
//! one prescription item per `MedicationRequest`, so the header elements
//! sit on every request of a prescription. No FHIRconnect context feeds a
//! row: a member that serves the category gives its own `MedicationRequest`.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe MPD `MedicationRequest` profile.
pub const MPD_MEDICATION_REQUEST: &str =
    "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd";

/// The electronic prescription crosswalk.
pub static PRESCRIPTION: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationPrescription",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationPrescriptionObligations",
    },
    profile: MPD_MEDICATION_REQUEST,
    companions: &[],
    sliced: "MedicationRequest.extension",
    ehn: EHN,
    rows: ROWS,
    gaps: &[],
};

/// The eHealth Network prescription dataset elements the rows name, from
/// the guideline's §4 table.
const EHN: &[EhnElement] = &[
    ehn("A.1", "Core data elements"),
    ehn("A.1.1", "Patient administrative data"),
    ehn("A.1.2", "Authentication of the prescription"),
    ehn("A.1.2.1", "Identifier of the Prescription"),
    ehn("A.1.2.2", "Issue date"),
    ehn(
        "A.1.3",
        "Identification of the prescribing health professional",
    ),
    ehn("A.1.4", "Identification of the prescribed product"),
    ehn("A.1.5", "Prescription information"),
    ehn("A.1.5.2", "Quantity of prescribed product"),
    ehn("A.1.5.3", "Dose regimen"),
    ehn("A.1.5.5", "Duration of treatment"),
    ehn("A.1.5.8", "Prescription expiry date"),
    ehn("A.1.5.9", "Repeats"),
    ehn("A.1.5.10", "Reason for prescription"),
    ehn("A.1.5.11", "Substitution"),
];

/// The catalogue entry `id`, `name`.
const fn ehn(id: &'static str, name: &'static str) -> EhnElement {
    EhnElement { id, name }
}

/// The row of `path`, its eHealth Network ids, its producer obligation and
/// the `MedicationRequest-eu-mpd` element that carries it.
const fn row(
    path: &'static str,
    ehn: &'static [&'static str],
    producer: Option<Populate>,
    eps: &'static str,
) -> Row {
    Row {
        path,
        ehn,
        producer,
        eps,
        contexts: &[],
    }
}

/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// The rows, in the model's order.
const ROWS: &[Row] = &[
    row(
        "EHDSMedicationPrescription.header.subject",
        &["A.1.1"],
        SHALL,
        "MedicationRequest.subject",
    ),
    row(
        "EHDSMedicationPrescription.header.identifier",
        &["A.1.2.1"],
        SHALL,
        "MedicationRequest.groupIdentifier",
    ),
    row(
        "EHDSMedicationPrescription.header.author[x]",
        &["A.1.3"],
        SHALL,
        "MedicationRequest.requester",
    ),
    row(
        "EHDSMedicationPrescription.header.date",
        &["A.1.2.2"],
        SHALL,
        "MedicationRequest.authoredOn",
    ),
    row(
        "EHDSMedicationPrescription.header.status",
        &[],
        SHALL,
        "MedicationRequest.status",
    ),
    row(
        "EHDSMedicationPrescription.header.language",
        &[],
        None,
        "MedicationRequest.language",
    ),
    row(
        "EHDSMedicationPrescription.header.statusReason[x]",
        &[],
        None,
        "MedicationRequest.statusReason",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem",
        &["A.1.5"],
        SHALL,
        "MedicationRequest",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.identifier",
        &["A.1.2.1"],
        SHALL,
        "MedicationRequest.identifier",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.status",
        &[],
        SHALL,
        "MedicationRequest.status",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.statusReason[x]",
        &[],
        None,
        "MedicationRequest.statusReason",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.medication",
        &["A.1.4"],
        SHALL,
        "MedicationRequest.medication[x]",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.indication[x]",
        &["A.1.5.10"],
        None,
        "MedicationRequest.reasonCode",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.periodOfUse",
        &["A.1.5.5"],
        None,
        "MedicationRequest.extension:effectiveDosePeriod",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.quantityPrescribed",
        &["A.1.5.2"],
        SHOULD,
        "MedicationRequest.dispenseRequest.extension:prescribedQuantity",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.dosageInstructions",
        &["A.1.5.3"],
        SHALL,
        "MedicationRequest.dosageInstruction",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.validityPeriod",
        &["A.1.5.8"],
        SHOULD,
        "MedicationRequest.dispenseRequest.validityPeriod",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.substitution",
        &["A.1.5.11"],
        SHOULD,
        "MedicationRequest.substitution",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.substitution.allowed[x]",
        &["A.1.5.11"],
        SHALL,
        "MedicationRequest.substitution.allowed[x]",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.substitution.reason[x]",
        &["A.1.5.11"],
        None,
        "MedicationRequest.substitution.reason",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.numberOfRepeats",
        &["A.1.5.9"],
        None,
        "MedicationRequest.dispenseRequest.numberOfRepeatsAllowed",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.minimumDispenseInterval",
        &["A.1.5.9"],
        None,
        "MedicationRequest.dispenseRequest.dispenseInterval",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.offLabel",
        &["A.1.5.10"],
        None,
        "MedicationRequest.extension:offLabelUse",
    ),
    row(
        "EHDSMedicationPrescription.prescriptionItem.note",
        &[],
        SHOULD,
        "MedicationRequest.note",
    ),
];
