// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic dispensation read from and written as
//! `MedicationDispense-eu-mpd`, held to the vendored MPD profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::medication::Medicinal;
use eehrxf::medication::ReadError;
use eehrxf::medication::WriteError;
use eehrxf::medication::dispensation::Dispense;
use eehrxf::medication::dispensation::RECORDED;
use eehrxf::receive::conform::FindingKind;
use eehrxf::receive::conform::Unread;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::medication::Medication;
use fhir_types::r4::reference::Reference;

use super::fixtures::coded;
use super::fixtures::examples;
use super::fixtures::findings;
use super::fixtures::identifier;
use super::fixtures::object;
use super::fixtures::profiles;
use super::fixtures::quantity;
use super::fixtures::reference;

/// A synthetic dispense against the prescription item `RX-0001`.
fn dispense() -> Dispense {
    Dispense {
        subject: reference("Patient/example-patient"),
        identifier: vec![identifier("DX-0001")],
        author: vec![reference("PractitionerRole/example-pharmacist")],
        date: String::from("2026-02-01T10:00:00Z"),
        status: String::from("completed"),
        language: None,
        location: Some(reference("Location/example-pharmacy")),
        receiver: None,
        related_request: vec![Reference {
            identifier: Some(Box::new(identifier("RX-0001"))),
            ..Reference::default()
        }],
        medication: Medicinal::Coded(coded("product-1")),
        quantity: quantity("30"),
        time_of_dispensation: Some(String::from("2026-02-01T10:05:00Z")),
        substitution_occurred: Some(false),
        dosage: Vec::new(),
        note: None,
    }
}

/// The synthetic dispense written as JSON text, edited by `edit`.
fn edited(edit: impl Fn(&mut Object)) -> Result<String, Box<dyn Error>> {
    let mut written = dispense().to_json()?;
    edit(&mut written);
    Ok(serde_json::to_string(&Value::Object(written))?)
}

#[test]
fn a_written_dispense_conforms_to_mpd() -> Result<(), Box<dyn Error>> {
    profiles()?.check_dispense(&dispense().to_json()?)?;
    Ok(())
}

// NOTE: FHIR R4 conformance-rules.html §2.1.0.6: an invariant "must evaluate to true" on
// each occurrence, so one the check does not read is listed, on every element in play.
#[test]
fn the_invariants_of_every_element_in_play_are_listed_as_not_evaluated()
-> Result<(), Box<dyn Error>> {
    let conformance = profiles()?.check_dispense(&dispense().to_json()?)?;
    let listed = |element: &str, key: &str| {
        conformance.unevaluated().iter().any(|unevaluated| {
            unevaluated.element == element
                && unevaluated.reason == Unread::Invariant(key.to_owned())
        })
    };
    assert!(
        listed("MedicationDispense.status", "ele-1"),
        "an invariant of a present element below the root: {:#?}",
        conformance.unevaluated()
    );
    assert!(
        !listed("MedicationDispense.receiver", "ele-1"),
        "an element the dispense does not carry has no occurrence to hold"
    );
    Ok(())
}

#[test]
fn a_dispense_that_breaks_an_invariant_is_answered_with_it_listed() -> Result<(), Box<dyn Error>> {
    let text = edited(|dispense| {
        dispense.insert(
            String::from("whenPrepared"),
            Value::String(String::from("2026-02-02T10:00:00Z")),
        );
    })?;
    let conformance = profiles()?.check_dispense(&object(&text)?)?;
    assert!(
        conformance.unevaluated().iter().any(|unevaluated| {
            unevaluated.element == "MedicationDispense"
                && unevaluated.reason == Unread::Invariant(String::from("mdd-1"))
        }),
        "mdd-1, which the dispense breaks, is named as not evaluated, never passed: {:#?}",
        conformance.unevaluated()
    );
    Ok(())
}

#[test]
fn a_status_outside_its_required_value_set_is_answered_with_the_binding_listed()
-> Result<(), Box<dyn Error>> {
    let text = edited(|dispense| {
        dispense.insert(
            String::from("status"),
            Value::String(String::from("synthetic-not-a-status")),
        );
    })?;
    let conformance = profiles()?.check_dispense(&object(&text)?)?;
    assert!(
        conformance.unevaluated().iter().any(|unevaluated| {
            unevaluated.element == "MedicationDispense.status"
                && matches!(unevaluated.reason, Unread::Binding { .. })
        }),
        "the binding the status breaks is named as not evaluated, never passed: {:#?}",
        conformance.unevaluated()
    );
    Ok(())
}

#[test]
fn every_required_binding_is_listed_as_not_evaluated() -> Result<(), Box<dyn Error>> {
    let conformance = profiles()?.check_dispense(&dispense().to_json()?)?;
    assert!(
        conformance.unevaluated().iter().any(|unevaluated| {
            unevaluated.element == "MedicationDispense.status"
                && matches!(unevaluated.reason, Unread::Binding { .. })
        }),
        "the required binding of the status, which no expansion decides: {:#?}",
        conformance.unevaluated()
    );
    Ok(())
}

#[test]
fn a_dispense_round_trips() -> Result<(), Box<dyn Error>> {
    let written = dispense();
    let text = serde_json::to_string(&Value::Object(written.to_json()?))?;
    assert_eq!(Dispense::read(&text)?, written);
    Ok(())
}

#[test]
fn a_dispense_with_a_contained_medication_round_trips_and_conforms() -> Result<(), Box<dyn Error>> {
    let mut written = dispense();
    written.medication = Medicinal::Contained {
        id: String::from("med-1"),
        medication: Box::new(Medication {
            code: Some(coded("product-1")),
            ..Medication::default()
        }),
    };
    let json = written.to_json()?;
    profiles()?.check_dispense(&json)?;
    let text = serde_json::to_string(&Value::Object(json))?;
    assert_eq!(Dispense::read(&text)?, written);
    Ok(())
}

#[test]
fn a_member_dispense_reads_and_writes_back_the_same_json() -> Result<(), Box<dyn Error>> {
    let text = r#"{"resourceType":"MedicationDispense","meta":{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd"]},"extension":[{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-03-01T09:00:00Z"}],"status":"declined","medicationCodeableConcept":{"coding":[{"system":"urn:oid:2.999.1.7.2","code":"product-2"}]},"subject":{"reference":"Patient/example-patient"},"performer":[{"actor":{"reference":"PractitionerRole/example-pharmacist"}}],"authorizingPrescription":[{"identifier":{"system":"urn:oid:2.999.1.7.1","value":"RX-0002"}}],"quantity":{"value":0,"system":"http://unitsofmeasure.org","code":"1"}}"#;
    let read = Dispense::read(text)?;
    assert_eq!(read.status, "declined");
    assert_eq!(
        read.related_request
            .first()
            .and_then(|request| request.identifier.as_deref())
            .and_then(|held: &Identifier| held.value.as_ref())
            .and_then(|value| value.value.as_deref()),
        Some("RX-0002")
    );
    assert_eq!(read.to_json()?, object(text)?);
    Ok(())
}

#[test]
fn every_mpd_dispense_example_conforms_and_reads() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let texts = examples("MedicationDispense-")?;
    assert_eq!(texts.len(), 4);
    for text in &texts {
        mpd.check_dispense(&object(text)?)?;
        Dispense::read(text)?;
    }
    Ok(())
}

#[test]
fn a_dispense_without_the_recorded_extension_breaks_the_profile() -> Result<(), Box<dyn Error>> {
    let mut written = dispense().to_json()?;
    written.remove("extension");
    let found = findings(profiles()?.check_dispense(&written))?;
    assert!(
        found
            .iter()
            .any(|finding| finding.element == "MedicationDispense.extension"
                && matches!(finding.kind, FindingKind::TooFew { min: 1, found: 0 })),
        "{found:?}"
    );
    let text = serde_json::to_string(&Value::Object(written))?;
    assert!(matches!(
        Dispense::read(&text),
        Err(ReadError::Missing {
            element: "MedicationDispense.extension:recorded"
        })
    ));
    Ok(())
}

#[test]
fn a_recorded_extension_of_the_wrong_type_is_refused() -> Result<(), Box<dyn Error>> {
    let text = edited(|written| {
        let mut extension = Object::new();
        extension.insert(String::from("url"), Value::String(String::from(RECORDED)));
        extension.insert(
            String::from("valueString"),
            Value::String(String::from("yesterday")),
        );
        written.insert(
            String::from("extension"),
            Value::Array(vec![Value::Object(extension)]),
        );
    })?;
    assert!(matches!(
        Dispense::read(&text),
        Err(ReadError::Extension { url: RECORDED })
    ));
    Ok(())
}

#[test]
fn a_dispense_missing_a_dataset_element_is_refused() -> Result<(), Box<dyn Error>> {
    for (key, element) in [
        ("subject", "MedicationDispense.subject"),
        ("performer", "MedicationDispense.performer"),
        ("quantity", "MedicationDispense.quantity"),
    ] {
        let text = edited(|written| {
            written.remove(key);
        })?;
        let refused = Dispense::read(&text);
        assert!(
            matches!(refused, Err(ReadError::Missing { element: found }) if found == element),
            "{key}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn two_receivers_are_refused() -> Result<(), Box<dyn Error>> {
    let text = edited(|written| {
        let receiver = |text: &str| {
            let mut object = Object::new();
            object.insert(String::from("reference"), Value::String(text.to_owned()));
            Value::Object(object)
        };
        written.insert(
            String::from("receiver"),
            Value::Array(vec![
                receiver("RelatedPerson/example-1"),
                receiver("RelatedPerson/example-2"),
            ]),
        );
    })?;
    assert!(matches!(
        Dispense::read(&text),
        Err(ReadError::Repeated {
            element: "MedicationDispense.receiver"
        })
    ));
    Ok(())
}

#[test]
fn a_medication_request_is_no_dispense() {
    assert!(matches!(
        Dispense::read(r#"{"resourceType":"MedicationRequest"}"#),
        Err(ReadError::WrongType {
            expected: "MedicationDispense"
        })
    ));
}

#[test]
fn a_dispense_with_no_author_is_not_written() {
    let mut written = dispense();
    written.author.clear();
    assert!(matches!(
        written.resource(),
        Err(WriteError::Empty {
            element: "EHDSMedicationDispense.header.author[x]"
        })
    ));
}
