// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report's stored query: the entry archetypes of the
//! vendored CKM laboratory test result template, in the shared section
//! query shape, under its reserved name, feeding a row of the laboratory
//! report crosswalk; and the report the gateway writes from one member's
//! composition, held to `Bundle-eu-lab` over the Laboratory Report 2.0.0
//! package closure by FerroBRIDGE's validator.

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::crosswalk::laboratory::LABORATORY;
use eehrxf::document::laboratory::LAB_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::validation::ValidationError;
use ferrobridge_validator::set::PackageSet;
use ferrofed_eehrxf::laboratory::{self, ARCHETYPES, ELEMENT, SLUG, Served};
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::report::{Report, Writes as _};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use ferrofed_eehrxf::section_query::{ISSUER, PATIENT};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_identity::role::header::{PatientHeader, PersonName};
use ferrofed_testkit::{fhir_packages, laboratory as fixture};
use openehr_query::parser;

type TestResult = Result<(), Box<dyn Error>>;

/// The template source, vendored by `scripts/vendor/openehr-ckm.sh`.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/openehr-ckm-laboratory/generic-lab-test-result.oet"
);

/// The CKM record of the template.
const RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/openehr-ckm-laboratory/generic-lab-test-result.ckm.xml"
);

/// The RM classes of the entries a composition holds.
const ENTRIES: [&str; 5] = [
    "OBSERVATION",
    "EVALUATION",
    "INSTRUCTION",
    "ACTION",
    "ADMIN_ENTRY",
];

/// Every entry archetype the template names.
fn template_entries() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let text = std::fs::read_to_string(TEMPLATE)?;
    let mut entries = BTreeSet::new();
    for item in text.split("archetype_id=\"").skip(1) {
        let id = item.split('"').next().ok_or("an unterminated id")?;
        let class = id
            .strip_prefix("openEHR-EHR-")
            .and_then(|rest| rest.split('.').next())
            .ok_or("an archetype id outside openEHR-EHR")?;
        if ENTRIES.contains(&class) {
            entries.insert(id.to_owned());
        }
    }
    Ok(entries)
}

#[test]
fn the_vendored_template_is_ckm_1013_26_408_at_asset_version_3_in_draft() -> TestResult {
    let record = std::fs::read_to_string(RECORD)?;
    assert!(record.contains("<cid>1013.26.408</cid>"), "{record}");
    assert!(record.contains("<status>DRAFT</status>"), "{record}");
    assert!(
        record.contains("<versionAsset>3</versionAsset>"),
        "{record}"
    );
    Ok(())
}

#[test]
fn the_query_names_every_entry_archetype_of_the_template_and_no_other() -> TestResult {
    let named: BTreeSet<String> = ARCHETYPES
        .iter()
        .map(|archetype| archetype.id.to_owned())
        .collect();
    assert_eq!(template_entries()?, named);
    for archetype in ARCHETYPES {
        assert!(
            archetype
                .id
                .starts_with(&format!("openEHR-EHR-{}.", archetype.class)),
            "{archetype:?}"
        );
    }
    Ok(())
}

#[test]
fn the_query_parses_back_to_the_tree_it_was_printed_from() -> TestResult {
    assert_eq!(laboratory::query(), parser::parse_str(&laboratory::aql())?);
    Ok(())
}

#[test]
fn the_query_names_its_patient_by_parameter_alone() {
    let aql = laboratory::aql();
    assert!(aql.contains(&format!("${PATIENT}")), "{aql}");
    assert!(aql.contains(&format!("${ISSUER}")), "{aql}");
    assert!(!aql.contains('\''), "no literal: {aql}");
    assert!(aql.starts_with("SELECT DISTINCT"), "{aql}");
    for archetype in ARCHETYPES {
        assert!(aql.contains(archetype.id), "{aql}");
    }
}

#[test]
fn the_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let definition = laboratory::definition()?;
    assert_eq!(
        format!("{NAMESPACE}::{SLUG}"),
        definition.name().to_string()
    );
    assert!(reserved::reserves(definition.name()));
    assert_eq!(VERSION, definition.version());
    assert_eq!(SAVED, definition.saved());
    assert_eq!(laboratory::aql(), definition.aql());
    assert_eq!(laboratory::definition()?, definition, "the same every time");
    Ok(())
}

#[test]
fn the_query_feeds_a_row_of_the_laboratory_report_crosswalk() {
    assert!(LABORATORY.row(ELEMENT).is_some(), "{ELEMENT}");
}

#[test]
fn the_gateway_and_the_testkit_name_one_result_context() {
    assert_eq!(
        laboratory::RESULT_CONTEXT,
        ferrofed_testkit::laboratory::RESULT_CONTEXT
    );
    assert!(
        eehrxf::mappings::LABORATORY
            .contexts()
            .contains(&laboratory::RESULT_CONTEXT),
        "the shipped project lists the result context"
    );
}

/// The laboratory report the gateway writes from the testkit's synthetic
/// composition, about the patient the identity binding describes with
/// `header`, as JSON text.
fn written(header: &PatientHeader) -> Result<String, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let identifier = ("urn:oid:2.999.1", "synthetic-patient-0001");
    let composition = fixture::composition(
        dir.path(),
        &[identifier],
        (
            "7c4e1d20-0000-4000-8000-0000000008b1::cdr-a.example.org::1",
            &[],
        ),
    )?;
    let opt = std::fs::read_to_string(fixture::opt())?;
    let organisation = |name: &str| Organisation {
        id: name.to_lowercase().replace(' ', "-"),
        name: Some(name.to_owned()),
        identifiers: Vec::new(),
    };
    let (operator, member) = (
        organisation("Synthetic operator"),
        organisation("Synthetic member"),
    );
    let bundle = Served::compile(Template::of(&opt, &[]))?.document(
        &composition,
        Report {
            base: "https://gateway.example.org/fhir",
            identifier: "urn:uuid:7c4e1d20-0000-4000-8000-0000000008b1",
            timestamp: "2026-10-21T08:00:00Z",
            patient: identifier,
            header,
            operator: &operator,
            member: &member,
        },
    )?;
    Ok(serde_json::to_string(&bundle)?)
}

/// Every error FerroBRIDGE's validator finds in `document` against
/// `Bundle-eu-lab`, over the Laboratory Report package and its dependency
/// closure.
async fn breaks(document: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut paths = vec![fixture::package()];
    paths.extend(fhir_packages::dependencies()?);
    let set = PackageSet::load(paths.iter())?;
    let validated = ReceivedDocument::read(document)?
        .validate(&set, LAB_BUNDLE, None::<&ferrobridge_term::client::Client>)
        .await;
    match validated {
        Ok(_) => Ok(Vec::new()),
        Err(ValidationError::NonConformant { findings }) => {
            Ok(findings.iter().map(ToString::to_string).collect())
        }
        Err(other) => Err(other.into()),
    }
}

#[tokio::test]
async fn a_written_laboratory_report_conforms_to_bundle_eu_lab() -> TestResult {
    let header = PatientHeader {
        names: vec![PersonName {
            family: Some(String::from("SENTINEL-FAMILY-l4b2")),
            ..PersonName::default()
        }],
        birth_date: Some(String::from("1970-01-01")),
        ..PatientHeader::default()
    };
    let breaks = breaks(&written(&header)?).await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}

#[tokio::test]
async fn a_laboratory_report_of_a_patient_the_binding_holds_nothing_of_conforms() -> TestResult {
    let breaks = breaks(&written(&PatientHeader::default())?).await?;
    assert!(breaks.is_empty(), "{breaks:#?}");
    Ok(())
}
