// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic dispensation dataset, Xt-EHR `EHDSMedicationDispense`
//! 1.0.0, in the `MedicationDispense-eu-mpd` form (feature
//! `dispensation`).
//!
//! One [`Dispense`] is one `MedicationDispense`. The date of the dispense
//! record, `header.date`, is the profile's required `recorded` extension.
//! The element map is the crosswalk
//! [`DISPENSATION`](crate::crosswalk::dispensation::DISPENSATION).

use fhir_types::codec::Json as _;
use fhir_types::codec::Object;
use fhir_types::r4::dosage::Dosage;
use fhir_types::r4::extension::Extension;
use fhir_types::r4::extension::ExtensionValue;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::medication_dispense::MedicationDispense;
use fhir_types::r4::medication_dispense::MedicationDispenseMedication;
use fhir_types::r4::medication_dispense::MedicationDispensePerformer;
use fhir_types::r4::medication_dispense::MedicationDispenseSubstitution;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::reference::Reference;

use crate::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use crate::medication::Medicinal;
use crate::medication::Mpd;
use crate::medication::ReadError;
use crate::medication::WriteError;
use crate::medication::Written;
use crate::medication::annotation;
use crate::medication::claiming;
use crate::medication::decode;
use crate::medication::extension;
use crate::medication::note;
use crate::medication::single;
use crate::receive::conform::CheckError;
use crate::receive::conform::Conformance;

/// The canonical URL of the extension that carries when the dispense was
/// recorded, the type profile of the `MedicationDispense-eu-mpd` slice
/// `extension:recorded`, as a `valueDateTime`.
pub const RECORDED: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded";

/// One electronic dispensation, `EHDSMedicationDispense`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispense {
    /// The patient, `header.subject`, as the member references it.
    pub subject: Reference,
    /// The dispense record's business identifiers, `header.identifier`.
    pub identifier: Vec<Identifier>,
    /// Who issued the record, `header.author[x]`, one `performer.actor`
    /// each; one at least.
    pub author: Vec<Reference>,
    /// When the record was issued, `header.date`, a FHIR `dateTime`.
    pub date: String,
    /// The dispense's status, `header.status`, a code of the R4
    /// `medicationdispense-status` value set the profile binds.
    pub status: String,
    /// The language of the resource, `header.language`, a BCP 47 code.
    pub language: Option<String>,
    /// Where it was dispensed, `dispenseLocation`.
    pub location: Option<Reference>,
    /// Who received it, when not the patient, `receiver[x]`.
    pub receiver: Option<Reference>,
    /// The prescription items it was dispensed against, `relatedRequest`:
    /// each an `authorizingPrescription`, the Xt-EHR identifier in its
    /// `identifier`, or a literal reference to the `MedicationRequest`.
    pub related_request: Vec<Reference>,
    /// The product dispensed, `medication`.
    pub medication: Medicinal,
    /// How much was dispensed, `dispensedQuantity`.
    pub quantity: Quantity,
    /// When it was handed over, `timeOfDispensation`, a FHIR `dateTime`.
    pub time_of_dispensation: Option<String>,
    /// Whether the dispenser substituted the product,
    /// `substitutionOccurred`.
    pub substitution_occurred: Option<bool>,
    /// The dosage instructions, `dosageInstructions`, one `Dosage` per
    /// sequence.
    pub dosage: Vec<Dosage>,
    /// A comment, `note`.
    pub note: Option<String>,
}

impl Dispense {
    /// Reads one dispensation from the FHIR JSON text of its
    /// `MedicationDispense`.
    ///
    /// The text is read strictly: a repeated name, a property R4 does not
    /// define and anything the R4 model reads differently from the text are
    /// refused.
    ///
    /// # Errors
    ///
    /// Returns [`ReadError`] naming the first rule the text breaks.
    pub fn read(text: &str) -> Result<Self, ReadError> {
        let dispense = decode::<MedicationDispense>(text, "MedicationDispense")?;
        let missing = |element| ReadError::Missing { element };
        let (coded, reference) = match &dispense.medication {
            MedicationDispenseMedication::CodeableConcept(coded) => (Some(&**coded), None),
            MedicationDispenseMedication::Reference(reference) => (None, Some(&**reference)),
        };
        let medication = Medicinal::read(
            coded,
            reference,
            &dispense.contained,
            "MedicationDispense.medication[x]",
        )?;
        if dispense.performer.is_empty() {
            return Err(missing("MedicationDispense.performer"));
        }
        let date = extension(&dispense.extension, RECORDED, |value| match value {
            ExtensionValue::DateTime(date) => date.value.clone(),
            _ => None,
        })?
        .ok_or(missing("MedicationDispense.extension:recorded"))?;
        let substitution_occurred = dispense
            .substitution
            .as_ref()
            .map(|substitution| {
                substitution
                    .was_substituted
                    .value
                    .ok_or(missing("MedicationDispense.substitution.wasSubstituted"))
            })
            .transpose()?;
        Ok(Self {
            subject: dispense
                .subject
                .clone()
                .ok_or(missing("MedicationDispense.subject"))?,
            identifier: dispense.identifier.clone(),
            author: dispense
                .performer
                .iter()
                .map(|performer| performer.actor.clone())
                .collect(),
            date,
            status: dispense
                .status
                .value
                .clone()
                .ok_or(missing("MedicationDispense.status"))?,
            language: dispense
                .language
                .as_ref()
                .and_then(|code| code.value.clone()),
            location: dispense.location.clone(),
            receiver: single(&dispense.receiver, "MedicationDispense.receiver")?.cloned(),
            related_request: dispense.authorizing_prescription.clone(),
            medication,
            quantity: dispense
                .quantity
                .clone()
                .ok_or(missing("MedicationDispense.quantity"))?,
            time_of_dispensation: dispense
                .when_handed_over
                .as_ref()
                .and_then(|date| date.value.clone()),
            substitution_occurred,
            dosage: dispense.dosage_instruction.clone(),
            note: note(&dispense.note, "MedicationDispense.note")?,
        })
    }

    /// Returns the `MedicationDispense` of the dispensation, claiming
    /// `MedicationDispense-eu-mpd`.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Empty`] for a dispensation with no author.
    pub fn resource(&self) -> Result<MedicationDispense, WriteError> {
        if self.author.is_empty() {
            return Err(WriteError::Empty {
                element: "EHDSMedicationDispense.header.author[x]",
            });
        }
        let (medication, contained) = match self.medication.write() {
            (Written::Coded(coded), contained) => (
                MedicationDispenseMedication::CodeableConcept(coded),
                contained,
            ),
            (Written::Referenced(reference), contained) => (
                MedicationDispenseMedication::Reference(reference),
                contained,
            ),
        };
        Ok(MedicationDispense {
            id: None,
            meta: Some(claiming(MPD_MEDICATION_DISPENSE)),
            implicit_rules: None,
            language: self.language.as_deref().map(Into::into),
            text: None,
            contained: contained.into_iter().collect(),
            extension: vec![Extension {
                url: RECORDED.to_owned(),
                value: Some(ExtensionValue::DateTime(self.date.as_str().into())),
                ..Extension::default()
            }],
            modifier_extension: Vec::new(),
            identifier: self.identifier.clone(),
            part_of: Vec::new(),
            status: self.status.as_str().into(),
            status_reason: None,
            category: None,
            medication,
            subject: Some(self.subject.clone()),
            context: None,
            supporting_information: Vec::new(),
            performer: self
                .author
                .iter()
                .map(|actor| MedicationDispensePerformer {
                    actor: actor.clone(),
                    ..MedicationDispensePerformer::default()
                })
                .collect(),
            location: self.location.clone(),
            authorizing_prescription: self.related_request.clone(),
            r#type: None,
            quantity: Some(self.quantity.clone()),
            days_supply: None,
            when_prepared: None,
            when_handed_over: self.time_of_dispensation.as_deref().map(Into::into),
            destination: None,
            receiver: self.receiver.clone().into_iter().collect(),
            note: annotation(self.note.as_ref()),
            dosage_instruction: self.dosage.clone(),
            substitution: self.substitution_occurred.map(|substituted| {
                MedicationDispenseSubstitution {
                    was_substituted: substituted.into(),
                    ..MedicationDispenseSubstitution::default()
                }
            }),
            detected_issue: Vec::new(),
            event_history: Vec::new(),
        })
    }

    /// Returns the `MedicationDispense` as a FHIR JSON object.
    ///
    /// # Errors
    ///
    /// The errors of [`Dispense::resource`], and [`WriteError::Encode`] for
    /// a resource the codec cannot encode.
    pub fn to_json(&self) -> Result<Object, WriteError> {
        self.resource()?
            .to_json()
            .map_err(|source| WriteError::Encode { source })
    }
}

impl Mpd {
    /// Checks `dispense`, a `MedicationDispense` in FHIR JSON, against
    /// `MedicationDispense-eu-mpd`, and each `Medication` it contains
    /// against `Medication-eu-mpd`.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::NonConformant`] listing every finding of both
    /// profiles when the dispense breaks one.
    pub fn check_dispense(&self, dispense: &Object) -> Result<Conformance, CheckError> {
        self.check(&self.dispense, "MedicationDispense", dispense)
    }
}
