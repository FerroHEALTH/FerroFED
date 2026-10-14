// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The structural check of a `DocumentReference` against the vendored HL7
//! Europe EU Health Data API 1.0.0-ballot profile `EehrxfMhdDocumentReference`
//! (#810).
//!
//! [`check`] reads the profile's snapshot from the vendored package and holds
//! a resource to the cardinality of every element it constrains, slices and
//! choice elements aside, at every occurrence of the element's parent, and
//! to the `type` binding, the LOINC codes of the value sets that
//! `eehrxf-document-type-vs` includes. It answers every finding, so a test
//! names what is wrong. HL7's FHIR Validator is the full check (#688).
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored profile and the resource are read as JSON"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde_json::Value;

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The canonical URL of the profile.
pub const PROFILE: &str =
    "http://hl7.eu/fhir/health-data-api/StructureDefinition/EehrxfMhdDocumentReference";

/// The canonical URL of the document type value set the profile binds.
const DOCUMENT_TYPES: &str = "http://hl7.eu/fhir/health-data-api/ValueSet/eehrxf-document-type-vs";

/// Returns the path of the vendored EU Health Data API package.
#[must_use]
pub fn package() -> PathBuf {
    PathBuf::from(ROOT)
        .join("docs/specs/eu-hl7-health-data-api/hl7.fhir.eu.health-data-api-1.0.0-ballot.tgz")
}

/// The profile and every value set of the package, by canonical URL.
fn read_package() -> Result<(Value, BTreeMap<String, Value>), Box<dyn Error>> {
    let mut profile = None;
    let mut value_sets = BTreeMap::new();
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let wanted = path == "package/StructureDefinition-EehrxfMhdDocumentReference.json"
            || (path.starts_with("package/ValueSet-")
                && std::path::Path::new(&path)
                    .extension()
                    .is_some_and(|held| held == "json"));
        if !wanted {
            continue;
        }
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        let resource: Value = serde_json::from_str(&text)?;
        if path.starts_with("package/ValueSet-") {
            let url = resource
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            value_sets.insert(url, resource);
        } else {
            profile = Some(resource);
        }
    }
    Ok((profile.ok_or("the package lacks the profile")?, value_sets))
}

/// The codes, each a system and a code, the value set `url` includes, the
/// value sets it includes followed.
fn codes(value_sets: &BTreeMap<String, Value>, url: &str) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let Some(includes) = value_sets
        .get(url)
        .and_then(|value_set| value_set.pointer("/compose/include"))
        .and_then(Value::as_array)
    else {
        return found;
    };
    for include in includes {
        let system = include
            .get("system")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for concept in include
            .get("concept")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(code) = concept.get("code").and_then(Value::as_str) {
                found.insert((system.to_owned(), code.to_owned()));
            }
        }
        for nested in include
            .get("valueSet")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            found.extend(codes(value_sets, nested));
        }
    }
    found
}

/// Every occurrence of the element at the dotted `path` under `nodes`,
/// arrays flattened.
fn walk<'a>(nodes: Vec<&'a Value>, path: &[&str]) -> Vec<&'a Value> {
    path.iter().fold(nodes, |nodes, member| {
        nodes
            .into_iter()
            .filter_map(|node| node.get(member))
            .flat_map(|value| match value {
                Value::Array(items) => items.iter().collect(),
                other => vec![other],
            })
            .collect()
    })
}

/// How many times `node` carries `member`, a primitive with only an
/// extension included (`_member`).
fn count(node: &Value, member: &str) -> u64 {
    let of = |value: Option<&Value>| match value {
        None | Some(Value::Null) => 0,
        Some(Value::Array(items)) => u64::try_from(items.len()).unwrap_or(u64::MAX),
        Some(_) => 1,
    };
    of(node.get(member)).max(of(node.get(format!("_{member}"))))
}

/// Every finding of `resource`, a FHIR JSON `DocumentReference`, against
/// `EehrxfMhdDocumentReference`; empty when it meets the profile.
///
/// # Errors
///
/// Returns an error when the resource is no JSON or the package cannot be
/// read.
pub fn check(resource: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let resource: Value = serde_json::from_str(resource)?;
    let (profile, value_sets) = read_package()?;
    let mut findings = Vec::new();
    if resource.get("resourceType").and_then(Value::as_str) != Some("DocumentReference") {
        findings.push("no DocumentReference".to_owned());
    }
    let claims = resource
        .pointer("/meta/profile")
        .and_then(Value::as_array)
        .is_some_and(|profiles| profiles.iter().any(|held| held == PROFILE));
    if !claims {
        findings.push(format!("meta.profile does not name {PROFILE}"));
    }
    let elements = profile
        .pointer("/snapshot/element")
        .and_then(Value::as_array)
        .ok_or("a profile without a snapshot")?;
    for element in elements {
        let Some(id) = element.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(relative) = id.strip_prefix("DocumentReference.") else {
            continue;
        };
        if relative.contains([':', '[']) {
            continue;
        }
        let segments: Vec<&str> = relative.split('.').collect();
        let Some((member, parents)) = segments.split_last() else {
            continue;
        };
        let min = element.get("min").and_then(Value::as_u64).unwrap_or(0);
        let max = element
            .get("max")
            .and_then(Value::as_str)
            .and_then(|max| max.parse::<u64>().ok());
        for parent in walk(vec![&resource], parents) {
            let held = count(parent, member);
            if held < min {
                findings.push(format!("DocumentReference.{relative} below {min}"));
            }
            if max.is_some_and(|max| held > max) {
                findings.push(format!("DocumentReference.{relative} above its maximum"));
            }
        }
    }
    let allowed = codes(&value_sets, DOCUMENT_TYPES);
    for coding in walk(vec![&resource], &["type", "coding"]) {
        let held = (
            coding
                .get("system")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            coding
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
        if !allowed.contains(&held) {
            findings.push(format!(
                "DocumentReference.type {}|{} is outside {DOCUMENT_TYPES}",
                held.0, held.1
            ));
        }
    }
    Ok(findings)
}
