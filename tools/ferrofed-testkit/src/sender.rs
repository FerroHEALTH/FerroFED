// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic system that sends a document the gateway receives: its
//! named patient, its `Device` and `Organization` as the document's
//! authors, and the displays it writes for its codes. No specification
//! governs the harness: our own design.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the synthetic resources are written as JSON"
)]

use std::error::Error;

use eehrxf::document::Document;
use fhir_types::codec::{Json, Path as CodecPath, Value as CodecValue};
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::organization::Organization;
use fhir_types::r4::resource::Resource;
use serde_json::Value;

/// The base the sender names its entries under, in the `example.org`
/// domain RFC 6761 §6.5 reserves.
pub(crate) const BASE: &str = "https://sender.example.org/fhir";

/// Decodes the FHIR JSON `value` as the R4 resource of `kind`.
pub(crate) fn resource(kind: &str, value: &Value) -> Result<Resource, Box<dyn Error>> {
    let CodecValue::Object(object) = serde_json::from_str::<CodecValue>(&value.to_string())? else {
        return Err(format!("the synthetic {kind} is no JSON object").into());
    };
    Ok(Resource::from_json(&object, &mut CodecPath::root(kind))?)
}

/// Names in `document` the synthetic patient of `identifiers`, each a
/// `(system, value)`, and authors it by the sender's `Device` named
/// `system` and its `Organization` `(id, name)`; returns the `fullUrl`s of
/// the patient and the device.
pub(crate) fn authored(
    document: &mut Document,
    identifiers: &[(&str, &str)],
    (system, (id, name)): (&str, (&str, &str)),
) -> Result<(String, String), Box<dyn Error>> {
    let identifier: Vec<Value> = identifiers
        .iter()
        .map(|(system, value)| serde_json::json!({ "system": system, "value": value }))
        .collect();
    let Resource::Patient(patient) = resource(
        "Patient",
        &serde_json::json!({
            "resourceType": "Patient",
            "id": "synthetic-patient",
            "identifier": identifier,
            "name": [{ "family": "Synthetic" }],
            "birthDate": "1970-01-01"
        }),
    )?
    else {
        return Err("the synthetic patient is no Patient".into());
    };
    let subject = document.with_patient(*patient)?;
    let device = document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("sender")),
        device_name: vec![DeviceDeviceName {
            name: system.into(),
            r#type: "user-friendly-name".into(),
            ..DeviceDeviceName::default()
        }],
        ..Device::default()
    })))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(id.to_owned()),
        name: Some(name.into()),
        ..Organization::default()
    })))?;
    Ok((subject, device))
}

/// Gives every coding in `value` that names no display the one `displays`
/// holds for it, each `(system, code, display)`, as a sender that names its
/// codes does: the component's builder writes some codings with no display,
/// and the receive path refuses a coding with none when no terminology
/// server can look one up.
pub(crate) fn displayed(value: &mut Value, displays: &[(&str, &str, &str)]) {
    match value {
        Value::Object(object) => {
            let named = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
            if let (Some(system), Some(code), None) =
                (named("system"), named("code"), object.get("display"))
                && let Some((_, _, display)) = displays
                    .iter()
                    .find(|(known, held, _)| *known == system && *held == code)
            {
                object.insert(String::from("display"), Value::from(*display));
            }
            object
                .values_mut()
                .for_each(|member| displayed(member, displays));
        }
        Value::Array(items) => items.iter_mut().for_each(|item| displayed(item, displays)),
        _ => {}
    }
}
