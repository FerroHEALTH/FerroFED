// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The medication queries of the map mode: one per entry archetype of the
//! vendored CKM medication archetypes, in the shared section query shape,
//! under their reserved names, the order query leaving out every imported
//! copy (A102 point 1).

use std::error::Error;

use ferrofed_eehrxf::medication::{self, ACTION, Medication, ORDER};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use ferrofed_eehrxf::section_query::{ISSUER, PATIENT};
use openehr_query::parser;

type TestResult = Result<(), Box<dyn Error>>;

/// The vendored archetype of `archetype`, as CKM serves it in ADL 1.4.
fn vendored(id: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!(
        "{}/../../docs/specs/openehr-ckm-medication/{id}.adl",
        env!("CARGO_MANIFEST_DIR")
    ))?)
}

#[test]
fn each_query_names_a_vendored_ckm_archetype_of_its_class() -> TestResult {
    for (query, archetype) in [(Medication::Orders, ORDER), (Medication::Actions, ACTION)] {
        assert_eq!(archetype, query.archetype());
        let adl = vendored(archetype.id)?;
        assert!(adl.contains(archetype.id), "{}", archetype.id);
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
fn each_query_parses_back_to_the_tree_it_was_printed_from() -> TestResult {
    for query in Medication::ALL {
        assert_eq!(query.query(), parser::parse_str(&query.aql())?, "{query:?}");
    }
    Ok(())
}

#[test]
fn each_query_names_its_patient_by_parameter_alone() {
    for query in Medication::ALL {
        let aql = query.aql();
        assert!(aql.contains(&format!("${PATIENT}")), "{aql}");
        assert!(aql.contains(&format!("${ISSUER}")), "{aql}");
        assert!(!aql.contains('\''), "no literal: {aql}");
        assert!(aql.starts_with("SELECT DISTINCT"), "{aql}");
        assert!(aql.contains(query.archetype().id), "{aql}");
        assert!(
            aql.contains("c/uid/value"),
            "the uid names its creating system: {aql}"
        );
    }
}

#[test]
fn the_order_query_leaves_out_every_imported_copy_and_the_action_query_none() {
    assert!(
        Medication::Orders
            .aql()
            .contains("NOT EXISTS c/feeder_audit"),
        "{}",
        Medication::Orders.aql()
    );
    assert!(
        !Medication::Actions.aql().contains("feeder_audit"),
        "a dispensation the face wrote keeps its original: {}",
        Medication::Actions.aql()
    );
}

#[test]
fn each_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let held = medication::definitions()?;
    assert_eq!(Medication::ALL.len(), held.len());
    for (query, definition) in Medication::ALL.iter().zip(&held) {
        assert_eq!(
            format!("{NAMESPACE}::{}", query.slug()),
            definition.name().to_string()
        );
        assert!(reserved::reserves(definition.name()));
        assert_eq!(VERSION, definition.version());
        assert_eq!(SAVED, definition.saved());
        assert_eq!(query.aql(), definition.aql());
    }
    let all = reserved::definitions()?;
    for definition in &held {
        assert!(all.contains(definition), "{}", definition.name());
    }
    Ok(())
}
