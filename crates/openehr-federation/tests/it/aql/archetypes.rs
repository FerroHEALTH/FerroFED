// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The archetype and template ids a bound query constrains its data to: the
//! archetype predicates of the containment and the `=` comparisons of the
//! archetype node id and the template id, never one the query excludes
//! (AQL §Archetype predicate, §Class expressions, §Containment).

use std::collections::BTreeSet;

use openehr_federation::aql::archetypes::Constrained;
use openehr_federation::aql::{Paging, analyse};
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;

use super::{analysed, ask_all};

fn constrained(aql: &str) -> Constrained {
    analysed(aql, &ask_all())
        .expect("an analysed query")
        .constrained()
        .clone()
}

fn ids(set: &BTreeSet<String>) -> Vec<&str> {
    set.iter().map(String::as_str).collect()
}

#[test]
fn archetype_predicates_in_the_containment_are_read() {
    let found = constrained(
        "SELECT o/data FROM EHR e CONTAINS COMPOSITION c[openEHR-EHR-COMPOSITION.report.v1] \
         CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.lab_test.v1]",
    );
    assert_eq!(
        ids(found.archetypes()),
        [
            "openEHR-EHR-COMPOSITION.report.v1",
            "openEHR-EHR-OBSERVATION.lab_test.v1"
        ]
    );
    assert!(found.templates().is_empty());
}

#[test]
fn a_template_and_an_archetype_compared_with_equals_are_read() {
    let found = constrained(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_details/template_id/value = 'Example Report.v1' \
         AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.report.v1'",
    );
    assert_eq!(ids(found.templates()), ["Example Report.v1"]);
    assert_eq!(
        ids(found.archetypes()),
        ["openEHR-EHR-COMPOSITION.report.v1"]
    );
}

#[test]
fn an_excluded_or_patterned_constraint_is_not_read() {
    let found = constrained(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         NOT CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.lab_test.v1] \
         WHERE NOT c/archetype_details/template_id/value = 'Excluded.v1' \
         AND c/archetype_node_id != 'openEHR-EHR-COMPOSITION.other.v1' \
         AND c/archetype_details/template_id/value LIKE 'Lab*'",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn every_branch_of_an_or_is_read() {
    let found = constrained(
        "SELECT c, d FROM EHR e CONTAINS (COMPOSITION c[openEHR-EHR-COMPOSITION.a.v1] \
         OR COMPOSITION d[openEHR-EHR-COMPOSITION.b.v1])",
    );
    assert_eq!(
        ids(found.archetypes()),
        [
            "openEHR-EHR-COMPOSITION.a.v1",
            "openEHR-EHR-COMPOSITION.b.v1"
        ]
    );
}

#[test]
fn a_query_naming_no_archetype_names_nothing() {
    assert!(constrained("SELECT c FROM EHR e CONTAINS COMPOSITION c").is_empty());
}

#[test]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
fn a_bound_parameter_is_read_as_its_value() -> Result<(), Box<dyn std::error::Error>> {
    let mut parameters = Parameters::new();
    parameters.insert("template", Primitive::String("Bound Report.v1".to_owned()));
    let analysis = analyse(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_details/template_id/value = $template",
        &parameters,
        Paging::default(),
        &ask_all(),
    )?;
    assert_eq!(ids(analysis.constrained().templates()), ["Bound Report.v1"]);
    Ok(())
}

#[test]
fn an_archetype_id_in_a_comment_names_nothing() {
    let found = constrained(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         -- COMPOSITION c[openEHR-EHR-COMPOSITION.admin.v1]\n",
    );
    assert!(found.is_empty(), "{found:?}");
    assert!(
        !found.every_root_bound(),
        "the composition is bound by nothing"
    );
}

#[test]
fn an_archetype_id_in_a_string_compared_with_another_path_names_nothing() {
    let found = constrained(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/name/value = 'openEHR-EHR-COMPOSITION.admin.v1'",
    );
    assert!(found.is_empty(), "{found:?}");
    assert!(!found.every_root_bound());
}

#[test]
fn a_negated_predicate_neither_names_nor_binds() {
    for aql in [
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE NOT c/archetype_node_id = 'openEHR-EHR-COMPOSITION.admin.v1'",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_node_id != 'openEHR-EHR-COMPOSITION.admin.v1'",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         NOT CONTAINS COMPOSITION d[openEHR-EHR-COMPOSITION.admin.v1]",
    ] {
        let found = constrained(aql);
        assert!(found.is_empty(), "{aql}: {found:?}");
        assert!(!found.every_root_bound(), "{aql}");
    }
}

#[test]
fn every_class_bound_by_its_predicate_its_condition_or_its_container_is_bound() {
    for aql in [
        "SELECT o/data FROM EHR e CONTAINS COMPOSITION c[openEHR-EHR-COMPOSITION.report.v1] \
         CONTAINS OBSERVATION o",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_details/template_id/value = 'Example Report.v1'",
        "SELECT c FROM EHR e CONTAINS VERSION v CONTAINS \
         COMPOSITION c[openEHR-EHR-COMPOSITION.report.v1]",
    ] {
        assert!(constrained(aql).every_root_bound(), "{aql}");
    }
}

#[test]
fn a_class_beside_or_under_an_unbound_one_leaves_the_query_unbound() {
    for aql in [
        "SELECT c, d FROM EHR e CONTAINS (COMPOSITION c[openEHR-EHR-COMPOSITION.report.v1] \
         AND COMPOSITION d)",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.lab_test.v1]",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_node_id = 'openEHR-EHR-COMPOSITION.a.v1' \
         OR c/archetype_node_id = 'openEHR-EHR-COMPOSITION.b.v1'",
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE c/archetype_details/template_id/value LIKE 'Lab*'",
    ] {
        assert!(!constrained(aql).every_root_bound(), "{aql}");
    }
}

#[test]
fn a_query_of_the_ehr_alone_is_bound_and_names_nothing() {
    let found = constrained("SELECT e/ehr_id/value FROM EHR e");
    assert!(found.every_root_bound());
    assert!(found.is_empty());
}

#[test]
fn a_patient_query_keeps_its_constraints_after_the_rewrite() {
    let found = constrained(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c[openEHR-EHR-COMPOSITION.report.v1] \
         WHERE e/ehr_status/subject/external_ref/id/value = '4711'",
    );
    assert_eq!(
        ids(found.archetypes()),
        ["openEHR-EHR-COMPOSITION.report.v1"]
    );
}
