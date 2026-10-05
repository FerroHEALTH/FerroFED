// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stored-query definition admitted to the registry of §12.7 (N44): the
//! analysis of a façade query over the text, the patient's origin, and the
//! canonical text the registry holds.

use openehr_federation::aql::definition::{Definition, SubjectOrigin};
use openehr_federation::aql::refusal::{Refusal, Unreducible};
use openehr_federation::aql::{Context, Targeting};

use super::{NAMESPACE, analysed, ask_all};

/// The patient predicate on `EHR_STATUS`, comparing with `operand`.
fn external_ref(operand: &str) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = {operand}"
    )
}

#[test]
fn a_patient_named_by_a_parameter_is_admitted_as_a_parameter() {
    let stored = Definition::admit(&external_ref("$patient"), &ask_all()).expect("admitted");
    assert_eq!(Some(SubjectOrigin::Parameter), stored.subject());
    assert!(
        stored.aql().contains("$patient"),
        "the parameter is held as written: {}",
        stored.aql()
    );
}

#[test]
fn a_patient_named_by_a_literal_is_reported_with_its_position() {
    let aql = external_ref("'4711'");
    let stored = Definition::admit(&aql, &ask_all()).expect("the text analyses");
    let Some(SubjectOrigin::Literal { at: Some(at) }) = stored.subject() else {
        panic!("a literal subject, located: {:?}", stored.subject());
    };
    let written = aql.get(at).expect("the range lies in the text");
    assert!(written.contains("'4711'"), "{written}");
}

#[test]
fn a_literal_spelled_as_a_placeholder_is_still_a_literal() {
    for spelled in [
        "'openEHR-EHR-CLUSTER.ferrofed_parameter_0_0.v1'",
        "'openEHR-EHR-CLUSTER.ferrofed_parameter_1_0.v1'",
    ] {
        let aql = format!("{} AND c/name/value = $name", external_ref(spelled));
        let stored = Definition::admit(&aql, &ask_all()).expect("the text analyses");
        assert!(
            matches!(stored.subject(), Some(SubjectOrigin::Literal { .. })),
            "{spelled}: {:?}",
            stored.subject()
        );
    }
}

#[test]
fn an_entry_level_subject_named_by_a_parameter_is_a_parameter() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o \
         WHERE o/subject/identifiers/id = $patient AND o/subject/identifiers/issuer = '{NAMESPACE}'"
    );
    let stored = Definition::admit(&aql, &ask_all()).expect("admitted");
    assert_eq!(
        Some(SubjectOrigin::Parameter),
        stored.subject(),
        "§5.4.3: both carriers on equal terms"
    );
}

#[test]
fn a_definition_without_a_patient_has_no_subject() {
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = $name";
    let stored = Definition::admit(aql, &ask_all()).expect("admitted");
    assert_eq!(None, stored.subject());
}

#[test]
fn the_held_text_is_the_canonical_print_without_a_comment() {
    let aql = format!("{} -- SENTINEL-COMMENT-7q", external_ref("$patient"));
    let stored = Definition::admit(&aql, &ask_all()).expect("admitted");
    assert!(
        !stored.aql().contains("SENTINEL-COMMENT-7q"),
        "§5.4.1: text the parser drops is never held: {}",
        stored.aql()
    );
    let again = Definition::admit(stored.aql(), &ask_all()).expect("the canonical text analyses");
    assert_eq!(stored, again, "the canonical print is a fixed point");
}

#[test]
fn the_directive_is_held_with_the_query() {
    let aql = "SELECT c/uid/value FROM ENDPOINT p [\"node-a-pub\"] CONTAINS EHR e \
               CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient";
    let stored = Definition::admit(aql, &ask_all()).expect("admitted");
    assert!(stored.aql().contains("ENDPOINT"), "{}", stored.aql());
    assert!(stored.aql().contains("node-a-pub"), "{}", stored.aql());
}

#[test]
fn a_parameter_in_an_archetype_predicate_is_admitted() {
    let aql = "SELECT o/uid/value FROM EHR e CONTAINS OBSERVATION o[$archetype] \
               WHERE e/ehr_status/subject/external_ref/id/value = $patient";
    let stored = Definition::admit(aql, &ask_all()).expect("an archetype parameter binds");
    assert_eq!(Some(SubjectOrigin::Parameter), stored.subject());
}

#[test]
fn text_that_is_not_aql_is_refused() {
    let refusal = Definition::admit("SELECT FROM WHERE", &ask_all()).expect_err("refused");
    assert!(matches!(refusal, Refusal::NotAql { .. }), "{refusal:?}");
}

#[test]
fn a_patient_predicate_the_rewrite_cannot_consume_is_refused_whatever_is_bound() {
    let aql = format!("{} OR c/name/value = 'Visit'", external_ref("$patient"));
    let refusal = Definition::admit(&aql, &ask_all()).expect_err("refused");
    assert!(
        matches!(
            refusal,
            Refusal::Unreducible {
                reason: Unreducible::NotConjunctive,
                ..
            }
        ),
        "§7.1: {refusal:?}"
    );
}

#[test]
fn a_patient_without_a_namespace_is_refused_where_no_default_is_declared() {
    let refusal = Definition::admit(&external_ref("$patient"), &Context::new(Targeting::AskAll))
        .expect_err("refused");
    assert_eq!(Refusal::NoNamespace, refusal, "§5.2");
}

#[test]
fn an_aggregate_is_left_to_the_targeting_of_each_invocation() {
    let aql = "SELECT COUNT(c/uid/value) FROM EHR e CONTAINS COMPOSITION c";
    let undirected = analysed(aql, &ask_all()).expect_err("no function is decomposable");
    assert_eq!("undirected-aggregate", undirected.kind());
    Definition::admit(aql, &ask_all())
        .expect("§11.6.3: one directed endpoint answers it unchanged (N14)");
}
