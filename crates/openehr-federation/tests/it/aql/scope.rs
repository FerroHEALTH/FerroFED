// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two AQL forms that scope a query to one `ehr_id`: the `WHERE`
//! predicate and the `FROM EHR` class predicate, semantically equivalent
//! (N29, CP-22). Both are recognised as the same scope and reach a node as
//! the same query, the canonical `WHERE` form of §7.1, and neither lets the
//! patient identifier through (§5.4.1, N33).

use std::num::NonZeroUsize;

use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Context, Paging, Targeting, UnscopedQuery, analyse};
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;

use super::{EHR_ID, NAMESPACE, analysed, ask_all, assert_same_aql, node_aql, refused};

const WHERE_FORM: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                          WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398' \
                          AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'";

const FROM_FORM: &str = "SELECT c/uid/value \
                         FROM EHR e[ehr_id/value='7d44b88c-4199-4bad-97dc-d78268e01398'] \
                         CONTAINS COMPOSITION c \
                         WHERE c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'";

fn unscoped(aql: &str, context: &Context) -> UnscopedQuery {
    match analysed(aql, context) {
        Ok(Analysis::Unscoped(query)) => query,
        other => panic!("expected a query that names no patient, got {other:?}"),
    }
}

// conformance: CP-22
#[test]
fn both_forms_name_the_same_ehr_id_and_reach_a_node_as_the_same_query() {
    let where_form = unscoped(WHERE_FORM, &ask_all());
    let from_form = unscoped(FROM_FORM, &ask_all());
    assert_eq!(Some(EHR_ID), where_form.ehr_scope(), "N29: the WHERE form");
    assert_eq!(Some(EHR_ID), from_form.ehr_scope(), "N29: the FROM form");
    assert_eq!(
        where_form.node_query(),
        from_form.node_query(),
        "N29: the forms are semantically equivalent, and §7.1 dispatches the canonical one"
    );
    assert_same_aql(from_form.node_query().aql(), WHERE_FORM);
    let rendered = |query: &UnscopedQuery| -> Vec<(String, Option<String>)> {
        query
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.path.clone()))
            .collect()
    };
    assert_eq!(
        rendered(&where_form),
        rendered(&from_form),
        "the same columns[]"
    );
}

// conformance: CP-22
#[test]
fn an_unnamed_ehr_in_the_from_form_is_scoped_through_a_fresh_variable() {
    let aql =
        format!("SELECT c/uid/value FROM EHR[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c");
    let query = unscoped(&aql, &ask_all());
    assert_eq!(Some(EHR_ID), query.ehr_scope());
    assert_same_aql(
        query.node_query().aql(),
        &format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'"
        ),
    );
}

// conformance: CP-22
#[test]
fn a_bound_parameter_scopes_the_query_in_either_form() {
    let mut parameters = Parameters::new();
    parameters.insert("ehr_id", Primitive::String(EHR_ID.to_owned()));
    for aql in [
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = $ehr_id",
        "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_id] CONTAINS COMPOSITION c",
    ] {
        let analysis = analyse(aql, &parameters, Paging::default(), &ask_all());
        let Ok(Analysis::Unscoped(query)) = analysis else {
            panic!("{aql}: expected a query that names no patient, got {analysis:?}");
        };
        assert_eq!(Some(EHR_ID), query.ehr_scope(), "{aql}");
    }
}

#[test]
fn two_different_ehr_ids_scope_the_query_to_none_of_them() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c \
         WHERE e/ehr_id/value = '1111bbbb-1111-4111-8111-111111111111'"
    );
    let query = unscoped(&aql, &ask_all());
    assert!(query.ehr_scoped(), "an ehr_id predicate is still there");
    assert_eq!(None, query.ehr_scope(), "but no one ehr_id names the owner");
}

#[test]
fn an_ehr_id_under_or_scopes_nothing() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_id/value = '{EHR_ID}' OR c/name/value = 'Visit'"
    );
    assert_eq!(None, unscoped(&aql, &ask_all()).ehr_scope());
}

#[test]
fn a_scoped_query_has_a_node_set_where_a_localizer_is_configured() {
    let localized = Context::new(Targeting::Localized).with_default_namespace(NAMESPACE);
    assert_eq!(
        Some(EHR_ID),
        unscoped(FROM_FORM, &localized).ehr_scope(),
        "§12.5.1, N29: the ehr_id names its owner, so the node set is defined"
    );
    let unscoped_query = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";
    assert_eq!(
        Err(Refusal::NodeSetUndefined),
        analysed(unscoped_query, &localized).map(|_| ()),
        "a query with neither a patient nor an ehr_id still has none (N4)"
    );
    let directed = Context::new(Targeting::Directed {
        endpoints: NonZeroUsize::MIN,
    });
    assert_eq!(Some(EHR_ID), unscoped(WHERE_FORM, &directed).ehr_scope());
}

// conformance: CP-22 CP-26
#[test]
fn a_patient_query_in_the_from_form_dispatches_the_canonical_scope_and_no_identifier() {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '4711'"
    );
    let node = node_aql(&aql);
    assert!(!node.contains("4711"), "N33: {node}");
    assert_same_aql(
        &node,
        &format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_id/value = '{EHR_ID}' AND e/ehr_id/value = '{EHR_ID}'"
        ),
    );
}

// conformance: CP-22 CP-26
#[test]
fn the_patient_identifier_written_as_an_ehr_id_is_refused_in_either_form() {
    for aql in [
        "SELECT c/uid/value FROM EHR e[ehr_id/value='4711'] CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '4711'",
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_id/value = '4711' AND e/ehr_status/subject/external_ref/id/value = '4711'",
    ] {
        assert!(
            matches!(refused(aql), Refusal::IdentifierElsewhere { .. }),
            "§5.4.1, N33: {aql}"
        );
    }
}
