// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ENDPOINT attributes a directed query selects (§9.3, N12, N17, N18;
//! CP-35, CP-37): named by the rewrite, asked of no node, refused where they
//! collide with an EHR-derived column or would group an aggregate by
//! endpoint, and absent from a query that selects none.

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::refusal::{Indecomposable, Refusal};
use openehr_federation::aql::{Analysis, ColumnSource, Context, Targeting};
use openehr_federation::attribute::EndpointAttribute;

use super::{EHR_ID, analysed, ask_all};

/// The §9.4 example query, as the specification writes it.
const EXAMPLE: &str = r#"SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition_id FROM ENDPOINT p ["node_1","node_2"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '12345'"#;

/// A deployment directed at `endpoints` endpoints.
fn directed(endpoints: usize) -> Context {
    let endpoints = NonZeroUsize::new(endpoints).expect("a directive names an endpoint");
    ask_all().with_targeting(Targeting::Directed { endpoints })
}

/// `columns`, selected `FROM ENDPOINT p` over two endpoints for the fixture
/// patient.
fn selecting(columns: &str) -> String {
    format!(
        r#"SELECT {columns} FROM ENDPOINT p ["node_1", "node_2"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '12345'"#
    )
}

/// The names and paths of `columns[]`.
fn columns(analysis: &Analysis) -> Vec<(&str, Option<&str>)> {
    analysis
        .columns()
        .iter()
        .map(|column| (column.name.as_str(), column.path.as_deref()))
        .collect()
}

// conformance: CP-35 CP-37
#[test]
fn the_example_query_names_its_attributes_and_asks_no_node_for_them() {
    let analysis = analysed(EXAMPLE, &directed(2)).expect("§9.4");
    assert_eq!(
        vec![
            ("endpoint_id", Some("/id")),
            ("system_id", Some("/system_id")),
            ("composition_id", Some("/uid/value")),
        ],
        columns(&analysis),
        "§9.2: the gateway's rendering of the submitted AQL, in the ITS-REST path form"
    );
    assert_eq!(
        [
            ColumnSource::Endpoint(EndpointAttribute::EndpointId),
            ColumnSource::Endpoint(EndpointAttribute::SystemId),
            ColumnSource::Node(0),
        ],
        analysis.sources(),
        "N12: the attributes are the Tier's, the composition id the node's"
    );
    assert_eq!(
        vec![EndpointAttribute::EndpointId, EndpointAttribute::SystemId],
        analysis.attributes()
    );
    let Analysis::Patient(query) = analysis else {
        panic!("the example names a patient");
    };
    let node = query.for_node(&HierObjectId::new(EHR_ID).expect("a HIER_OBJECT_ID"));
    for absent in ["p/", "system_id", "ENDPOINT", "node_1", "12345"] {
        assert!(
            !node.aql().contains(absent),
            "§8.1, N33: the node query carries {absent:?}: {}",
            node.aql()
        );
    }
}

// conformance: CP-37
#[test]
fn every_attribute_the_table_lists_is_selectable() {
    for (path, attribute) in [
        ("p/id", EndpointAttribute::EndpointId),
        ("p/endpoint_id", EndpointAttribute::EndpointId),
        ("p/organisation", EndpointAttribute::Organisation),
        ("p/organization_id", EndpointAttribute::Organisation),
        ("p/system_id", EndpointAttribute::SystemId),
        ("p/url", EndpointAttribute::Url),
    ] {
        let analysis =
            analysed(&selecting(&format!("{path}, c/uid/value")), &directed(2)).expect(path);
        assert_eq!(
            [ColumnSource::Endpoint(attribute), ColumnSource::Node(0)],
            analysis.sources(),
            "§9.3: {path}"
        );
    }
}

#[test]
fn a_path_that_selects_no_listed_attribute_is_refused() {
    for path in [
        "p/name",
        "p/ID",
        "p",
        "p/id/value",
        "p/system_id[at0001]",
        "p[at0001]/id",
    ] {
        let refusal = analysed(&selecting(&format!("{path}, c/uid/value")), &directed(2))
            .expect_err("§9.3 lists the attributes");
        assert!(
            matches!(refusal, Refusal::EndpointAttributeUnknown { at: Some(_) }),
            "{path}: {refusal:?}"
        );
        assert_eq!("endpoint-attribute-unknown", refusal.kind());
    }
}

// conformance: CP-35 CP-37
#[test]
fn a_query_selecting_no_attribute_keeps_the_single_cdr_shape() {
    let analysis =
        analysed(&selecting("c/uid/value AS composition_id"), &directed(2)).expect("§8.1");
    assert_eq!(
        vec![("composition_id", Some("/uid/value"))],
        columns(&analysis)
    );
    assert_eq!([ColumnSource::Node(0)], analysis.sources(), "N17");
    assert!(analysis.attributes().is_empty());
}

// conformance: CP-35
#[test]
fn an_alias_resolves_the_collision_and_both_columns_survive() {
    let analysis = analysed(
        &selecting("p/system_id AS node_system, e/system_id/value AS system_id"),
        &directed(2),
    )
    .expect("N18: the alias resolves the collision");
    assert_eq!(
        vec![
            ("node_system", Some("/system_id")),
            ("system_id", Some("/system_id/value")),
        ],
        columns(&analysis)
    );
    assert_eq!(
        [
            ColumnSource::Endpoint(EndpointAttribute::SystemId),
            ColumnSource::Node(0),
        ],
        analysis.sources(),
        "CP-35: no column is shadowed"
    );
    let unaliased = analysed(&selecting("p/system_id, e/system_id/value"), &directed(2))
        .expect("an unaliased column is named by its index, so none collides");
    assert_eq!(
        vec![("#0", Some("/system_id")), ("#1", Some("/system_id/value")),],
        columns(&unaliased)
    );
}

// conformance: CP-35
#[test]
fn an_alias_that_leaves_the_collision_standing_is_refused() {
    for columns in [
        "p/system_id AS system_id, e/system_id/value AS system_id",
        "c/uid/value AS id, p/id AS id",
        "e/ehr_status/subject/external_ref/id/value AS who, p/url AS who",
    ] {
        let refusal = analysed(&selecting(columns), &directed(2))
            .expect_err("N18, CP-35: one name would denote two columns");
        assert!(
            matches!(refusal, Refusal::EndpointNameCollision { at: Some(_) }),
            "{columns}: {refusal:?}"
        );
        assert_eq!("endpoint-name-collision", refusal.kind());
    }
    assert!(
        analysed(
            &selecting("p/id AS x, p/system_id AS x, c/uid/value"),
            &directed(2)
        )
        .is_ok(),
        "N18 governs a collision with an EHR-derived column only"
    );
}

#[test]
fn an_attribute_beside_a_recombined_aggregate_is_refused() {
    let declared = directed(2).with_decomposable_aggregates(AggregateFunction::ALL);
    let refusal = analysed(&selecting("p/id, COUNT(c/uid/value)"), &declared)
        .expect_err("§11.6.3: the attribute groups the count by endpoint");
    assert!(
        matches!(
            refusal,
            Refusal::Indecomposable {
                reason: Indecomposable::PlainColumn,
                at: Some(_)
            }
        ),
        "{refusal:?}"
    );
    let one = r#"SELECT p/id, COUNT(c/uid/value) FROM ENDPOINT p ["node_1"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '12345'"#;
    let analysis = analysed(one, &directed(1)).expect("N14: a directed single-node aggregate");
    assert!(analysis.recombination().is_none());
    assert_eq!(
        [
            ColumnSource::Endpoint(EndpointAttribute::EndpointId),
            ColumnSource::Node(0),
        ],
        analysis.sources()
    );
}

// conformance: CP-37
#[test]
fn under_distinct_the_node_is_asked_only_its_own_columns() {
    let query = selecting("DISTINCT p/id, c/name/value") + " ORDER BY c/name/value LIMIT 5";
    let analysis = analysed(&query, &directed(2)).expect("N13");
    assert_eq!(
        Some(&[0_usize][..]),
        analysis.order().distinct(),
        "the node column of the name; the merge adds the attribute to the tuple"
    );
    assert_eq!(vec![EndpointAttribute::EndpointId], analysis.attributes());
}

// conformance: CP-37
#[test]
fn ordering_on_an_attribute_is_refused_with_or_without_a_limit() {
    for tail in [" ORDER BY p/id", " ORDER BY p/system_id DESC LIMIT 2"] {
        let refusal = analysed(&(selecting("p/id, c/uid/value") + tail), &directed(2))
            .expect_err("§9.3 makes the attributes selectable, and only that");
        assert!(
            matches!(refusal, Refusal::EndpointVariable { at: Some(_) }),
            "{tail}: {refusal:?}"
        );
    }
}
