// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the written report tests share: the report the gateway writes from
//! a composition about a synthetic patient, and the errors FerroBRIDGE's
//! validator finds in a document against its `Bundle` profile.

use std::error::Error;
use std::path::PathBuf;

use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::validation::ValidationError;
use ferrobridge_validator::set::PackageSet;
use ferrofed_eehrxf::report::{Report, Writes};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_identity::role::header::PatientHeader;
use fhir_types::r4::bundle::Bundle;

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
    let (operator, member) = (
        organisation("Synthetic operator"),
        organisation("Synthetic member"),
    );
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
