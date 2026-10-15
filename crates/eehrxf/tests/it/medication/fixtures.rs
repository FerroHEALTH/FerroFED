// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic content and the readers the dataset suites share.

use std::error::Error;
use std::io::Read;

use eehrxf::medication::Mpd;
use eehrxf::receive::conform::CheckError;
use eehrxf::receive::conform::Finding;
use fhir_types::codec::Object;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::reference::Reference;
use flate2::read::GzDecoder;

use crate::support::mpd_package;

/// The synthetic identifier system of the prescriptions and dispenses, in
/// the `2.999` example arc.
pub(crate) const SYSTEM: &str = "urn:oid:2.999.1.7.1";

/// Reads the MPD profiles from the vendored package.
pub(crate) fn profiles() -> Result<Mpd, Box<dyn Error>> {
    Ok(Mpd::read(&std::fs::read(mpd_package())?)?)
}

/// The synthetic identifier `value` of [`SYSTEM`].
pub(crate) fn identifier(value: &str) -> Identifier {
    Identifier {
        system: Some(SYSTEM.into()),
        value: Some(value.into()),
        ..Identifier::default()
    }
}

/// The literal reference `text`.
pub(crate) fn reference(text: &str) -> Reference {
    Reference {
        reference: Some(text.into()),
        ..Reference::default()
    }
}

/// A synthetic code `code` of the example code system `2.999.1.7.2`.
pub(crate) fn coded(code: &str) -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding {
            system: Some("urn:oid:2.999.1.7.2".into()),
            code: Some(code.into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }
}

/// `value` units of the UCUM unit `1`.
pub(crate) fn quantity(value: &str) -> Quantity {
    Quantity {
        value: Some(value.into()),
        system: Some("http://unitsofmeasure.org".into()),
        code: Some("1".into()),
        ..Quantity::default()
    }
}

/// The findings of a check that must refuse.
pub(crate) fn findings<T: std::fmt::Debug>(
    checked: Result<T, CheckError>,
) -> Result<Vec<Finding>, Box<dyn Error>> {
    match checked {
        Ok(conformance) => Err(format!("the resource conforms: {conformance:?}").into()),
        Err(CheckError::NonConformant { findings }) => Ok(findings),
        Err(other) => Err(other.into()),
    }
}

/// The text of every example of the vendored MPD package whose file name
/// starts with `prefix`, in archive order.
pub(crate) fn examples(prefix: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(mpd_package())?));
    let mut texts = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        if path.starts_with(&format!("package/example/{prefix}")) {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            texts.push(text);
        }
    }
    Ok(texts)
}

/// Parses `text` as a FHIR JSON object.
pub(crate) fn object(text: &str) -> Result<Object, Box<dyn Error>> {
    match serde_json::from_str(text)? {
        fhir_types::codec::Value::Object(object) => Ok(object),
        _ => Err("the text is no JSON object".into()),
    }
}
