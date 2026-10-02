// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node query under version-identity dedup (§10.2, N15, CP-9): every
//! node is asked the version uid of each row, the uid of the first
//! `COMPOSITION`, else of the first `VERSION`; a query with a `LIMIT` and no
//! `ORDER BY` is ordered on it; under `DISTINCT` nothing is added; and a
//! recombined aggregate is refused (§11.6.3). Under the default `none` the
//! node query is the one the client wrote.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::refusal::{Indecomposable, Refusal};
use openehr_federation::aql::{Analysis, Context, Targeting};
use openehr_federation::dedup::DedupMode;
use openehr_federation::order::{Direction, ResultOrder, SortKey};

use super::{EHR_ID, analysed, ask_all, assert_same_aql};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value = '4711'";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c";
const SCOPE: &str = "e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

fn deduplicating() -> Context {
    ask_all().with_dedup(DedupMode::VersionIdentity)
}

/// The node query and the order of the patient query `aql` under `context`.
fn patient(aql: &str, context: &Context) -> (String, ResultOrder) {
    match analysed(aql, context) {
        Ok(Analysis::Patient(query)) => {
            let node = query.for_node(&HierObjectId::new(EHR_ID).expect("a HIER_OBJECT_ID"));
            (
                node.aql().to_owned(),
                Analysis::Patient(query).order().clone(),
            )
        }
        other => panic!("{aql} names a patient: {other:?}"),
    }
}

// conformance: CP-9
#[test]
fn the_version_uid_is_asked_as_a_hidden_column() {
    let aql = format!("SELECT c/name/value {FROM} WHERE {SUBJECT}");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!("SELECT c/name/value, c/uid/value {FROM} WHERE {SCOPE}"),
    );
    assert_eq!(order.version_key(), Some(1), "§10.2: the dedup key");
    assert!(order.keys().is_empty(), "no LIMIT, so no order is imposed");
}

// conformance: CP-9
#[test]
fn a_selected_uid_is_read_where_it_is() {
    let aql = format!("SELECT c/uid/value, c/name/value {FROM} WHERE {SUBJECT}");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!("SELECT c/uid/value, c/name/value {FROM} WHERE {SCOPE}"),
    );
    assert_eq!(order.version_key(), Some(0));
}

// conformance: CP-9 CP-32
#[test]
fn a_limit_without_order_by_is_ordered_on_the_uid() {
    let aql = format!("SELECT c/name/value {FROM} WHERE {SUBJECT} LIMIT 5");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!(
            "SELECT c/name/value, c/uid/value {FROM} WHERE {SCOPE} ORDER BY c/uid/value ASC LIMIT 5"
        ),
    );
    assert_eq!(order.keys(), [SortKey::new(1, Direction::Ascending)]);
    assert_eq!(order.tie_break(), [1]);
    assert_eq!(order.version_key(), Some(1));
}

// conformance: CP-9 CP-32
#[test]
fn under_order_by_the_row_key_is_the_dedup_key() {
    let aql = format!("SELECT c/name/value {FROM} WHERE {SUBJECT} ORDER BY c/name/value LIMIT 3");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!(
            "SELECT c/name/value, c/uid/value {FROM} WHERE {SCOPE} ORDER BY c/name/value, c/uid/value ASC LIMIT 3"
        ),
    );
    assert_eq!(order.tie_break(), [1]);
    assert_eq!(order.version_key(), Some(1));
}

// conformance: CP-9
#[test]
fn a_version_variable_supplies_the_uid() {
    let aql = format!(
        "SELECT v/commit_audit/time_committed/value FROM EHR e CONTAINS VERSION v WHERE {SUBJECT}"
    );
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!(
            "SELECT v/commit_audit/time_committed/value, v/uid/value FROM EHR e CONTAINS VERSION v WHERE {SCOPE}"
        ),
    );
    assert_eq!(
        order.version_key(),
        Some(1),
        "§10.2: a version history keyed on the full id"
    );
}

// conformance: CP-9
#[test]
fn a_query_that_reads_no_version_has_no_dedup_key() {
    let aql = "SELECT e/ehr_id/value FROM EHR e";
    let analysis = analysed(aql, &deduplicating()).expect("accepted");
    assert_eq!(analysis.order().version_key(), None, "nothing to suppress");
    let Analysis::Unscoped(query) = analysis else {
        panic!("no patient");
    };
    assert_same_aql(query.node_query().aql(), aql);
}

// conformance: CP-9 CP-8
#[test]
fn under_distinct_an_unselected_uid_is_not_added() {
    let aql = format!("SELECT DISTINCT c/name/value {FROM} WHERE {SUBJECT}");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!("SELECT DISTINCT c/name/value {FROM} WHERE {SCOPE}"),
    );
    assert_eq!(
        order.version_key(),
        None,
        "N13: a hidden column would change which rows are distinct"
    );
}

// conformance: CP-9 CP-8
#[test]
fn under_distinct_a_selected_uid_is_the_dedup_key() {
    let aql = format!("SELECT DISTINCT c/uid/value, c/name/value {FROM} WHERE {SUBJECT} LIMIT 2");
    let (node, order) = patient(&aql, &deduplicating());
    assert_same_aql(
        &node,
        &format!(
            "SELECT DISTINCT c/uid/value, c/name/value {FROM} WHERE {SCOPE} ORDER BY c/uid/value ASC, c/name/value ASC LIMIT 2"
        ),
    );
    assert_eq!(order.version_key(), Some(0));
    assert_eq!(order.distinct(), Some([0, 1].as_slice()));
}

#[test]
fn under_the_default_mode_nothing_is_added() {
    let aql = format!("SELECT c/name/value {FROM} WHERE {SUBJECT} LIMIT 5");
    let (node, order) = patient(&aql, &ask_all());
    assert_same_aql(
        &node,
        &format!("SELECT c/name/value {FROM} WHERE {SCOPE} LIMIT 5"),
    );
    assert_eq!(order.version_key(), None, "§10.1, N15");
    assert_eq!(ask_all().dedup(), DedupMode::None);
}

// conformance: CP-9 CP-10
#[test]
fn a_recombined_aggregate_under_dedup_is_refused() {
    let context = deduplicating().with_decomposable_aggregates(AggregateFunction::ALL);
    let aql = format!("SELECT COUNT(*) {FROM} WHERE {SUBJECT}");
    let refusal = analysed(&aql, &context).expect_err("refused");
    assert!(
        matches!(
            refusal,
            Refusal::Indecomposable {
                reason: Indecomposable::Dedup,
                ..
            }
        ),
        "§11.6.3: not combined with de-duplication, got {refusal:?}"
    );
    assert_eq!(refusal.kind(), "indecomposable-aggregate");
    assert!(
        analysed(
            &aql,
            &ask_all().with_decomposable_aggregates(AggregateFunction::ALL)
        )
        .is_ok()
    );
}

// conformance: CP-9
#[test]
fn a_directed_single_node_aggregate_under_dedup_is_dispatched() {
    let one = NonZeroUsize::new(1).expect("one");
    let context =
        Context::new(Targeting::Directed { endpoints: one }).with_dedup(DedupMode::VersionIdentity);
    let aql = format!("SELECT COUNT(*) {FROM}");
    let analysis = analysed(&aql, &context).expect("a single node aggregate passes (N14)");
    assert!(analysis.recombination().is_none());
}
