// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the written report tests share: the report the gateway writes from
//! a composition about a synthetic patient, the custodian its
//! `Composition` names, and the errors FerroBRIDGE's validator finds in a
//! document against its `Bundle` profile.

use std::error::Error;
use std::path::PathBuf;

use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::validation::ValidationError;
use ferrobridge_validator::set::PackageSet;
use ferrofed_eehrxf::report::{Report, Writes};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_identity::role::header::PatientHeader;
use fhir_types::r4::bundle::Bundle;
use serde_json::Value;

/// The report `served` writes from `composition`, named `identifier`, read
/// at a synthetic member for a synthetic operator, about the synthetic
/// patient the identity binding describes with `header`.
pub(crate) fn written(
    served: &dyn Writes,
    (composition, identifier): (&str, &str),
    header: &PatientHeader,
) -> Result<Bundle, Box<dyn Error>> {
    let organisation = |name: &str| Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: Vec::new(),
    };
    let (operator, member) = (organisation("Synthetic operator"), organisation(MEMBER));
    Ok(served.document(
        composition,
        Report {
            base: "https://gateway.example.org/fhir",
            identifier,
            timestamp: "2026-10-21T08:00:00Z",
            patient: ("urn:oid:2.999.1", "synthetic-patient-0001"),
            header,
            operator: &operator,
            member: &member,
        },
    )?)
}

/// The name of the member [`written`] reads the report at.
pub(crate) const MEMBER: &str = "Synthetic member";

/// The resource of the entry of `bundle` whose `fullUrl` the custodian of
/// its one `Composition` names.
pub(crate) fn custodian(bundle: &Value) -> Result<&Value, Box<dyn Error>> {
    let entries = bundle
        .get("entry")
        .and_then(Value::as_array)
        .ok_or("entries")?;
    let resources = || entries.iter().filter_map(|entry| entry.get("resource"));
    let [composition] = resources()
        .filter(|resource| resource.get("resourceType") == Some(&Value::from("Composition")))
        .collect::<Vec<_>>()[..]
    else {
        return Err("one Composition".into());
    };
    let named = composition
        .get("custodian")
        .and_then(|custodian| custodian.get("reference"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the Composition names a custodian: {composition}"))?;
    entries
        .iter()
        .find(|entry| entry.get("fullUrl").and_then(Value::as_str) == Some(named))
        .and_then(|entry| entry.get("resource"))
        .ok_or_else(|| format!("the custodian {named} resolves to an entry").into())
}

/// Every error FerroBRIDGE's validator finds in `document` against
/// `profile`, over `package` and its dependency closure `dependencies`.
pub(crate) async fn breaks(
    document: &str,
    profile: &str,
    (package, dependencies): (PathBuf, Vec<PathBuf>),
) -> Result<Vec<String>, Box<dyn Error>> {
    let set = PackageSet::load(std::iter::once(package).chain(dependencies))?;
    let validated = ReceivedDocument::read(document)?
        .validate(&set, profile, None::<&ferrobridge_term::client::Client>)
        .await;
    match validated {
        Ok(_) => Ok(Vec::new()),
        Err(ValidationError::NonConformant { findings }) => {
            Ok(findings.iter().map(ToString::to_string).collect())
        }
        Err(other) => Err(other.into()),
    }
}
