// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each prescription the face serves, held to the MyHealth@EU ePrescription
//! profile (MyHealth@EU Requirements Catalogue 10.0.0 §1.14.1 and §1.14.2).
//!
//! The eHealth Network ePrescription guidelines 3.1 Art 10(5) leave the
//! cardinality of the dataset to the implementation guides and ask that an
//! element with no data be communicated as such; the MyHealth@EU
//! ePrescription and eDispensation guide (`myhealth.eu.fhir.epd` 1.0.0-poc)
//! fixes it in `MedicationRequest-myhealtheu-epd`. [`held`] checks a
//! prescription against that profile, and asks of it its narrative (FHIR R4
//! `Resource.text`) and the language it is written in (`Resource.language`),
//! the copy of the original a pharmacist receives beside the translation
//! (guidelines Art 3, process steps). [`hold`] keeps, of a page, the
//! prescriptions that pass and says why each other is withheld, never
//! quoting a value. [`reasoned`] completes a prescription the gateway maps
//! itself: a FHIR R4 `data-absent-reason` of `unknown` on each element the
//! profile requires that the order does not carry, the order's language, and
//! a narrative of the order's own words.

use std::io::Cursor;
use std::sync::LazyLock;

use eehrxf::dataset::{DatasetError, ResourceProfile};
use eehrxf::receive::conform::{CheckError, check_resource};
use fhir_types::codec::{Object, Value};

use super::page::Page;

/// The canonical URL of the MyHealth@EU ePrescription `MedicationRequest`
/// profile.
pub const EPD_MEDICATION_REQUEST: &str =
    "http://fhir.ehdsi.eu/epd/StructureDefinition/MedicationRequest-myhealtheu-epd";

/// The FHIR R4 extension that says why an element has no value.
pub const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// The profile [`held`] checks a prescription against, read once from the
/// vendored package.
static EPD: LazyLock<Result<ResourceProfile, DatasetError>> = LazyLock::new(|| {
    ResourceProfile::read(
        Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/specs/ehdsi-epd/myhealth.eu.fhir.epd-1.0.0-poc.tgz"
        ))),
        EPD_MEDICATION_REQUEST,
    )
});

/// Why a prescription is not served.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unheld {
    /// The profile cannot be read from the package this build carries.
    #[error("the profile {EPD_MEDICATION_REQUEST} cannot be read")]
    Profile(#[source] &'static DatasetError),
    /// The resource is no JSON object.
    #[error("the prescription is no JSON object")]
    NotObject,
    /// The resource breaks the profile.
    #[error("it breaks MedicationRequest-myhealtheu-epd: {0}")]
    Profiled(#[source] CheckError),
    /// The resource carries no narrative.
    #[error("it carries no narrative (text.status and text.div), the copy of the original")]
    NoNarrative,
    /// The resource names no language.
    #[error("it names no language (language) its original is written in")]
    NoLanguage,
}

/// Holds `prescription`, a `MedicationRequest`, to
/// `MedicationRequest-myhealtheu-epd`, with a narrative and a language.
///
/// # Errors
///
/// The [`Unheld`] that says why the prescription is not served.
pub fn held(prescription: &Value) -> Result<(), Unheld> {
    let profile = EPD.as_ref().map_err(Unheld::Profile)?;
    let Value::Object(resource) = prescription else {
        return Err(Unheld::NotObject);
    };
    check_resource(profile, "MedicationRequest", resource).map_err(Unheld::Profiled)?;
    let narrated = resource
        .get("text")
        .is_some_and(|text| filled(text.get("status")) && filled(text.get("div")));
    if !narrated {
        return Err(Unheld::NoNarrative);
    }
    if !filled(resource.get("language")) {
        return Err(Unheld::NoLanguage);
    }
    Ok(())
}

/// Whether `value` is a string with more than white space.
fn filled(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

/// Keeps, of `page`, the prescriptions [`held`] passes, and records in
/// [`Page::withheld`] why each other was withheld, in the page's order.
pub fn hold(page: &mut Page) {
    let mut withheld = Vec::new();
    page.prescriptions
        .retain(|prescription| match held(&prescription.kept.resource) {
            Ok(()) => true,
            Err(unheld) => {
                withheld.push(unheld.to_string());
                false
            }
        });
    page.withheld.extend(withheld);
}

/// Completes `prescription`, a `MedicationRequest` the gateway mapped from
/// an order written in `language`, an ISO 639-1 code, as [`held`] asks.
///
/// Each element `MedicationRequest-myhealtheu-epd` requires that the
/// mapping cannot always fill, `identifier`, `authoredOn` and `requester`,
/// carries the `data-absent-reason` `unknown` when the order gave it none;
/// the language is the order's when the mapping wrote none; and a narrative
/// of status `generated` holds the order's own texts when the mapping wrote
/// none. An element the order carries is left as it is.
pub fn reasoned(prescription: &mut Value, language: Option<&str>) {
    let Value::Object(resource) = prescription else {
        return;
    };
    // NOTE: FHIR R4 extension-data-absent-reason, code `unknown`: the order gives no value, and the
    // gateway knows no reason (eHealth Network ePrescription guidelines 3.1 Art 10(5)).
    if absent(resource, "identifier") {
        resource.insert("identifier".to_owned(), Value::Array(vec![absent_reason()]));
    }
    if absent(resource, "authoredOn") && absent(resource, "_authoredOn") {
        resource.insert("_authoredOn".to_owned(), absent_reason());
    }
    if absent(resource, "requester") {
        resource.insert("requester".to_owned(), absent_reason());
    }
    if let Some(language) = language.filter(|language| !language.trim().is_empty())
        && absent(resource, "language")
    {
        resource.insert("language".to_owned(), Value::String(language.to_owned()));
    }
    if absent(resource, "text") {
        let texts = texts(resource);
        if !texts.is_empty() {
            let mut paragraphs = String::new();
            for text in &texts {
                paragraphs.push_str("<p>");
                paragraphs.push_str(&escaped(text));
                paragraphs.push_str("</p>");
            }
            let mut text = Object::new();
            text.insert("status".to_owned(), Value::String("generated".to_owned()));
            text.insert(
                "div".to_owned(),
                Value::String(format!(
                    "<div xmlns=\"http://www.w3.org/1999/xhtml\">{paragraphs}</div>"
                )),
            );
            resource.insert("text".to_owned(), Value::Object(text));
        }
    }
}

/// Whether `resource` carries no value under `name`.
fn absent(resource: &Object, name: &str) -> bool {
    match resource.get(name) {
        None | Some(Value::Null) => true,
        Some(Value::Array(held)) => held.is_empty(),
        Some(_) => false,
    }
}

/// An element that carries no value, only the `data-absent-reason`
/// `unknown`.
fn absent_reason() -> Value {
    let mut extension = Object::new();
    extension.insert(
        "url".to_owned(),
        Value::String(DATA_ABSENT_REASON.to_owned()),
    );
    extension.insert("valueCode".to_owned(), Value::String("unknown".to_owned()));
    let mut element = Object::new();
    element.insert(
        "extension".to_owned(),
        Value::Array(vec![Value::Object(extension)]),
    );
    Value::Object(element)
}

/// The order's own texts in `resource`, in the order a reader takes them:
/// the medicinal product, each dosage instruction, and each note.
fn texts(resource: &Object) -> Vec<String> {
    let mut texts = Vec::new();
    if let Some(product) = resource.get("medicationCodeableConcept") {
        let shown = product.get("text").and_then(Value::as_str).or_else(|| {
            product
                .get("coding")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .find_map(|coding| coding.get("display").and_then(Value::as_str))
        });
        texts.extend(shown.map(str::to_owned));
    }
    for (name, field) in [("dosageInstruction", "text"), ("note", "text")] {
        let held = resource
            .get(name)
            .and_then(Value::as_array)
            .unwrap_or_default();
        texts.extend(
            held.iter()
                .filter_map(|item| item.get(field).and_then(Value::as_str))
                .map(str::to_owned),
        );
    }
    texts.retain(|text| !text.trim().is_empty());
    texts
}

/// `text` escaped for XHTML character data (XML 1.0 §2.4).
fn escaped(text: &str) -> String {
    let mut written = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => written.push_str("&amp;"),
            '<' => written.push_str("&lt;"),
            '>' => written.push_str("&gt;"),
            '"' => written.push_str("&quot;"),
            other => written.push(other),
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Value;

    use super::{Unheld, held, reasoned};

    /// A prescription with every element the profile requires, its
    /// narrative and its language.
    const SERVED: &str = r#"{"resourceType":"MedicationRequest","language":"nl","text":{"status":"generated","div":"<div xmlns=\"http://www.w3.org/1999/xhtml\"><p>Synthetisch middel A</p></div>"},"identifier":[{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-1"}],"status":"active","intent":"order","medicationCodeableConcept":{"text":"Synthetisch middel A"},"subject":{"reference":"Patient/p-a"},"authoredOn":"2026-11-01","requester":{"display":"Synthetic prescriber"}}"#;

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_complete_prescription_is_served() {
        held(&parsed(SERVED)).unwrap();
    }

    #[test]
    fn a_prescription_with_no_requester_breaks_the_profile() {
        let unheld = held(&parsed(
            &SERVED.replace(r#","requester":{"display":"Synthetic prescriber"}"#, ""),
        ))
        .unwrap_err();
        assert!(matches!(unheld, Unheld::Profiled(_)), "{unheld}");
        assert!(unheld.to_string().contains("requester"), "{unheld}");
        assert!(
            !unheld.to_string().contains("SYNTHETIC"),
            "no value: {unheld}"
        );
    }

    #[test]
    fn a_prescription_with_no_narrative_or_language_is_not_served() {
        let narrative = SERVED
            .split(r#""text":"#)
            .next()
            .map(|head| format!("{head}{}", SERVED.split_once(r#"</div>"},"#).unwrap().1))
            .unwrap();
        assert!(matches!(
            held(&parsed(&narrative)),
            Err(Unheld::NoNarrative)
        ));
        let language = SERVED.replace(r#""language":"nl","#, "");
        assert!(matches!(held(&parsed(&language)), Err(Unheld::NoLanguage)));
    }

    #[test]
    fn a_mapped_prescription_is_completed_with_reasons_language_and_narrative() {
        let mut mapped = parsed(
            r#"{"resourceType":"MedicationRequest","status":"active","intent":"order","medicationCodeableConcept":{"text":"Middel <A> & B"},"subject":{"identifier":{"system":"urn:oid:2.999.1","value":"p-1"}},"dosageInstruction":[{"text":"1 tablet per dag"}]}"#,
        );
        reasoned(&mut mapped, Some("nl"));
        held(&mapped).unwrap();
        let reason = |element: &Value| {
            element
                .get("extension")
                .and_then(Value::as_array)
                .and_then(|extensions| extensions.first())
                .and_then(|extension| extension.get("valueCode"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let identifier = mapped
            .get("identifier")
            .and_then(Value::as_array)
            .and_then(|held| held.first())
            .unwrap();
        assert_eq!(Some("unknown".to_owned()), reason(identifier));
        assert_eq!(
            Some("unknown".to_owned()),
            reason(mapped.get("_authoredOn").unwrap())
        );
        assert_eq!(
            Some("unknown".to_owned()),
            reason(mapped.get("requester").unwrap())
        );
        assert_eq!(Some("nl"), mapped.get("language").and_then(Value::as_str));
        assert_eq!(
            Some(
                "<div xmlns=\"http://www.w3.org/1999/xhtml\"><p>Middel &lt;A&gt; &amp; B</p><p>1 tablet per dag</p></div>"
            ),
            mapped
                .get("text")
                .and_then(|text| text.get("div"))
                .and_then(Value::as_str)
        );
    }
}
