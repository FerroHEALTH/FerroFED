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
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use eehrxf::terminology::translation::{Binding, MYHEALTH_EU_CORE};
use ferrofed_eehrxf::report::{Delivered, Report, ServedError, Writes};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_eehrxf::terminology::BridgeTerminology;
use ferrofed_eehrxf::vaccination::Attester;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_testkit::ehdsi_language;
use ferrofed_testkit::eps::transcoding;
use ferrofed_testkit::mock::Server;
use fhir_types::r4::bundle::Bundle;
use serde_json::Value;

/// The terminology the stub server of [`ehdsi_language::start`] answers
/// eHDSILanguage from; the server lives as long as the returned handle.
pub(crate) async fn language_terminology() -> Result<(Server, Terminology), Box<dyn Error>> {
    let server = ehdsi_language::start().await;
    let endpoint = serde_json::from_value(serde_json::json!({
        "url": format!("{}{}", server.uri(), ehdsi_language::BASE),
        "retry": { "max_attempts": 1 },
    }))?;
    let client = BridgeTerminology::from_endpoint(&endpoint, "eehrxf.terminology.endpoint")?;
    Ok((server, Terminology::server(Arc::new(client))))
}

/// The report `served` writes from `composition`, named `identifier`, read
/// at a synthetic member for a synthetic operator, about the synthetic
/// patient the identity binding describes with `header`, asking
/// `terminology` whether a value set the form binds holds what it writes.
pub(crate) async fn written(
    served: &dyn Writes,
    (composition, identifier): (&str, &str),
    header: &PatientHeader,
    terminology: &Terminology,
) -> Result<Bundle, Box<dyn Error>> {
    Ok(attested(
        served,
        (composition, identifier),
        (header, &[]),
        terminology,
    )
    .await?)
}

/// What [`written`] answers for a version the persons `attesters` attested.
pub(crate) async fn attested(
    served: &dyn Writes,
    (composition, identifier): (&str, &str),
    (header, attesters): (&PatientHeader, &[Attester]),
    terminology: &Terminology,
) -> Result<Bundle, ServedError> {
    delivered(
        served,
        (composition, identifier),
        (header, attesters),
        (terminology, &[]),
    )
    .await
    .map(|delivered| delivered.bundle)
}

/// What [`attested`] answers with the mapped resources translated through
/// `terminology` to meet `bindings`, with each concept left unbound.
pub(crate) async fn delivered(
    served: &dyn Writes,
    (composition, identifier): (&str, &str),
    (header, attesters): (&PatientHeader, &[Attester]),
    (terminology, bindings): (&Terminology, &[Binding]),
) -> Result<Delivered, ServedError> {
    let organisation = |name: &str| Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: Vec::new(),
    };
    let (operator, member) = (organisation("Synthetic operator"), organisation(MEMBER));
    served
        .document(
            composition,
            Report {
                base: "https://gateway.example.org/fhir",
                identifier,
                timestamp: "2026-10-21T08:00:00Z",
                patient: ("urn:oid:2.999.1", "synthetic-patient-0001"),
                header,
                operator: &operator,
                member: &member,
                attesters,
            },
            (terminology, bindings),
        )
        .await
}

/// The terminology of [`language_terminology`], whose server also maps
/// codes for MyHealth@EU as [`transcoding::mount`] describes.
pub(crate) async fn transcoding_terminology() -> Result<(Server, Terminology), Box<dyn Error>> {
    let (server, terminology) = language_terminology().await?;
    transcoding::mount(&server).await;
    Ok((server, terminology))
}

/// Holds that transcoding for MyHealth@EU leaves the report `served` writes
/// from `composition` as it is and asks the server for no translation: no
/// MyHealth@EU Core binding is on an element the report writes.
pub(crate) async fn untouched(
    served: &dyn Writes,
    (composition, identifier): (&str, &str),
    (header, attesters): (&PatientHeader, &[Attester]),
) -> Result<(), Box<dyn Error>> {
    let (stub, terminology) = transcoding_terminology().await?;
    let write = |bindings| {
        delivered(
            served,
            (composition, identifier),
            (header, attesters),
            (&terminology, bindings),
        )
    };
    let recorded = write(&[]).await?;
    let transcoded = write(&MYHEALTH_EU_CORE).await?;
    assert!(transcoded.unbound.is_empty(), "{:?}", transcoded.unbound);
    assert_eq!(
        serde_json::to_value(&recorded.bundle)?,
        serde_json::to_value(&transcoded.bundle)?,
        "the report is written as recorded"
    );
    let asked = ferrofed_testkit::mock::recorded(&stub)
        .await?
        .iter()
        .filter(|request| request.url.path().ends_with("/ConceptMap/$translate"))
        .count();
    assert_eq!(0, asked, "no translation is asked");
    Ok(())
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
