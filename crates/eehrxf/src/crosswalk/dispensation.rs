// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic dispensation crosswalk (feature `dispensation`).
//!
//! The rows key on the Xt-EHR `EHDSMedicationDispense` 1.0.0 element paths,
//! name the eHealth Network ids of the *ePrescription and eDispensation of
//! Authorised Medicinal Products* guidelines Release 3.1, §4, and the
//! elements of the HL7 Europe Medication Prescription and Dispense
//! `MedicationDispense-eu-mpd` profile, 1.0.0, whose one required extension
//! slice, `recorded`, carries the date of the dispense record. No
//! FHIRconnect context feeds a row.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the HL7 Europe MPD `MedicationDispense` profile.
pub const MPD_MEDICATION_DISPENSE: &str =
    "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd";

/// The electronic dispensation crosswalk.
pub static DISPENSATION: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationDispense",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSMedicationDispenseObligations",
    },
    profile: MPD_MEDICATION_DISPENSE,
    companions: &[],
    sliced: "MedicationDispense.extension",
    ehn: EHN,
    rows: ROWS,
    gaps: &[],
};

/// The eHealth Network dispensation dataset elements the rows name, from
/// the guideline's §4 table.
const EHN: &[EhnElement] = &[
    ehn(
        "B.1",
        "Dispensation information (provided by the dispensing pharmacy)",
    ),
    ehn("B.1.1", "Identifier of the dispenser"),
    ehn("B.1.4", "Identifier of the pharmacy"),
    ehn("B.1.5", "Address of the pharmacy"),
    ehn("B.1.7", "Identifier of the prescription"),
    ehn("B.1.8", "Medicinal product"),
    ehn("B.1.9", "Dispensed quantity"),
    ehn("B.1.10", "Dispensation date"),
    ehn("B.1.11", "Substitution"),
];

/// The catalogue entry `id`, `name`.
const fn ehn(id: &'static str, name: &'static str) -> EhnElement {
    EhnElement { id, name }
}

/// The row of `path`, its eHealth Network ids, its producer obligation and
/// the `MedicationDispense-eu-mpd` element that carries it.
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
        "EHDSMedicationDispense.header.subject",
        &[],
        SHALL,
        "MedicationDispense.subject",
    ),
    row(
        "EHDSMedicationDispense.header.identifier",
        &[],
        None,
        "MedicationDispense.identifier",
    ),
    row(
        "EHDSMedicationDispense.header.author[x]",
        &["B.1.1", "B.1.4"],
        SHALL,
        "MedicationDispense.performer.actor",
    ),
    row(
        "EHDSMedicationDispense.header.date",
        &[],
        SHALL,
        "MedicationDispense.extension:recorded",
    ),
    row(
        "EHDSMedicationDispense.header.status",
        &[],
        SHALL,
        "MedicationDispense.status",
    ),
    row(
        "EHDSMedicationDispense.header.language",
        &[],
        None,
        "MedicationDispense.language",
    ),
    row(
        "EHDSMedicationDispense.dispenseLocation",
        &["B.1.5"],
        None,
        "MedicationDispense.location",
    ),
    row(
        "EHDSMedicationDispense.receiver[x]",
        &[],
        None,
        "MedicationDispense.receiver",
    ),
    row(
        "EHDSMedicationDispense.relatedRequest",
        &["B.1.7"],
        SHALL,
        "MedicationDispense.authorizingPrescription",
    ),
    row(
        "EHDSMedicationDispense.medication",
        &["B.1.8"],
        SHALL,
        "MedicationDispense.medication[x]",
    ),
    row(
        "EHDSMedicationDispense.dispensedQuantity",
        &["B.1.9"],
        SHALL,
        "MedicationDispense.quantity",
    ),
    row(
        "EHDSMedicationDispense.timeOfDispensation",
        &["B.1.10"],
        None,
        "MedicationDispense.whenHandedOver",
    ),
    row(
        "EHDSMedicationDispense.substitutionOccurred",
        &["B.1.11"],
        None,
        "MedicationDispense.substitution.wasSubstituted",
    ),
    row(
        "EHDSMedicationDispense.dosageInstructions",
        &[],
        SHOULD,
        "MedicationDispense.dosageInstruction",
    ),
    row(
        "EHDSMedicationDispense.note",
        &[],
        None,
        "MedicationDispense.note",
    ),
];
