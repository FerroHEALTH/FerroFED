// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The structural check of a document against the vendored HL7 Europe
//! Patient Summary profiles, which answers every finding.

#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored profiles and the document are read as JSON"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde_json::Value;

use super::ROOT;

/// The JSON null a missing member reads as.
static NULL: Value = Value::Null;

/// Reads a JSON member or the first array item with no panicking index.
trait At {
    /// Returns the member `key`, or null.
    fn at(&self, key: &str) -> &Value;
    /// Returns the first item of an array, or null.
    fn first_item(&self) -> &Value;
}

impl At for Value {
    fn at(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }

    fn first_item(&self) -> &Value {
        self.get(0).unwrap_or(&NULL)
    }
}

/// Returns the path of the vendored HL7 Europe Patient Summary package.
#[must_use]
pub fn package() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz")
}

/// The snapshot elements of the profiles `names`, read in one pass over
/// the package, in the order of `names`.
fn snapshots<const N: usize>(names: [&str; N]) -> Result<[Vec<Value>; N], Box<dyn Error>> {
    let mut found: [Option<Vec<Value>>; N] = std::array::from_fn(|_| None);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let Some(slot) = names
            .iter()
            .position(|name| path == format!("package/StructureDefinition-{name}.json"))
        else {
            continue;
        };
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        let profile: Value = serde_json::from_str(&text)?;
        let elements = profile
            .at("snapshot")
            .at("element")
            .as_array()
            .cloned()
            .ok_or("a profile without a snapshot")?;
        if let Some(held) = found.get_mut(slot) {
            *held = Some(elements);
        }
    }
    let mut out: [Vec<Value>; N] = std::array::from_fn(|_| Vec::new());
    for (index, held) in found.into_iter().enumerate() {
        let elements = held.ok_or_else(|| format!("the package lacks {:?}", names.get(index)))?;
        if let Some(slot) = out.get_mut(index) {
            *slot = elements;
        }
    }
    Ok(out)
}

/// The required top-level members of a resource whose snapshot is
/// `elements`: every `{root}.{member}` with `min` past zero, no slice.
fn required(elements: &[Value], root: &str) -> Vec<(String, u64)> {
    elements
        .iter()
        .filter_map(|element| {
            let id = element.at("id").as_str()?;
            let member = id.strip_prefix(root)?.strip_prefix('.')?;
            let min = element.at("min").as_u64().unwrap_or(0);
            (min > 0 && !member.contains(['.', ':'])).then(|| (member.to_owned(), min))
        })
        .collect()
}

/// Whether `resource` carries `member`, a primitive with only an
/// extension included (`_member`).
fn carries(resource: &Value, member: &str) -> bool {
    let present = |value: &Value| match value {
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        _ => true,
    };
    present(resource.at(member)) || present(resource.at(&format!("_{member}")))
}

/// How many times `resource` carries `member`.
fn count(resource: &Value, member: &str) -> u64 {
    match &resource.at(member) {
        Value::Null => u64::from(carries(resource, member)),
        Value::Array(items) => u64::try_from(items.len()).unwrap_or(u64::MAX),
        _ => 1,
    }
}

/// Every finding of `document`, a FHIR JSON document `Bundle`, against the
/// vendored EPS profiles and the FHIR R4 document rules; empty when it
/// meets them.
///
/// # Errors
///
/// Returns an error when the document is no JSON or the package cannot be
/// read.
pub fn check(document: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let bundle: Value = serde_json::from_str(document)?;
    let [bundle_profile, composition_profile, patient_profile] =
        snapshots(["bundle-eu-eps", "composition-eu-eps", "patient-eu-eps"])?;
    let mut findings = Vec::new();
    let mut need = |held: bool, finding: String| {
        if !held {
            findings.push(finding);
        }
    };
    need(
        bundle.at("resourceType") == "Bundle",
        "no Bundle".to_owned(),
    );
    need(
        bundle.at("type") == "document",
        "Bundle.type is no document".to_owned(),
    );
    for (member, min) in required(&bundle_profile, "Bundle") {
        need(
            count(&bundle, &member) >= min,
            format!("Bundle.{member} below {min}"),
        );
    }
    need(
        bundle.at("identifier").at("system").is_string()
            && bundle.at("identifier").at("value").is_string(),
        "bdl-9: a document identifier with a system and a value".to_owned(),
    );
    need(
        bundle.at("timestamp").is_string(),
        "bdl-10: a timestamp".to_owned(),
    );
    let entries = bundle.at("entry").as_array().cloned().unwrap_or_default();
    let mut urls = BTreeSet::new();
    for entry in &entries {
        let url = entry.at("fullUrl").as_str().unwrap_or_default().to_owned();
        need(!url.is_empty(), "Bundle.entry.fullUrl 1..1".to_owned());
        need(
            entry.at("resource").is_object(),
            "Bundle.entry.resource 1..1".to_owned(),
        );
        for absent in ["search", "request", "response"] {
            need(
                entry.at(absent).is_null(),
                format!("Bundle.entry.{absent} 0..0"),
            );
        }
        need(
            urls.insert(url.clone()),
            format!("bdl-7: {url} is repeated"),
        );
    }
    let of_type = |kind: &str| -> Vec<&Value> {
        entries
            .iter()
            .map(|entry| entry.at("resource"))
            .filter(|resource| resource.at("resourceType") == kind)
            .collect()
    };
    let compositions = of_type("Composition");
    let patients = of_type("Patient");
    need(
        compositions.len() == 1,
        "Bundle.entry:composition 1..1".to_owned(),
    );
    need(patients.len() == 1, "Bundle.entry:patient 1..1".to_owned());
    need(
        entries
            .first()
            .is_some_and(|first| first.at("resource").at("resourceType") == "Composition"),
        "bdl-11: the composition is the first entry".to_owned(),
    );
    invariants(&bundle_profile, &entries, &mut need);
    if let Some(patient) = patients.first() {
        for (member, min) in required(&patient_profile, "Patient") {
            need(
                count(patient, &member) >= min,
                format!("Patient.{member} below {min}"),
            );
        }
        for name in patient.at("name").as_array().into_iter().flatten() {
            need(
                ["family", "given", "text"]
                    .iter()
                    .any(|part| !name.at(part).is_null()),
                "ips-pat-1: a Patient.name with no family, given or text".to_owned(),
            );
        }
    }
    if let Some(composition) = compositions.first() {
        composition_findings(
            (composition, full_url_of(&entries, composition)),
            &composition_profile,
            &urls,
            &mut need,
        );
    }
    Ok(findings)
}

/// The findings of the profile's reference invariants over `entries`: each
/// constraint `eps-bundle-…` names the resource types it holds and the
/// reference each must carry, read from its own expression.
fn invariants(profile: &[Value], entries: &[Value], need: &mut impl FnMut(bool, String)) {
    let constraints = profile
        .iter()
        .find(|element| element.at("id") == "Bundle")
        .and_then(|element| element.at("constraint").as_array().cloned())
        .unwrap_or_default();
    for constraint in constraints {
        let key = constraint.at("key").as_str().unwrap_or_default();
        let expression = constraint.at("expression").as_str().unwrap_or_default();
        if !key.starts_with("eps-bundle-") {
            continue;
        }
        let types: BTreeSet<&str> = expression
            .split("resource.is(")
            .skip(1)
            .filter_map(|rest| rest.split(')').next())
            .collect();
        let path = expression
            .rsplit(".all(resource.")
            .next()
            .and_then(|rest| rest.split(".exists()").next())
            .unwrap_or_default();
        let parts: Vec<&str> = path.split('.').collect();
        for entry in entries {
            let resource = entry.at("resource");
            let kind = resource.at("resourceType").as_str().unwrap_or_default();
            if !types.contains(kind) {
                continue;
            }
            let held = parts.iter().fold(resource, |value, part| match value {
                Value::Array(items) => items.first().map_or(&NULL, |item| item.at(part)),
                other => other.at(part),
            });
            need(!held.is_null(), format!("{key}: {kind}.{path}"));
        }
    }
}

/// Whether `concept`, a `CodeableConcept`, carries the coding `fixed` holds.
fn coded(concept: &Value, fixed: &Value) -> bool {
    concept.at("coding").as_array().is_some_and(|codings| {
        codings.iter().any(|coding| {
            coding.at("code") == fixed.at("code") && coding.at("system") == fixed.at("system")
        })
    })
}

/// The `fullUrl` of the entry of `entries` that holds `resource`, or the
/// empty string, which no reference resolves against.
fn full_url_of<'a>(entries: &'a [Value], resource: &Value) -> &'a str {
    entries
        .iter()
        .find(|entry| entry.at("resource") == resource)
        .and_then(|entry| entry.at("fullUrl").as_str())
        .unwrap_or_default()
}

/// Whether `reference`, held by the entry whose `fullUrl` is `holder`, names
/// one of `urls`.
///
/// An absolute reference names the entry whose `fullUrl` it is; a relative
/// `Type/id` resolves against the base of the holder's REST-style `fullUrl`
/// (FHIR R4 Bundle references,
/// <https://hl7.org/fhir/R4/bundle.html#references>).
fn resolves(reference: &Value, holder: &str, urls: &BTreeSet<String>) -> bool {
    let Some(url) = reference.at("reference").as_str() else {
        return false;
    };
    if urls.contains(url) {
        return true;
    }
    let relative = url.split('/').count() == 2 && !url.contains(':') && !url.starts_with('#');
    let base = holder
        .rsplitn(3, '/')
        .nth(2)
        .filter(|base| base.contains("://"));
    relative && base.is_some_and(|base| urls.contains(&format!("{base}/{url}")))
}

/// The findings of `composition`, held by the entry whose `fullUrl` is
/// `holder`, against its profile's `elements`, its references resolved
/// among `urls`.
fn composition_findings(
    (composition, holder): (&Value, &str),
    elements: &[Value],
    urls: &BTreeSet<String>,
    need: &mut impl FnMut(bool, String),
) {
    for (member, min) in required(elements, "Composition") {
        need(
            count(composition, &member) >= min,
            format!("Composition.{member} below {min}"),
        );
    }
    let pattern = |id: &str| {
        elements
            .iter()
            .find(|element| element.at("id") == id)
            .map_or(Value::Null, |element| {
                element
                    .at("patternCodeableConcept")
                    .at("coding")
                    .first_item()
                    .clone()
            })
    };
    need(
        coded(composition.at("type"), &pattern("Composition.type")),
        "Composition.type is no patient summary".to_owned(),
    );
    need(
        resolves(composition.at("subject"), holder, urls),
        "Composition.subject resolves".to_owned(),
    );
    for author in composition.at("author").as_array().into_iter().flatten() {
        need(
            resolves(author, holder, urls),
            "Composition.author resolves".to_owned(),
        );
    }
    let sections = composition
        .at("section")
        .as_array()
        .cloned()
        .unwrap_or_default();
    for element in elements {
        let Some(slice) = element
            .at("id")
            .as_str()
            .and_then(|id| id.strip_prefix("Composition.section:"))
            .filter(|slice| !slice.contains('.'))
        else {
            continue;
        };
        let min = element.at("min").as_u64().unwrap_or(0);
        let code = pattern(&format!("Composition.section:{slice}.code"));
        let held = sections
            .iter()
            .filter(|section| coded(section.at("code"), &code))
            .count();
        need(
            u64::try_from(held).unwrap_or(u64::MAX) >= min,
            format!("Composition.section:{slice} below {min}"),
        );
        need(held <= 1, format!("Composition.section:{slice} above 1"));
    }
    for section in &sections {
        section_findings(section, holder, urls, need);
    }
}

/// The findings of one composition `section` of the composition whose entry's
/// `fullUrl` is `holder`, its references resolved among `urls`.
fn section_findings(
    section: &Value,
    holder: &str,
    urls: &BTreeSet<String>,
    need: &mut impl FnMut(bool, String),
) {
    for member in ["title", "code", "text"] {
        need(
            !section.at(member).is_null(),
            format!("Composition.section.{member} 1..1"),
        );
    }
    need(
        section.at("section").is_null(),
        "Composition.section.section 0..0".to_owned(),
    );
    let entry = section.at("entry").as_array().map_or(0, Vec::len);
    need(
        entry == 0 || section.at("emptyReason").is_null(),
        "cmp-2: an empty reason beside an entry".to_owned(),
    );
    need(
        entry > 0 || !section.at("text").is_null() || !section.at("emptyReason").is_null(),
        "cmp-1: a section with no text and no entry".to_owned(),
    );
    for reference in section
        .at("entry")
        .as_array()
        .into_iter()
        .flatten()
        .chain(section.at("author").as_array().into_iter().flatten())
    {
        need(
            resolves(reference, holder, urls),
            format!("a section reference resolves: {reference}"),
        );
    }
    let div = section.at("text").at("div").as_str().unwrap_or_default();
    need(
        div.starts_with(r#"<div xmlns="http://www.w3.org/1999/xhtml">"#),
        "txt-1: the narrative is an XHTML div".to_owned(),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::resolves;

    /// The composition entry that holds the references.
    const HOLDER: &str = "http://example.org/fhir/Composition/c";

    /// The entries of a document under `http://example.org/fhir`.
    fn urls() -> BTreeSet<String> {
        BTreeSet::from([
            String::from("http://example.org/fhir/Composition/c"),
            String::from("http://example.org/fhir/Patient/p"),
        ])
    }

    #[test]
    fn an_absolute_reference_resolves_to_the_entry_it_names() {
        let reference = json!({ "reference": "http://example.org/fhir/Patient/p" });
        assert!(resolves(&reference, HOLDER, &urls()));
    }

    // FHIR R4 Bundle references: a relative reference resolves against the holder's RESTful base.
    #[test]
    fn a_relative_reference_resolves_against_the_holders_base() {
        assert!(resolves(
            &json!({ "reference": "Patient/p" }),
            HOLDER,
            &urls()
        ));
        assert!(!resolves(
            &json!({ "reference": "Patient/q" }),
            HOLDER,
            &urls()
        ));
    }

    #[test]
    fn a_relative_reference_held_by_an_entry_with_no_restful_full_url_does_not_resolve() {
        let reference = json!({ "reference": "Patient/p" });
        let holder = "urn:uuid:4f5c1d1e-0000-4000-8000-0000000000aa";
        assert!(!resolves(&reference, holder, &urls()));
    }

    #[test]
    fn a_reference_with_no_literal_does_not_resolve() {
        assert!(!resolves(
            &json!({ "display": "Synthetic" }),
            HOLDER,
            &urls()
        ));
    }
}
