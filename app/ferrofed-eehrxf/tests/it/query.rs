// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The section queries as AQL: printed from the syntax tree, parsed back to
//! the same tree, whole compositions with their template id, the patient a
//! parameter, and each one under its reserved name at the one version.

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_eehrxf::patient_summary::{ISSUER, PATIENT, Section, definitions};
use ferrofed_eehrxf::reserved::{self, NAMESPACE, SAVED, VERSION};
use openehr_query::ast::{ColumnExpr, IdentifiedExpr, Terminal, WhereExpr};
use openehr_query::parser;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn each_query_parses_back_to_the_tree_it_was_printed_from() -> TestResult {
    for section in Section::ALL {
        let printed = section.aql();
        let parsed = parser::parse_str(&printed)?;
        assert_eq!(
            section.query(),
            parsed,
            "{section:?}: printer::to_aql round trip"
        );
    }
    Ok(())
}

#[test]
fn each_query_selects_distinct_whole_compositions_with_uid_and_template_id() {
    for section in Section::ALL {
        let query = section.query();
        assert!(
            query.select.distinct,
            "{section:?}: AQL DISTINCT, one row per composition"
        );
        let columns: Vec<(String, Option<&str>)> = query
            .select
            .columns
            .iter()
            .map(|column| match &column.column {
                ColumnExpr::Path(path) => (
                    path.path
                        .as_ref()
                        .map_or_else(|| path.root.clone(), |tail| format!("{}/{tail}", path.root)),
                    column.alias.as_deref(),
                ),
                other => panic!("{section:?}: a column that is no path: {other:?}"),
            })
            .collect();
        assert_eq!(
            vec![
                ("c".to_owned(), Some("composition")),
                ("c/uid/value".to_owned(), Some("uid")),
                (
                    "c/archetype_details/template_id/value".to_owned(),
                    Some("template_id")
                ),
            ],
            columns,
            "{section:?}"
        );
        assert!(query.order_by.is_empty() && query.limit.is_none());
        let aql = section.aql();
        for archetype in section.archetypes() {
            assert!(aql.contains(archetype.id), "{section:?}: {aql}");
        }
    }
}

/// Every terminal a `WHERE` tree compares with, `None` for a condition that
/// is no comparison.
fn terminals(tree: &WhereExpr, found: &mut Vec<Option<Terminal>>) {
    match tree {
        WhereExpr::Identified(IdentifiedExpr::Compare { rhs, .. }, _) => {
            found.push(Some(rhs.clone()));
        }
        WhereExpr::Identified(_, _) => found.push(None),
        WhereExpr::Not(inner) => terminals(inner, found),
        WhereExpr::And(left, right) | WhereExpr::Or(left, right) => {
            terminals(left, found);
            terminals(right, found);
        }
    }
}

// NOTE: §5.4.1, N33; the definition is held at rest, so it names its patient
// by parameters alone and holds no value of its own.
#[test]
fn the_patient_and_its_namespace_are_parameters_and_nothing_else_is_compared() {
    for section in Section::ALL {
        let query = section.query();
        let mut found = Vec::new();
        terminals(query.where_.as_ref().expect("a WHERE clause"), &mut found);
        assert_eq!(
            vec![
                Some(Terminal::Parameter(format!("${PATIENT}"))),
                Some(Terminal::Parameter(format!("${ISSUER}"))),
            ],
            found,
            "{section:?}"
        );
        let aql = section.aql();
        assert!(
            aql.contains("e/ehr_status/subject/external_ref/id/value=$patient")
                && aql.contains("e/ehr_status/subject/external_ref/namespace=$namespace"),
            "{section:?}: {aql}"
        );
        assert!(!aql.contains('\''), "{section:?}: no literal: {aql}");
    }
}

#[test]
fn every_query_sits_under_its_reserved_name_at_the_immutable_version() -> TestResult {
    let held = definitions()?;
    assert_eq!(Section::ALL.len(), held.len());
    let mut names = BTreeSet::new();
    for (section, definition) in Section::ALL.into_iter().zip(&held) {
        assert_eq!(
            format!("{NAMESPACE}::patient-summary-{}", section.slug()),
            definition.name().as_str()
        );
        assert_eq!(Some(NAMESPACE), definition.name().namespace());
        assert!(reserved::reserves(definition.name()));
        assert_eq!(VERSION, definition.version());
        assert_eq!("1.0.0", definition.version().to_string());
        assert_eq!(SAVED, definition.saved());
        assert_eq!(section.aql(), definition.aql());
        names.insert(definition.name().clone());
    }
    assert_eq!(held.len(), names.len(), "one name per section");
    Ok(())
}

#[test]
fn a_section_query_is_the_same_text_every_time_it_is_built() -> TestResult {
    assert_eq!(definitions()?, definitions()?);
    Ok(())
}
