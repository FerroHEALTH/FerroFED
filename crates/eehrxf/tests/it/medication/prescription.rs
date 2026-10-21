// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription read from and written as
//! `MedicationRequest-eu-mpd`, held to the vendored MPD profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::dataset::constraint::DiscriminatorKind;
use eehrxf::medication::MPD_MEDICATION;
use eehrxf::medication::Medicinal;
use eehrxf::medication::ReadError;
use eehrxf::medication::WriteError;
use eehrxf::medication::prescription::Allowed;
use eehrxf::medication::prescription::PRESCRIBED_QUANTITY;
use eehrxf::medication::prescription::Prescription;
use eehrxf::medication::prescription::PrescriptionHeader;
use eehrxf::medication::prescription::PrescriptionItem;
use eehrxf::medication::prescription::REMAINING_DISPENSATIONS;
use eehrxf::medication::prescription::Substitution;
use eehrxf::receive::conform::FindingKind;
use eehrxf::receive::conform::Unread;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::dosage::Dosage;
use fhir_types::r4::medication::Medication;
use fhir_types::r4::period::Period;

use super::fixtures::coded;
use super::fixtures::examples;
use super::fixtures::findings;
use super::fixtures::identifier;
use super::fixtures::object;
use super::fixtures::profiles;
use super::fixtures::quantity;
use super::fixtures::reference;

/// The canonical URL of `MedicationRequest-eu-mpd`.
const PROFILE: &str = "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd";

/// A synthetic item `value`, coded `code`, with `remaining` dispensations
/// left.
fn item(value: &str, code: &str, remaining: Option<u32>) -> PrescriptionItem {
    PrescriptionItem {
        identifier: vec![identifier(value)],
        status: String::from("active"),
        status_reason: None,
        medication: Medicinal::Coded(coded(code)),
        indication: vec![coded("indication-1")],
        quantity_prescribed: Some(quantity("30")),
        dosage: vec![Dosage {
            text: Some("one a day".into()),
            ..Dosage::default()
        }],
        validity_period: Some(Period {
            start: Some("2026-01-01".into()),
            end: Some("2026-06-30".into()),
            ..Period::default()
        }),
        substitution: Some(Substitution {
            allowed: Allowed::Boolean(true),
            reason: None,
        }),
        repeats: Some(2),
        minimum_dispense_interval: None,
        remaining_dispensations: remaining,
        note: Some(String::from("synthetic note")),
    }
}

/// A synthetic prescription with `identifier` and `items`.
fn prescription(identifiers: &[&str], items: Vec<PrescriptionItem>) -> Prescription {
    Prescription {
        header: PrescriptionHeader {
            subject: reference("Patient/example-patient"),
            identifier: identifiers.iter().map(|value| identifier(value)).collect(),
            author: reference("Practitioner/example-prescriber"),
            date: String::from("2026-01-01"),
            language: None,
        },
        items,
    }
}

/// A single-item synthetic prescription with `remaining` dispensations
/// left.
fn single(remaining: Option<u32>) -> Prescription {
    prescription(&["RX-0001"], vec![item("RX-0001", "product-1", remaining)])
}

/// Writes `prescription` and returns its requests as JSON texts.
fn texts(prescription: &Prescription) -> Result<Vec<String>, Box<dyn Error>> {
    let mut texts = Vec::new();
    for object in prescription.to_json()? {
        texts.push(serde_json::to_string(&Value::Object(object))?);
    }
    Ok(texts)
}

/// Reads `texts` back as one prescription.
fn read(texts: &[String]) -> Result<Prescription, ReadError> {
    let borrowed: Vec<&str> = texts.iter().map(String::as_str).collect();
    Prescription::read(&borrowed)
}

/// The one request of `requests`.
fn one(requests: Vec<Object>) -> Result<Object, Box<dyn Error>> {
    let [request] = <[Object; 1]>::try_from(requests)
        .map_err(|requests| format!("{} requests, one expected", requests.len()))?;
    Ok(request)
}

/// The extensions of `request`'s `dispenseRequest` at `url`.
fn dispense_extensions<'a>(request: &'a Object, url: &str) -> Vec<&'a Object> {
    request
        .get("dispenseRequest")
        .and_then(|dispense| dispense.get("extension"))
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_object)
        .filter(|extension| extension.get("url").and_then(Value::as_str) == Some(url))
        .collect()
}

/// The single request text `prescription` writes, edited by `edit`.
fn edited(edit: impl Fn(&mut Object)) -> Result<String, Box<dyn Error>> {
    let mut objects = single(Some(1)).to_json()?;
    let mut request = objects.pop().ok_or("one request is written")?;
    edit(&mut request);
    Ok(serde_json::to_string(&Value::Object(request))?)
}

#[test]
fn a_written_single_item_prescription_conforms_to_mpd() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let request = one(single(Some(1)).to_json()?)?;
    let conformance = mpd.check_request(&request)?;
    let extension_by_url = conformance.unevaluated().iter().any(|unevaluated| {
        unevaluated.profile == PROFILE
            && unevaluated.element == "MedicationRequest.dispenseRequest.extension"
            && unevaluated.reason
                == Unread::Discriminator {
                    kind: DiscriminatorKind::Value,
                    path: String::from("url"),
                }
    });
    assert!(
        extension_by_url,
        "the url discriminator of an extension slice is named as not evaluated: {conformance:?}"
    );
    Ok(())
}

#[test]
fn a_written_multi_item_prescription_conforms_and_shares_its_group() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let written = prescription(
        &["RX-0002"],
        vec![
            item("RX-0002-1", "product-1", None),
            item("RX-0002-2", "product-2", Some(0)),
        ],
    );
    let requests = written.to_json()?;
    assert_eq!(requests.len(), 2);
    for request in &requests {
        mpd.check_request(request)?;
        let group = request
            .get("groupIdentifier")
            .and_then(|group| group.get("value"))
            .and_then(Value::as_str);
        assert_eq!(group, Some("RX-0002"));
    }
    Ok(())
}

#[test]
fn a_single_item_prescription_round_trips() -> Result<(), Box<dyn Error>> {
    let written = single(Some(1));
    let texts = texts(&written)?;
    let request = object(texts.first().ok_or("one request")?)?;
    assert!(
        !request.contains_key("groupIdentifier"),
        "a single item that carries the prescription identifier needs no group"
    );
    assert_eq!(read(&texts)?, written);
    Ok(())
}

#[test]
fn a_multi_item_prescription_round_trips() -> Result<(), Box<dyn Error>> {
    let written = prescription(
        &["RX-0003"],
        vec![
            item("RX-0003-1", "product-1", Some(3)),
            item("RX-0003-2", "product-2", None),
        ],
    );
    assert_eq!(read(&texts(&written)?)?, written);
    Ok(())
}

#[test]
fn a_member_request_reads_and_writes_back_the_same_json() -> Result<(), Box<dyn Error>> {
    let text = r#"{"resourceType":"MedicationRequest","meta":{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd"]},"identifier":[{"system":"urn:oid:2.999.1.7.1","value":"RX-0004"}],"status":"active","intent":"order","medicationCodeableConcept":{"coding":[{"system":"urn:oid:2.999.1.7.2","code":"product-4"}]},"subject":{"reference":"Patient/example-patient"},"authoredOn":"2026-02-01","requester":{"reference":"Practitioner/example-prescriber"},"dosageInstruction":[{"text":"two a day"}],"dispenseRequest":{"extension":[{"url":"https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations","valueUnsignedInt":4}],"numberOfRepeatsAllowed":5}}"#;
    let prescription = Prescription::read(&[text])?;
    let [item] = prescription.items.as_slice() else {
        return Err("one item".into());
    };
    assert_eq!(item.remaining_dispensations, Some(4));
    assert_eq!(item.repeats, Some(5));
    assert_eq!(prescription.to_json()?, vec![object(text)?]);
    Ok(())
}

#[test]
fn the_remaining_dispensations_extension_is_written_when_a_value_is_given()
-> Result<(), Box<dyn Error>> {
    for remaining in [0, 3] {
        let request = one(single(Some(remaining)).to_json()?)?;
        let found = dispense_extensions(&request, REMAINING_DISPENSATIONS);
        let [extension] = found.as_slice() else {
            return Err(format!("one extension for {remaining}: {found:?}").into());
        };
        assert_eq!(
            match extension.get("valueUnsignedInt") {
                Some(Value::Number(number)) => number.as_u64(),
                _ => None,
            },
            Some(u64::from(remaining))
        );
    }
    Ok(())
}

#[test]
fn the_remaining_dispensations_extension_is_absent_when_none_is_given() -> Result<(), Box<dyn Error>>
{
    let request = one(single(None).to_json()?)?;
    assert_eq!(
        dispense_extensions(&request, REMAINING_DISPENSATIONS),
        Vec::<&std::collections::BTreeMap<String, Value>>::new()
    );
    assert_eq!(dispense_extensions(&request, PRESCRIBED_QUANTITY).len(), 1);
    let mut bare = single(None);
    for item in &mut bare.items {
        item.quantity_prescribed = None;
        item.validity_period = None;
        item.repeats = None;
    }
    let request = one(bare.to_json()?)?;
    assert!(
        !request.contains_key("dispenseRequest"),
        "an item with nothing to dispense-request writes none"
    );
    Ok(())
}

#[test]
fn a_contained_medication_round_trips_and_is_held_to_medication_eu_mpd()
-> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let mut written = single(None);
    for item in &mut written.items {
        item.medication = Medicinal::Contained {
            id: String::from("med-1"),
            medication: Box::new(Medication {
                code: Some(coded("product-5")),
                ..Medication::default()
            }),
        };
    }
    let request = one(written.to_json()?)?;
    mpd.check_request(&request)?;
    let text = serde_json::to_string(&Value::Object(request))?;
    assert_eq!(Prescription::read(&[&text])?, written);
    let mut request = one(written.to_json()?)?;
    if let Some(Value::Array(contained)) = request.get_mut("contained")
        && let Some(Value::Object(medication)) = contained.first_mut()
    {
        let mut ingredient = Object::new();
        ingredient.insert(String::from("isActive"), Value::Bool(true));
        medication.insert(
            String::from("ingredient"),
            Value::Array(vec![Value::Object(ingredient)]),
        );
    }
    let found = findings(mpd.check_request(&request))?;
    assert!(
        found.iter().any(|finding| finding.profile == MPD_MEDICATION
            && finding.element == "Medication.ingredient.item[x]"
            && matches!(finding.kind, FindingKind::TooFew { min: 1, found: 0 })),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn a_request_without_its_authoring_date_breaks_the_profile() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let mut objects = single(None).to_json()?;
    let request = objects.first_mut().ok_or("one request")?;
    request.remove("authoredOn");
    let found = findings(mpd.check_request(request))?;
    assert!(
        found
            .iter()
            .any(|finding| finding.element == "MedicationRequest.authoredOn"
                && matches!(finding.kind, FindingKind::TooFew { min: 1, found: 0 })),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn every_mpd_request_example_conforms_and_reads() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let texts = examples("MedicationRequest-")?;
    assert_eq!(texts.len(), 8);
    for text in &texts {
        mpd.check_request(&object(text)?)?;
    }
    let single_line = texts
        .iter()
        .find(|text| text.contains("400C-prescription-cefuroxime-singleline"))
        .ok_or("the single-line example")?;
    let read = Prescription::read(&[single_line])?;
    assert_eq!(
        read.header.identifier,
        read.items.first().ok_or("one item")?.identifier
    );
    Ok(())
}

#[test]
fn text_that_is_no_medication_request_is_refused() {
    assert!(matches!(
        Prescription::read(&["not json"]),
        Err(ReadError::Json { .. })
    ));
    assert!(matches!(
        Prescription::read(&[
            r#"{"resourceType":"MedicationRequest","status":"active","status":"stopped"}"#
        ]),
        Err(ReadError::RepeatedName { .. })
    ));
    assert!(matches!(
        Prescription::read(&["[]"]),
        Err(ReadError::NotAnObject)
    ));
    assert!(matches!(
        Prescription::read(&[r#"{"resourceType":"MedicationDispense"}"#]),
        Err(ReadError::WrongType {
            expected: "MedicationRequest"
        })
    ));
    assert!(matches!(
        Prescription::read(&[]),
        Err(ReadError::Missing {
            element: "MedicationRequest"
        })
    ));
}

#[test]
fn a_property_r4_does_not_define_is_refused() -> Result<(), Box<dyn Error>> {
    let text = edited(|request| {
        request.insert(String::from("remainingRepeats"), Value::from(2_u32));
    })?;
    assert!(matches!(
        Prescription::read(&[&text]),
        Err(ReadError::Decode { .. })
    ));
    Ok(())
}

#[test]
fn a_request_missing_a_dataset_element_is_refused() -> Result<(), Box<dyn Error>> {
    for (key, element) in [
        ("identifier", "MedicationRequest.identifier"),
        ("requester", "MedicationRequest.requester"),
        ("authoredOn", "MedicationRequest.authoredOn"),
        ("dosageInstruction", "MedicationRequest.dosageInstruction"),
    ] {
        let text = edited(|request| {
            request.remove(key);
        })?;
        let refused = Prescription::read(&[&text]);
        assert!(
            matches!(refused, Err(ReadError::Missing { element: found }) if found == element),
            "{key}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_malformed_remaining_dispensations_extension_is_refused() -> Result<(), Box<dyn Error>> {
    let wrong_type = edited(|request| {
        replace_remaining(
            request,
            vec![remaining(Value::String(String::from("two")), "valueString")],
        );
    })?;
    let twice = edited(|request| {
        replace_remaining(
            request,
            vec![
                remaining(Value::from(1_u32), "valueUnsignedInt"),
                remaining(Value::from(2_u32), "valueUnsignedInt"),
            ],
        );
    })?;
    for text in [wrong_type, twice] {
        let refused = Prescription::read(&[&text]);
        assert!(
            matches!(
                refused,
                Err(ReadError::Extension {
                    url: REMAINING_DISPENSATIONS
                })
            ),
            "{refused:?}"
        );
    }
    Ok(())
}

/// A remaining-dispensations extension whose value is `value` under `key`.
fn remaining(value: Value, key: &str) -> Value {
    let mut extension = Object::new();
    extension.insert(
        String::from("url"),
        Value::String(String::from(REMAINING_DISPENSATIONS)),
    );
    extension.insert(key.to_owned(), value);
    Value::Object(extension)
}

/// Replaces the remaining-dispensations extensions of `request` by
/// `extensions`.
fn replace_remaining(request: &mut Object, extensions: Vec<Value>) {
    if let Some(Value::Object(dispense)) = request.get_mut("dispenseRequest")
        && let Some(Value::Array(held)) = dispense.get_mut("extension")
    {
        held.retain(|extension| {
            extension.get("url").and_then(Value::as_str) != Some(REMAINING_DISPENSATIONS)
        });
        held.extend(extensions);
    }
}

#[test]
fn a_local_medication_reference_with_no_contained_medication_is_refused()
-> Result<(), Box<dyn Error>> {
    let text = edited(|request| {
        request.remove("medicationCodeableConcept");
        let mut local = Object::new();
        local.insert(
            String::from("reference"),
            Value::String(String::from("#absent")),
        );
        request.insert(String::from("medicationReference"), Value::Object(local));
    })?;
    assert!(matches!(
        Prescription::read(&[&text]),
        Err(ReadError::Dangling {
            element: "MedicationRequest.medication[x]"
        })
    ));
    Ok(())
}

#[test]
fn two_notes_are_refused() -> Result<(), Box<dyn Error>> {
    let text = edited(|request| {
        if let Some(Value::Array(notes)) = request.get_mut("note")
            && let Some(first) = notes.first().cloned()
        {
            notes.push(first);
        }
    })?;
    assert!(matches!(
        Prescription::read(&[&text]),
        Err(ReadError::Repeated {
            element: "MedicationRequest.note"
        })
    ));
    Ok(())
}

#[test]
fn requests_of_one_prescription_must_agree_on_the_header() -> Result<(), Box<dyn Error>> {
    let written = prescription(
        &["RX-0006"],
        vec![
            item("RX-0006-1", "product-1", None),
            item("RX-0006-2", "product-2", None),
        ],
    );
    let mut texts = texts(&written)?;
    let second = texts.pop().ok_or("two requests")?;
    let other_patient = second.replace("Patient/example-patient", "Patient/example-other");
    texts.push(other_patient);
    assert!(matches!(
        read(&texts),
        Err(ReadError::Disagree {
            element: "MedicationRequest.subject"
        })
    ));
    Ok(())
}

#[test]
fn several_requests_without_a_group_are_refused() -> Result<(), Box<dyn Error>> {
    let first = texts(&single(None))?;
    let second = texts(&prescription(
        &["RX-0007"],
        vec![item("RX-0007", "product-2", None)],
    ))?;
    let both: Vec<String> = first.into_iter().chain(second).collect();
    assert!(matches!(
        read(&both),
        Err(ReadError::Missing {
            element: "MedicationRequest.groupIdentifier"
        })
    ));
    Ok(())
}

#[test]
fn a_prescription_the_profile_cannot_carry_is_not_written() {
    assert!(matches!(
        prescription(&["RX-0008"], Vec::new()).requests(),
        Err(WriteError::Empty {
            element: "EHDSMedicationPrescription.prescriptionItem"
        })
    ));
    let two_identifiers = prescription(
        &["RX-0009", "RX-0009-B"],
        vec![
            item("RX-0009-1", "product-1", None),
            item("RX-0009-2", "product-2", None),
        ],
    );
    assert!(matches!(
        two_identifiers.requests(),
        Err(WriteError::Unrepresentable {
            element: "EHDSMedicationPrescription.header.identifier"
        })
    ));
    let mut no_dosage = single(None);
    for item in &mut no_dosage.items {
        item.dosage.clear();
    }
    assert!(matches!(
        no_dosage.requests(),
        Err(WriteError::Empty {
            element: "EHDSMedicationPrescription.prescriptionItem.dosageInstructions"
        })
    ));
}
