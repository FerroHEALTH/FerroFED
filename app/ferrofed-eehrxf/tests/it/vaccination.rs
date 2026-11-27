// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vaccination history's stored query, under its reserved name, and the
//! document the gateway writes from one member's immunisation list through
//! the shipped `euvac` project, compiled against FerroBRIDGE's vendored
//! patient summary template, legally authenticated by the person who
//! attested the record, and held to the vendored MyHealth@EU Vaccination
//! 10.0.0-preview profiles.
#![expect(
    clippy::disallowed_types,
    reason = "a test reads the document it wrote as JSON"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::dataset::ResourceProfile;
use eehrxf::document::vaccination::{EUVAC_BUNDLE, EUVAC_COMPOSITION, IMMUNIZATION_HISTORY};
use eehrxf::receive::ReceivedDocument;
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::report::ServedError;
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use ferrofed_eehrxf::section_query::{self, ATTESTATIONS, ISSUER, PATIENT};
use ferrofed_eehrxf::vaccination::{self, SECTION, SLUG, Served, Unattested, Unnamed, Unversioned};
use ferrofed_identity::role::header::{PatientHeader, PersonName};
use ferrofed_testkit::medication;
use ferrofed_testkit::vaccination as fixture;
use openehr_query::parser;
use serde_json::Value;

use crate::conforming;

type TestResult = Result<(), Box<dyn Error>>;

/// The version id of the synthetic composition.
const UID: &str = "4f5c1d1e-0000-4000-8000-000000001980::cdr-a.example.org::1";

/// The vendored MyHealth@EU Vaccination 10.0.0-preview package.
fn package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/ehdsi-euvac/myhealth.eu.fhir.euvac-10.0.0-preview.tgz")
}

/// The profile at `url` of [`package`].
fn profile(url: &str) -> Result<ResourceProfile, Box<dyn Error>> {
    Ok(ResourceProfile::read(std::fs::File::open(package())?, url)?)
}

/// The shipped `euvac` project compiled against FerroBRIDGE's patient summary
/// template.
fn served() -> Result<Served, Box<dyn Error>> {
    let written = medication::patient_summary_template()?;
    Ok(Served::compile(Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    })?)
}

/// What the identity binding holds of the synthetic patient.
fn header() -> PatientHeader {
    PatientHeader {
        names: vec![PersonName {
            family: Some(String::from("Synthetic")),
            given: vec![String::from("Patient")],
            ..PersonName::default()
        }],
        birth_date: Some(String::from("1970-01-01")),
        ..PatientHeader::default()
    }
}

/// The composition and attesters of the synthetic member's version,
/// attested once, by a person.
fn version() -> Result<(String, Vec<vaccination::Attester>), Unversioned> {
    vaccination::original_version(&fixture::original_version(
        UID,
        Some(&format!("[{}]", fixture::attestation(false))),
    ))
}

/// What the gateway writes for the synthetic member's version, about the
/// patient `header` describes.
async fn written(header: &PatientHeader) -> Result<Result<Value, ServedError>, Box<dyn Error>> {
    let (_server, terminology) = conforming::language_terminology().await?;
    let (composition, attesters) = version()?;
    let written = conforming::attested(
        &served()?,
        (
            &composition,
            "urn:uuid:4f5c1d1e-0000-4000-8000-000000001980",
        ),
        (header, &attesters),
        &terminology,
    )
    .await;
    Ok(match written {
        Ok(bundle) => Ok(serde_json::to_value(&bundle)?),
        Err(refused) => Err(refused),
    })
}

#[tokio::test]
async fn transcoding_for_myhealth_eu_leaves_a_vaccination_history_as_recorded() -> TestResult {
    let (composition, attesters) = version()?;
    conforming::untouched(
        &served()?,
        (
            &composition,
            "urn:uuid:4f5c1d1e-0000-4000-8000-000000001980",
        ),
        (&header(), &attesters),
    )
    .await
}

#[test]
fn the_query_parses_back_to_the_tree_it_was_printed_from() -> TestResult {
    assert_eq!(
        section_query::attested(SECTION),
        parser::parse_str(&vaccination::aql())?
    );
    Ok(())
}

#[test]
fn the_query_selects_the_attestations_of_each_version_by_parameter_alone() {
    let aql = vaccination::aql();
    assert!(
        aql.contains("CONTAINS VERSION v CONTAINS COMPOSITION c"),
        "{aql}"
    );
    assert!(aql.contains(&format!("SECTION s0[{SECTION}]")), "{aql}");
    assert!(
        aql.contains(&format!("v/attestations AS {ATTESTATIONS}")),
        "{aql}"
    );
    assert!(aql.contains(&format!("${PATIENT}")), "{aql}");
    assert!(aql.contains(&format!("${ISSUER}")), "{aql}");
    assert!(!aql.contains('\''), "no literal: {aql}");
}

#[test]
fn the_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let definition = vaccination::definition()?;
    assert_eq!(
        format!("{NAMESPACE}::{SLUG}"),
        definition.name().to_string()
    );
    assert!(reserved::reserves(definition.name()));
    assert_eq!(VERSION, definition.version());
    assert_eq!(SAVED, definition.saved());
    assert!(
        reserved::definitions()?.contains(&definition),
        "the registry holds it"
    );
    Ok(())
}

#[tokio::test]
async fn an_attested_immunisation_list_is_written_as_a_legally_authenticated_history() -> TestResult
{
    let bundle = written(&header()).await??;
    let text = bundle.to_string();
    ReceivedDocument::read(&text)?.check(&profile(EUVAC_BUNDLE)?, &profile(EUVAC_COMPOSITION)?)?;
    let composition = &bundle["entry"][0]["resource"];
    assert_eq!(
        composition["type"]["coding"][0]["code"],
        IMMUNIZATION_HISTORY
    );
    assert_eq!(composition["language"], "nl-NL", "{composition}");
    let [attester] = composition["attester"]
        .as_array()
        .ok_or("an attester")?
        .as_slice()
    else {
        return Err(format!("one attester: {composition}").into());
    };
    assert_eq!(attester["mode"], "legal");
    assert_eq!(attester["time"], fixture::ATTESTED);
    let party = attester["party"]["reference"].as_str().ok_or("a party")?;
    let practitioner = bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["fullUrl"] == party)
        .map(|entry| &entry["resource"])
        .ok_or("the party resolves")?;
    assert_eq!(practitioner["resourceType"], "Practitioner");
    assert_eq!(practitioner["name"][0]["text"], fixture::ATTESTER);
    let immunizations: Vec<&Value> = bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == "Immunization")
        .collect();
    let [immunization] = immunizations.as_slice() else {
        return Err(format!("one Immunization: {bundle}").into());
    };
    assert_eq!(
        immunization["vaccineCode"]["coding"][0]["code"],
        fixture::VACCINE
    );
    ferrofed_testkit::emit::document(
        ferrofed_testkit::emit::Lane::Vaccination,
        "ferrofed-eehrxf-vaccination-history",
        &text,
    )?;
    Ok(())
}

#[test]
fn a_version_no_one_attested_or_only_pending_names_no_attester() {
    let pending = format!("[{}]", fixture::attestation(true));
    for attestations in [None, Some(pending.as_str())] {
        assert!(matches!(
            vaccination::original_version(&fixture::original_version(UID, attestations)),
            Err(Unversioned::Unattested(Unattested::None))
        ));
    }
}

#[tokio::test]
async fn a_patient_with_no_birth_date_or_no_name_gets_no_history() -> TestResult {
    let undated = PatientHeader {
        birth_date: None,
        ..header()
    };
    assert!(matches!(
        written(&undated).await?,
        Err(ServedError::Unnamed(Unnamed::BirthDate))
    ));
    let unnamed = PatientHeader {
        names: Vec::new(),
        ..header()
    };
    assert!(matches!(
        written(&unnamed).await?,
        Err(ServedError::Unnamed(Unnamed::Name))
    ));
    Ok(())
}

#[tokio::test]
async fn a_report_with_no_attester_is_refused() -> TestResult {
    let (_server, terminology) = conforming::language_terminology().await?;
    let refused = conforming::attested(
        &served()?,
        (
            &fixture::composition(UID),
            "urn:uuid:4f5c1d1e-0000-4000-8000-000000001981",
        ),
        (&header(), &[]),
        &terminology,
    )
    .await;
    assert!(matches!(
        refused,
        Err(ServedError::Unattested(Unattested::None))
    ));
    Ok(())
}
