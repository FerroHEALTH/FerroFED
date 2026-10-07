// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document received in the exchange format: read, checked against the
//! vendored HL7 Europe Patient Summary profiles, and mapped into openEHR.
//!
//! The document is synthetic: every identifier sits under `example.org`
//! (RFC 6761 §6.5), a UUID of our own or the `2.999` example arc, and no
//! value is clinical.

#[cfg(feature = "patient-summary")]
mod category;
mod conform;
mod faithful;
#[cfg(feature = "openehr")]
mod openehr;
mod read;
mod reference;
mod subjects;

use std::error::Error;
use std::io::Read;

use flate2::read::GzDecoder;

use crate::support::eps_package;

/// The patient identifier of the synthetic document, which no refusal or
/// finding may quote.
pub(crate) const PATIENT_IDENTIFIER: &str = "synthetic-0665";

/// The canonical URL of the EPS Bundle profile.
pub(crate) const BUNDLE_PROFILE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The canonical URL of the EPS Composition profile.
pub(crate) const COMPOSITION_PROFILE: &str =
    "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// A synthetic patient summary: the five sections the EPS Composition
/// profile requires, one allergy that claims the synthetic mapping's profile,
/// and the patient.
pub(crate) const DOCUMENT: &str = r#"{
  "resourceType": "Bundle",
  "identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000665" },
  "type": "document",
  "timestamp": "2026-10-06T10:00:00Z",
  "entry": [
    {
      "fullUrl": "http://example.org/fhir/Composition/synthetic-composition",
      "resource": {
        "resourceType": "Composition",
        "id": "synthetic-composition",
        "meta": { "profile": ["http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"] },
        "identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000666" },
        "status": "final",
        "type": { "coding": [{ "system": "http://loinc.org", "code": "60591-5" }] },
        "subject": { "reference": "Patient/synthetic-patient" },
        "date": "2026-10-06T10:00:00Z",
        "author": [{ "display": "Synthetic author" }],
        "title": "Synthetic patient summary",
        "section": [
          {
            "title": "Problems",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "11450-4" }] },
            "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No problems recorded</div>" }
          },
          {
            "title": "Allergies",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "48765-2" }] },
            "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">Synthetic substance one</div>" },
            "entry": [{ "reference": "AllergyIntolerance/synthetic-allergy" }]
          },
          {
            "title": "Medication",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "10160-0" }] },
            "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No medication recorded</div>" }
          },
          {
            "title": "Procedures",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "47519-4" }] },
            "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No procedures recorded</div>" }
          },
          {
            "title": "Medical devices",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "46264-8" }] },
            "text": { "status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">No devices recorded</div>" }
          }
        ]
      }
    },
    {
      "fullUrl": "http://example.org/fhir/Patient/synthetic-patient",
      "resource": {
        "resourceType": "Patient",
        "id": "synthetic-patient",
        "identifier": [{ "system": "urn:oid:2.999.1.665", "value": "synthetic-0665" }]
      }
    },
    {
      "fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy",
      "resource": {
        "resourceType": "AllergyIntolerance",
        "id": "synthetic-allergy",
        "meta": { "profile": ["http://example.org/fhir/StructureDefinition/ferrofed-allergy"] },
        "code": { "text": "Synthetic substance one" },
        "patient": { "reference": "Patient/synthetic-patient" }
      }
    }
  ]
}"#;

/// Returns the text of the example `file` the vendored EPS package carries
/// under `package/example/`.
pub(crate) fn eps_example(file: &str) -> Result<String, Box<dyn Error>> {
    let wanted = format!("package/example/{file}");
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(eps_package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == wanted {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(text);
        }
    }
    Err(format!("the EPS package carries no {wanted}").into())
}

/// Returns the synthetic document with `entry`, one `Bundle.entry` object,
/// added as its last entry.
pub(crate) fn document_with_entry(entry: &str) -> Result<String, String> {
    let end = DOCUMENT.rfind("\n  ]\n}").ok_or("the end of the entries")?;
    let head = DOCUMENT.get(..end).ok_or("the entries")?;
    Ok(format!("{head},\n    {entry}\n  ]\n}}"))
}

/// Returns the synthetic document with `from` replaced by `to`, failing
/// when `from` is not in it exactly once.
pub(crate) fn document_with(from: &str, to: &str) -> Result<String, String> {
    match DOCUMENT.matches(from).count() {
        1 => Ok(DOCUMENT.replacen(from, to, 1)),
        count => Err(format!("{from:?} occurs {count} times in the document")),
    }
}
