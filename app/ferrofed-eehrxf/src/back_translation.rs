// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received dispensation's codes translated back into the code systems
//! the member that owns its prescription records.
//!
//! A national contact point sends a dispensation coded in the systems the
//! MyHealth@EU Master Value Sets Catalogue draws from, which the owner may
//! not use. Implementing Regulation (EU) 2026/2083 Art 3(4) and (5) give the
//! mapping and translation of codes to the Member State, whose national
//! infrastructure the gateway is part of. Each `CodeableConcept` of the
//! `MedicationDispense` and of each `Medication` it contains that carries a
//! coding, and none in the owner's systems, is asked of the terminology
//! server's `ConceptMap/$translate` into each of them through eehrxf's
//! [`Translator`], which adds only an `equivalent` or `equal` match, after
//! the recorded codings. A concept no such match reaches refuses the
//! dispensation, named by its element path and its own code systems, never
//! by a code. With nothing added the text travels as it came. Which concepts
//! must reach the owner's systems is our own design: no specification
//! governs it.

use eehrxf::terminology::Terminology;
use eehrxf::terminology::translation::{TranslationError, Translator};
use fhir_types::codec::{DecodeError, EncodeError, Json as _, Path, Value};
use fhir_types::r4::medication_dispense::MedicationDispense;
use fhir_types::r4::resource::Resource;

use crate::dispensations::RESOURCE;

/// Why a received dispensation's codes are not translated; no variant's
/// text quotes a code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Untranslated {
    /// The text is no `MedicationDispense` in FHIR JSON.
    #[error("the dispensation is no FHIR R4 MedicationDispense in JSON")]
    Text,
    /// The text does not decode against FHIR R4.
    #[error("the dispensation does not decode against FHIR R4")]
    Decode(#[source] DecodeError),
    /// The translated dispensation does not encode.
    #[error("the translated dispensation does not encode")]
    Encode(#[source] EncodeError),
    /// The translated dispensation cannot be written as JSON text.
    #[error("the translated dispensation cannot be written as JSON text")]
    Write(#[source] serde_json::Error),
    /// The terminology server refused or did not answer a translation.
    #[error("the terminology server gave no translation")]
    Terminology(#[source] TranslationError),
    /// A concept carries no coding in the owner's systems, and no exact
    /// translation brings it into one.
    #[error(
        "{location} carries no code in a system the member records, and the terminology server maps none of its codes in {} to one exactly",
        systems.join(", ")
    )]
    Unreached {
        /// Where the concept is, as an element path.
        location: String,
        /// The code systems the concept is recorded in.
        systems: Vec<String>,
    },
}

/// `text`, a received `MedicationDispense`, with each concept translated
/// into `systems`, the code systems the owner records, through
/// `terminology`; `None` when no coding was added, so the text travels as
/// it came.
///
/// # Errors
///
/// [`Untranslated::Unreached`] for the first concept no exact translation
/// brings into `systems`, [`Untranslated::Terminology`] when the server
/// refuses or does not answer, and the other variants for a text that does
/// not read or a result that does not write.
pub async fn translated(
    text: &str,
    terminology: &Terminology,
    systems: &[&str],
) -> Result<Option<String>, Untranslated> {
    let value: Value = serde_json::from_str(text).map_err(|_unparsed| Untranslated::Text)?;
    let Value::Object(object) = value else {
        return Err(Untranslated::Text);
    };
    let dispense = MedicationDispense::from_json(&object, &mut Path::root(RESOURCE))
        .map_err(Untranslated::Decode)?;
    let held = |system: &str| systems.contains(&system);
    let mut translator = Translator::new(terminology, systems);
    let translated = translator
        .translate(&Resource::MedicationDispense(Box::new(dispense)), held)
        .await
        .map_err(Untranslated::Terminology)?;
    let Resource::MedicationDispense(mut dispense) = translated else {
        return Err(Untranslated::Text);
    };
    for contained in &mut dispense.contained {
        if matches!(contained, Resource::Medication(_)) {
            *contained = translator
                .translate(contained, held)
                .await
                .map_err(Untranslated::Terminology)?;
        }
    }
    let written = dispense.to_json().map_err(Untranslated::Encode)?;
    for (key, member) in &written {
        match key.as_str() {
            "meta" => {}
            "contained" => {
                for (index, contained) in member.as_array().unwrap_or_default().iter().enumerate() {
                    if contained.get("resourceType").and_then(Value::as_str) == Some("Medication") {
                        unreached(contained, &format!("{RESOURCE}.contained[{index}]"), &held)?;
                    }
                }
            }
            _ => unreached(member, &format!("{RESOURCE}.{key}"), &held)?,
        }
    }
    if translator.mapped().is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&Resource::MedicationDispense(dispense))
        .map(Some)
        .map_err(Untranslated::Write)
}

/// Holds every concept in `value`, at `location`, to carry a coding whose
/// system `held` admits, or no coding with a system and a code at all.
///
/// # Errors
///
/// [`Untranslated::Unreached`] for the first concept that does not.
fn unreached(
    value: &Value,
    location: &str,
    held: &impl Fn(&str) -> bool,
) -> Result<(), Untranslated> {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                unreached(item, &format!("{location}[{index}]"), held)?;
            }
        }
        Value::Object(object) => {
            if let Some(codings) = object.get("coding").and_then(Value::as_array) {
                let mut systems: Vec<String> = codings
                    .iter()
                    .filter(|coding| coding.get("code").and_then(Value::as_str).is_some())
                    .filter_map(|coding| coding.get("system").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect();
                if !systems.is_empty() && !systems.iter().any(|system| held(system)) {
                    systems.sort();
                    systems.dedup();
                    return Err(Untranslated::Unreached {
                        location: location.to_owned(),
                        systems,
                    });
                }
            }
            for (key, member) in object {
                if key != "coding" {
                    unreached(member, &format!("{location}.{key}"), held)?;
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}
