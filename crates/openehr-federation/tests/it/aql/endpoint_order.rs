// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` on the directive's variable (§8.1, §9.3, §11.6.1, N13, N39):
//! a key on an ENDPOINT attribute the Tier evaluates, taken out of every node
//! query, placed among the client's keys where the client wrote it, while
//! the node keeps its other keys and its `LIMIT n`.

use std::num::{NonZeroU32, NonZeroUsize};

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Context, OffsetStrategy, Paging, Targeting, analyse};
use openehr_federation::attribute::EndpointAttribute;
use openehr_federation::dedup::DedupMode;
use openehr_federation::order::{Direction, SortKey};
use openehr_query::bind::Parameters;

use super::{EHR_ID, analysed, ask_all};

/// A deployment directed at two endpoints.
fn directed() -> Context {
    let endpoints = NonZeroUsize::new(2).expect("two endpoints");
    ask_all().with_targeting(Targeting::Directed { endpoints })
}

/// `columns` selected `FROM ENDPOINT p` over two endpoints for the fixture
/// patient, followed by `tail`.
fn patient_query(columns: &str, tail: &str) -> String {
    format!(
        r#"SELECT {columns} FROM ENDPOINT p ["node_1", "node_2"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '12345'{tail}"#
    )
}

/// The analysis of `aql` directed at two endpoints, which must be accepted.
fn accepted(aql: &str, context: &Context) -> Analysis {
    analysed(aql, context).unwrap_or_else(|refusal| panic!("{aql}: {refusal:?}"))
}

/// The node query of a patient analysis for the fixture `ehr_id`.
fn node_aql(analysis: &Analysis) -> String {
    let Analysis::Patient(query) = analysis else {
        panic!("expected a patient query, got {analysis:?}");
    };
    let ehr_id = HierObjectId::new(EHR_ID).expect("the fixture ehr_id is a HIER_OBJECT_ID");
    query.for_node(&ehr_id).aql().to_owned()
}

/// Asserts that `aql`, a node query, carries neither the directive nor a path
/// through its variable (§8.1, N7).
fn assert_no_directive(aql: &str) {
    for absent in ["ENDPOINT", "p/", "node_1", "node_2"] {
        assert!(!aql.contains(absent), "§8.1: {absent:?} in {aql}");
    }
}

// conformance: CP-37
#[test]
fn an_attribute_key_alone_is_the_tier_key_and_the_node_keeps_the_tie_break_and_limit() {
    for (tail, direction) in [
        (" ORDER BY p/id LIMIT 3", Direction::Ascending),
        (" ORDER BY p/id DESC LIMIT 3", Direction::Descending),
    ] {
        let analysis = accepted(
            &patient_query("p/id AS endpoint, c/uid/value", tail),
            &directed(),
        );
        assert_eq!(
            [SortKey::endpoint(0, direction)],
            analysis.order().keys(),
            "{tail}: §9.3, the attribute at its index in attributes()"
        );
        assert_eq!(
            vec![EndpointAttribute::EndpointId],
            analysis.attributes(),
            "{tail}"
        );
        assert_eq!([0_usize], analysis.order().tie_break(), "{tail}: the uid");
        assert_eq!(Some(3), analysis.order().limit(), "{tail}");
        let node = node_aql(&analysis);
        assert_no_directive(&node);
        assert!(
            node.contains("ORDER BY c/uid/value ASC LIMIT 3"),
            "{tail}: §11.6.1, the node is sent the tie-break and LIMIT n: {node}"
        );
    }
}

// conformance: CP-37
#[test]
fn an_attribute_key_keeps_its_place_among_the_node_keys() {
    let analysis = accepted(
        &patient_query(
            "p/organisation AS org, c/uid/value",
            " ORDER BY p/organisation DESC, c/uid/value DESC LIMIT 3",
        ),
        &directed(),
    );
    assert_eq!(
        [
            SortKey::endpoint(0, Direction::Descending),
            SortKey::new(0, Direction::Descending),
        ],
        analysis.order().keys()
    );
    let node = node_aql(&analysis);
    assert_no_directive(&node);
    assert!(
        node.contains("ORDER BY c/uid/value DESC LIMIT 3"),
        "§11.6.1: the node keeps the client's other key: {node}"
    );

    let analysis = accepted(
        &patient_query(
            "c/uid/value",
            " ORDER BY c/context/start_time/value, p/system_id DESC LIMIT 3",
        ),
        &directed(),
    );
    assert_eq!(
        [
            SortKey::new(1, Direction::Ascending),
            SortKey::endpoint(0, Direction::Descending),
        ],
        analysis.order().keys(),
        "the start time is a hidden node column, the attribute a Tier key after it"
    );
    assert_eq!(
        vec![EndpointAttribute::SystemId],
        analysis.attributes(),
        "an attribute ordered on and not selected is still read per endpoint"
    );
    let node = node_aql(&analysis);
    assert_no_directive(&node);
    assert!(
        node.contains("ORDER BY c/context/start_time/value, c/uid/value ASC LIMIT 3"),
        "{node}"
    );
}

// conformance: CP-37
#[test]
fn an_attribute_ordered_on_and_not_selected_follows_the_selected_ones() {
    let analysis = accepted(
        &patient_query(
            "p/url AS url, c/uid/value",
            " ORDER BY p/organization_id, p/url DESC",
        ),
        &directed(),
    );
    assert_eq!(
        vec![EndpointAttribute::Url, EndpointAttribute::Organisation],
        analysis.attributes()
    );
    assert_eq!(
        [
            SortKey::endpoint(1, Direction::Ascending),
            SortKey::endpoint(0, Direction::Descending),
        ],
        analysis.order().keys()
    );
    assert_no_directive(&node_aql(&analysis));
}

// conformance: CP-37
#[test]
fn an_unscoped_query_ordered_on_an_attribute_sends_no_directive() {
    let aql = r#"SELECT c/uid/value FROM ENDPOINT p ["node_1", "node_2"] CONTAINS EHR e CONTAINS COMPOSITION c ORDER BY p/id DESC LIMIT 4"#;
    let analysis = accepted(aql, &directed());
    let Analysis::Unscoped(query) = &analysis else {
        panic!("expected an unscoped query, got {analysis:?}");
    };
    let node = query.node_query().aql();
    assert_no_directive(node);
    assert!(node.contains("LIMIT 4"), "§11.6.1: {node}");
    assert_eq!(
        [SortKey::endpoint(0, Direction::Descending)],
        analysis.order().keys()
    );
}

#[test]
fn an_unknown_attribute_in_order_by_is_refused() {
    let refusal = analysed(
        &patient_query("c/uid/value", " ORDER BY p/name"),
        &directed(),
    )
    .expect_err("§9.3 lists no name attribute");
    assert!(
        matches!(refusal, Refusal::EndpointAttributeUnknown { at: Some(_) }),
        "{refusal:?}"
    );
}

#[test]
fn under_distinct_an_attribute_key_must_be_selected() {
    let refusal = analysed(
        &patient_query("DISTINCT c/uid/value", " ORDER BY p/id"),
        &directed(),
    )
    .expect_err("N13");
    assert!(
        matches!(refusal, Refusal::OrderNotSelected { at: Some(_) }),
        "{refusal:?}"
    );

    let analysis = accepted(
        &patient_query(
            "DISTINCT p/id AS endpoint, c/uid/value",
            " ORDER BY p/id DESC",
        ),
        &directed(),
    );
    assert_eq!(
        [SortKey::endpoint(0, Direction::Descending)],
        analysis.order().keys()
    );
}

#[test]
fn under_dedup_an_attribute_key_is_refused_with_a_limit_only() {
    let dedup = directed().with_dedup(DedupMode::VersionIdentity);
    let refusal = analysed(
        &patient_query("c/uid/value", " ORDER BY p/id LIMIT 2"),
        &dedup,
    )
    .expect_err("§10.2, §11.6.1: the copies of a version differ on the attribute");
    assert!(
        matches!(refusal, Refusal::EndpointOrderDedup { at: Some(_) }),
        "{refusal:?}"
    );
    assert_eq!("endpoint-order-dedup", refusal.kind());

    let analysis = accepted(&patient_query("c/uid/value", " ORDER BY p/id"), &dedup);
    assert_eq!(
        [SortKey::endpoint(0, Direction::Ascending)],
        analysis.order().keys(),
        "with no LIMIT every row reaches the Tier"
    );
}

#[test]
fn a_bounded_page_ordered_on_an_attribute_alone_is_ordered() {
    let window = NonZeroU32::new(100).expect("a window");
    let context = directed().with_offset_strategy(OffsetStrategy::Bounded { max_window: window });
    let analysis = analyse(
        &patient_query("c/uid/value", " ORDER BY p/id LIMIT 2 OFFSET 2"),
        &Parameters::new(),
        Paging::default(),
        &context,
    )
    .unwrap_or_else(|refusal| panic!("§11.6.2: {refusal:?}"));
    assert_eq!(Some(4), analysis.order().limit());
    assert_eq!(2, analysis.order().offset());
    let node = node_aql(&analysis);
    assert_no_directive(&node);
    assert!(node.contains("LIMIT 4"), "§11.6.2: {node}");
}

#[test]
fn a_directed_aggregate_ordered_on_an_attribute_needs_no_key() {
    let endpoints = NonZeroUsize::new(1).expect("one endpoint");
    let context = ask_all().with_targeting(Targeting::Directed { endpoints });
    let aql = r#"SELECT COUNT(c/uid/value) FROM ENDPOINT p ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '12345' ORDER BY p/id"#;
    let analysis = accepted(aql, &context);
    assert!(
        analysis.order().keys().is_empty(),
        "AQL 1.1.0 §Aggregate functions: one row, which no key reorders"
    );
    assert_no_directive(&node_aql(&analysis));
}
