// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report's stored query: the entry archetypes of the
//! vendored CKM laboratory test result template, in the shared section
//! query shape, under its reserved name, feeding a row of the laboratory
//! report crosswalk.

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::crosswalk::laboratory::LABORATORY;
use ferrofed_eehrxf::laboratory::{self, ARCHETYPES, ELEMENT, SLUG};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use ferrofed_eehrxf::section_query::{ISSUER, PATIENT};
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
