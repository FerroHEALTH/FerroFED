// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription dataset, Xt-EHR `EHDSMedicationPrescription`
//! 1.0.0, in the `MedicationRequest-eu-mpd` form (feature `prescription`).
//!
//! The profile carries one prescription item per `MedicationRequest`, so a
//! [`Prescription`] writes one request per item, each repeating the header.
//! The header's identifier is the requests' `groupIdentifier`; a
//! single-item prescription whose identifiers match its item's, as the
//! Xt-EHR obligations ask, is written without one and read back from the
//! item. The element map is the crosswalk
//! [`PRESCRIPTION`](crate::crosswalk::prescription::PRESCRIPTION).
//!
//! Two elements of the model are not carried. `header.status` of a
//! multi-item prescription, its aggregate status, has no element in a
//! `MedicationRequest`; the status of a single-item prescription is its
//! item's. `presentedForm` has no element in the profile.

use fhir_types::codec::Json as _;
use fhir_types::codec::Object;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::dosage::Dosage;
use fhir_types::r4::duration::Duration;
use fhir_types::r4::extension::Extension;
use fhir_types::r4::extension::ExtensionValue;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::medication_request::MedicationRequest;
use fhir_types::r4::medication_request::MedicationRequestDispenseRequest;
use fhir_types::r4::medication_request::MedicationRequestMedication;
use fhir_types::r4::medication_request::MedicationRequestSubstitution;
use fhir_types::r4::medication_request::MedicationRequestSubstitutionAllowed;
use fhir_types::r4::period::Period;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::reference::Reference;

use crate::crosswalk::prescription::MPD_MEDICATION_REQUEST;
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
use crate::receive::conform::CheckError;
use crate::receive::conform::Conformance;

/// The canonical URL of the extension that carries the number of
/// dispensations a prescription item has left, on
/// `MedicationRequest.dispenseRequest`, as a `valueUnsignedInt`.
///
/// No element of the Xt-EHR model or of the MPD profiles carries it:
/// `numberOfRepeatsAllowed` is the number authorised.
pub const REMAINING_DISPENSATIONS: &str =
    "https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations";

/// The FHIR R4 `StructureDefinition` of [`REMAINING_DISPENSATIONS`], as JSON.
///
/// A simple extension of `MedicationRequest.dispenseRequest`, at most once,
/// with a `valueUnsignedInt`, defined after FHIR R4 2.5.1 Defining
/// Extensions. The documentation site serves it at its canonical URL.
pub const REMAINING_DISPENSATIONS_DEFINITION: &str =
    include_str!("StructureDefinition-remaining-dispensations.json");

/// The canonical URL of the IHE MPD extension for the overall prescribed
/// quantity.
///
/// It is the type profile of the `MedicationRequest-eu-mpd` slice
/// `dispenseRequest.extension:prescribedQuantity`, written with a
/// `valueQuantity`.
pub const PRESCRIBED_QUANTITY: &str = "https://profiles.ihe.net/PHARM/MPD/StructureDefinition/ihe-ext-medicationrequest-prescribedQuantity";

/// The `MedicationRequest.intent` of a prescription item.
///
/// HL7 Europe MPD 1.0.0 writes it on the requests of a prescription no
/// `RequestGroup` orchestrates (its examples `200C` and `400C`), and
/// `option` on those one does; this module writes no `RequestGroup`.
pub const ORDER: &str = "order";

/// The header of a prescription, `EHDSMedicationPrescription.header`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescriptionHeader {
    /// The patient, `header.subject`, as the member references it.
    pub subject: Reference,
    /// The prescription's business identifiers, `header.identifier`; one
    /// at least, and at most one where the identifiers differ from a single
    /// item's, because `groupIdentifier` holds one.
    pub identifier: Vec<Identifier>,
    /// The prescriber, `header.author[x]`; the profile's `requester` holds
    /// one.
    pub author: Reference,
    /// When the prescription was issued, `header.date`, a FHIR `dateTime`.
    pub date: String,
    /// The language of the resource, `header.language`, a BCP 47 code.
    pub language: Option<String>,
}

/// Whether and how a prescription item may be substituted,
/// `prescriptionItem.substitution`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substitution {
    /// Whether substitution is allowed, `substitution.allowed[x]`.
    pub allowed: Allowed,
    /// Why, `substitution.reason[x]`.
    pub reason: Option<CodeableConcept>,
}

/// The two forms of `substitution.allowed[x]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allowed {
    /// The `boolean` form, which the Xt-EHR obligations ask every system to
    /// support.
    Boolean(bool),
    /// The coded form.
    Coded(CodeableConcept),
}

/// One prescription item, `EHDSMedicationPrescription.prescriptionItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescriptionItem {
    /// The item's business identifiers, `prescriptionItem.identifier`; one
    /// at least.
    pub identifier: Vec<Identifier>,
    /// The item's status, `prescriptionItem.status`, a code of the R4
    /// `medicationrequest-status` value set the profile binds.
    pub status: String,
    /// Why it has that status, `prescriptionItem.statusReason[x]`.
    pub status_reason: Option<CodeableConcept>,
    /// The prescribed product, `prescriptionItem.medication`.
    pub medication: Medicinal,
    /// Why it is prescribed, `prescriptionItem.indication[x]`.
    pub indication: Vec<CodeableConcept>,
    /// The overall prescribed quantity, `prescriptionItem.quantityPrescribed`.
    pub quantity_prescribed: Option<Quantity>,
    /// The dosage instructions, `prescriptionItem.dosageInstructions`, one
    /// `Dosage` per sequence; one at least.
    pub dosage: Vec<Dosage>,
    /// When the item may be dispensed, `prescriptionItem.validityPeriod`.
    pub validity_period: Option<Period>,
    /// Whether it may be substituted, `prescriptionItem.substitution`.
    pub substitution: Option<Substitution>,
    /// How many times it may be dispensed after the first,
    /// `prescriptionItem.numberOfRepeats`.
    pub repeats: Option<u32>,
    /// The least time between two dispensations,
    /// `prescriptionItem.minimumDispenseInterval`.
    pub minimum_dispense_interval: Option<Duration>,
    /// How many dispensations are left, as the member that holds the
    /// prescription states it ([`REMAINING_DISPENSATIONS`]); absent when it
    /// states none.
    pub remaining_dispensations: Option<u32>,
    /// A message to the dispenser, `prescriptionItem.note`.
    pub note: Option<String>,
}

/// One electronic prescription, `EHDSMedicationPrescription`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prescription {
    /// The header every item shares.
    pub header: PrescriptionHeader,
    /// The items; one at least.
    pub items: Vec<PrescriptionItem>,
}

impl Prescription {
    /// Reads one prescription from the FHIR JSON texts of its
    /// `MedicationRequest`s, one per item, in item order.
    ///
    /// Each text is read strictly: a repeated name, a property R4 does not
    /// define and anything the R4 model reads differently from the text are
    /// refused. The requests must agree on every header element and on
    /// their `groupIdentifier`, which only a single request may leave out;
    /// its identifiers are then the prescription's, as the Xt-EHR
    /// obligations on `header.identifier` ask of a single-item prescription.
    ///
    /// # Errors
    ///
    /// Returns [`ReadError`] naming the first rule a text breaks; no text at
    /// all is [`ReadError::Missing`] on `MedicationRequest`.
    pub fn read(texts: &[&str]) -> Result<Self, ReadError> {
        let requests = texts
            .iter()
            .map(|text| decode::<MedicationRequest>(text, "MedicationRequest"))
            .collect::<Result<Vec<_>, _>>()?;
        let (first, rest) = requests.split_first().ok_or(ReadError::Missing {
            element: "MedicationRequest",
        })?;
        let items = requests.iter().map(item).collect::<Result<Vec<_>, _>>()?;
        let mut header = header(first)?;
        for other in rest {
            agree(&header, first, other)?;
        }
        match (&first.group_identifier, items.as_slice()) {
            (Some(group), _) => header.identifier = vec![group.clone()],
            (None, [only]) => header.identifier.clone_from(&only.identifier),
            (None, _) => {
                return Err(ReadError::Missing {
                    element: "MedicationRequest.groupIdentifier",
                });
            }
        }
        Ok(Self { header, items })
    }

    /// Returns the `MedicationRequest`s of the prescription, one per item in
    /// item order, each claiming `MedicationRequest-eu-mpd` with the intent
    /// [`ORDER`].
    ///
    /// The header's identifier is written as every request's
    /// `groupIdentifier`, unless the prescription has one item whose
    /// identifiers are the header's.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Empty`] for a prescription with no item, a
    /// header or an item with no identifier, and an item with no dosage, and
    /// [`WriteError::Unrepresentable`] for more than one header identifier
    /// where a `groupIdentifier` is written.
    pub fn requests(&self) -> Result<Vec<MedicationRequest>, WriteError> {
        let group = match (self.items.as_slice(), self.header.identifier.as_slice()) {
            ([], _) => {
                return Err(WriteError::Empty {
                    element: "EHDSMedicationPrescription.prescriptionItem",
                });
            }
            (_, []) => {
                return Err(WriteError::Empty {
                    element: "EHDSMedicationPrescription.header.identifier",
                });
            }
            ([only], held) if only.identifier == held => None,
            (_, [group]) => Some(group.clone()),
            (_, _) => {
                return Err(WriteError::Unrepresentable {
                    element: "EHDSMedicationPrescription.header.identifier",
                });
            }
        };
        self.items
            .iter()
            .map(|item| request(&self.header, item, group.clone()))
            .collect()
    }

    /// Returns the `MedicationRequest`s as FHIR JSON objects, in item order.
    ///
    /// # Errors
    ///
    /// The errors of [`Prescription::requests`], and
    /// [`WriteError::Encode`] for a request the codec cannot encode.
    pub fn to_json(&self) -> Result<Vec<Object>, WriteError> {
        self.requests()?
            .iter()
            .map(|request| {
                request
                    .to_json()
                    .map_err(|source| WriteError::Encode { source })
            })
            .collect()
    }
}

impl Mpd {
    /// Checks `request`, a `MedicationRequest` in FHIR JSON, against
    /// `MedicationRequest-eu-mpd`, and each `Medication` it contains
    /// against `Medication-eu-mpd`.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::NonConformant`] listing every finding of both
    /// profiles when the request breaks one.
    pub fn check_request(&self, request: &Object) -> Result<Conformance, CheckError> {
        self.check(&self.request, "MedicationRequest", request)
    }
}

/// The header facts one request carries, its `groupIdentifier` aside.
fn header(request: &MedicationRequest) -> Result<PrescriptionHeader, ReadError> {
    Ok(PrescriptionHeader {
        subject: request.subject.clone(),
        identifier: Vec::new(),
        author: request.requester.clone().ok_or(ReadError::Missing {
            element: "MedicationRequest.requester",
        })?,
        date: request
            .authored_on
            .as_ref()
            .and_then(|date| date.value.clone())
            .ok_or(ReadError::Missing {
                element: "MedicationRequest.authoredOn",
            })?,
        language: request
            .language
            .as_ref()
            .and_then(|code| code.value.clone()),
    })
}

/// Refuses `other`, a request of the prescription whose first request is
/// `first`, where it states another header than `held`.
fn agree(
    held: &PrescriptionHeader,
    first: &MedicationRequest,
    other: &MedicationRequest,
) -> Result<(), ReadError> {
    let theirs = header(other)?;
    let disagree = |element| Err(ReadError::Disagree { element });
    if other.group_identifier != first.group_identifier {
        return disagree("MedicationRequest.groupIdentifier");
    }
    if theirs.subject != held.subject {
        return disagree("MedicationRequest.subject");
    }
    if theirs.author != held.author {
        return disagree("MedicationRequest.requester");
    }
    if theirs.date != held.date {
        return disagree("MedicationRequest.authoredOn");
    }
    if theirs.language != held.language {
        return disagree("MedicationRequest.language");
    }
    Ok(())
}

/// The prescription item one request carries.
fn item(request: &MedicationRequest) -> Result<PrescriptionItem, ReadError> {
    if request.identifier.is_empty() {
        return Err(ReadError::Missing {
            element: "MedicationRequest.identifier",
        });
    }
    if request.dosage_instruction.is_empty() {
        return Err(ReadError::Missing {
            element: "MedicationRequest.dosageInstruction",
        });
    }
    let (coded, reference) = match &request.medication {
        MedicationRequestMedication::CodeableConcept(coded) => (Some(&**coded), None),
        MedicationRequestMedication::Reference(reference) => (None, Some(&**reference)),
    };
    let medication = Medicinal::read(
        coded,
        reference,
        &request.contained,
        "MedicationRequest.medication[x]",
    )?;
    let dispense = request.dispense_request.as_ref();
    let extensions = dispense.map_or(&[][..], |dispense| dispense.extension.as_slice());
    let quantity_prescribed = extension(extensions, PRESCRIBED_QUANTITY, |value| match value {
        ExtensionValue::Quantity(quantity) => Some((**quantity).clone()),
        _ => None,
    })?;
    let remaining_dispensations =
        extension(extensions, REMAINING_DISPENSATIONS, |value| match value {
            ExtensionValue::UnsignedInt(count) => count.value,
            _ => None,
        })?;
    let status = request.status.value.clone().ok_or(ReadError::Missing {
        element: "MedicationRequest.status",
    })?;
    Ok(PrescriptionItem {
        identifier: request.identifier.clone(),
        status,
        status_reason: request.status_reason.clone(),
        medication,
        indication: request.reason_code.clone(),
        quantity_prescribed,
        dosage: request.dosage_instruction.clone(),
        validity_period: dispense.and_then(|dispense| dispense.validity_period.clone()),
        substitution: request
            .substitution
            .as_ref()
            .map(substitution)
            .transpose()?,
        repeats: dispense
            .and_then(|dispense| dispense.number_of_repeats_allowed.as_ref())
            .and_then(|count| count.value),
        minimum_dispense_interval: dispense.and_then(|dispense| dispense.dispense_interval.clone()),
        remaining_dispensations,
        note: note(&request.note, "MedicationRequest.note")?,
    })
}

/// The substitution a request's `substitution` states.
fn substitution(substitution: &MedicationRequestSubstitution) -> Result<Substitution, ReadError> {
    let allowed = match &substitution.allowed {
        MedicationRequestSubstitutionAllowed::Boolean(allowed) => {
            Allowed::Boolean(allowed.value.ok_or(ReadError::Missing {
                element: "MedicationRequest.substitution.allowed[x]",
            })?)
        }
        MedicationRequestSubstitutionAllowed::CodeableConcept(coded) => {
            Allowed::Coded((**coded).clone())
        }
    };
    Ok(Substitution {
        allowed,
        reason: substitution.reason.clone(),
    })
}

/// The request that carries `item` of a prescription with `header`.
fn request(
    header: &PrescriptionHeader,
    item: &PrescriptionItem,
    group: Option<Identifier>,
) -> Result<MedicationRequest, WriteError> {
    if item.identifier.is_empty() {
        return Err(WriteError::Empty {
            element: "EHDSMedicationPrescription.prescriptionItem.identifier",
        });
    }
    if item.dosage.is_empty() {
        return Err(WriteError::Empty {
            element: "EHDSMedicationPrescription.prescriptionItem.dosageInstructions",
        });
    }
    let (medication, contained) = match item.medication.write() {
        (Written::Coded(coded), contained) => (
            MedicationRequestMedication::CodeableConcept(coded),
            contained,
        ),
        (Written::Referenced(reference), contained) => {
            (MedicationRequestMedication::Reference(reference), contained)
        }
    };
    Ok(MedicationRequest {
        id: None,
        meta: Some(claiming(MPD_MEDICATION_REQUEST)),
        implicit_rules: None,
        language: header.language.as_deref().map(Into::into),
        text: None,
        contained: contained.into_iter().collect(),
        extension: Vec::new(),
        modifier_extension: Vec::new(),
        identifier: item.identifier.clone(),
        status: item.status.as_str().into(),
        status_reason: item.status_reason.clone(),
        intent: ORDER.into(),
        category: Vec::new(),
        priority: None,
        do_not_perform: None,
        reported: None,
        medication,
        subject: header.subject.clone(),
        encounter: None,
        supporting_information: Vec::new(),
        authored_on: Some(header.date.as_str().into()),
        requester: Some(header.author.clone()),
        performer: None,
        performer_type: None,
        recorder: None,
        reason_code: item.indication.clone(),
        reason_reference: Vec::new(),
        instantiates_canonical: Vec::new(),
        instantiates_uri: Vec::new(),
        based_on: Vec::new(),
        group_identifier: group,
        course_of_therapy_type: None,
        insurance: Vec::new(),
        note: annotation(item.note.as_ref()),
        dosage_instruction: item.dosage.clone(),
        dispense_request: dispense_request(item),
        substitution: item.substitution.as_ref().map(|substitution| {
            MedicationRequestSubstitution {
                allowed: match &substitution.allowed {
                    Allowed::Boolean(allowed) => {
                        MedicationRequestSubstitutionAllowed::Boolean((*allowed).into())
                    }
                    Allowed::Coded(coded) => MedicationRequestSubstitutionAllowed::CodeableConcept(
                        Box::new(coded.clone()),
                    ),
                },
                reason: substitution.reason.clone(),
                id: None,
                extension: Vec::new(),
                modifier_extension: Vec::new(),
            }
        }),
        prior_prescription: None,
        detected_issue: Vec::new(),
        event_history: Vec::new(),
    })
}

/// The `dispenseRequest` of `item`, or `None` when it states nothing that
/// goes there.
fn dispense_request(item: &PrescriptionItem) -> Option<MedicationRequestDispenseRequest> {
    let mut extension = Vec::new();
    if let Some(quantity) = &item.quantity_prescribed {
        extension.push(Extension {
            url: PRESCRIBED_QUANTITY.to_owned(),
            value: Some(ExtensionValue::Quantity(Box::new(quantity.clone()))),
            ..Extension::default()
        });
    }
    // NOTE: no specification governs this: our own design; the extension is written only
    // when the member states a value, so an absent count is never read as zero.
    if let Some(remaining) = item.remaining_dispensations {
        extension.push(Extension {
            url: REMAINING_DISPENSATIONS.to_owned(),
            value: Some(ExtensionValue::UnsignedInt(remaining.into())),
            ..Extension::default()
        });
    }
    let request = MedicationRequestDispenseRequest {
        extension,
        validity_period: item.validity_period.clone(),
        number_of_repeats_allowed: item.repeats.map(Into::into),
        dispense_interval: item.minimum_dispense_interval.clone(),
        ..MedicationRequestDispenseRequest::default()
    };
    (request != MedicationRequestDispenseRequest::default()).then_some(request)
}
