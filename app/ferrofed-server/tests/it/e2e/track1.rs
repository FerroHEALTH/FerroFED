// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 1, transparency, over two FerroEHR nodes: an unmodified openEHR
//! client sends a plain patient AQL and gets one single-CDR-shaped
//! `RESULT_SET`, with no endpoint column it did not select, and `columns[]`
//! is the same however the nodes responded (§16.3 track 1; N1, N2, N17, N18;
//! CP-1, CP-2, CP-35).

use std::time::Duration;

use axum::body::Body;
use ferrofed_testkit::containers;
use ferrofed_testkit::proxy::Fault;
use http::{Request, StatusCode};

use crate::e2e::scenario::{
    Options, exchange, gateway_with, patient_predicate, post_aql, queries, seed_both,
};
use crate::e2e::{EHR_A, EHR_B, TestResult, assert_no_patient_identifier_on_the_wire};

/// The plain patient query a client sends, its one column aliased.
fn plain_query() -> String {
    format!(
        "SELECT c/uid/value AS composition_uid FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    )
}

// conformance: CP-1 CP-2 CP-35 track-1
#[tokio::test]
async fn an_unmodified_client_gets_one_single_cdr_shaped_result_set() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    let posted = exchange(&app, post_aql(&plain_query(), &[])?).await?;
    assert_eq!(StatusCode::OK, posted.status, "CP-1: {}", posted.text);
    let answer = posted.federated()?;
    assert_eq!(
        vec!["composition_uid"],
        answer.names(),
        "CP-35: the client's columns and no endpoint column it did not select"
    );
    assert_eq!(
        2,
        answer.rows.len(),
        "one composition per node: {}",
        posted.text
    );
    assert!(
        answer
            .rows
            .iter()
            .all(|row| row.len() == answer.columns.len()),
        "CP-35: every row an ordered array matching columns[]"
    );
    assert!(
        answer.meta.complete.is_none() && answer.meta.endpoints.is_none(),
        "CP-35: no flat meta.complete or meta.endpoints"
    );
    for prefixed in ["\"_complete\"", "\"_endpoints\"", "\"_federation\""] {
        assert!(
            !posted.text.contains(prefixed),
            "CP-35: no prefixed {prefixed} member"
        );
    }
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        answer.statuses(),
        "CP-35: the additions nested under meta.federation"
    );
    for endpoint in ["node-a-pub", "node-b-pub"] {
        assert_eq!(
            Some(1),
            answer.endpoint(endpoint)?.row_count,
            "N16: each endpoint's row count"
        );
    }

    let encoded = crate::query_get::encoded(&plain_query());
    let get = Request::get(format!("/v1/query/aql?q={encoded}")).body(Body::empty())?;
    let fetched = exchange(&app, get).await?;
    assert_eq!(StatusCode::OK, fetched.status, "CP-1: {}", fetched.text);
    assert_eq!(
        answer.sorted_rows(),
        fetched.federated()?.sorted_rows(),
        "CP-1: the ITS-REST GET form answers as the POST form does"
    );

    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        for sent in queries(node) {
            let body = String::from_utf8_lossy(&sent.body);
            assert!(
                body.contains(&own.to_string()) && !body.contains(&other.to_string()),
                "N7: the node query is keyed on the node's own ehr_id: {body}"
            );
            assert!(
                !body.contains("subject"),
                "CP-2: no subject reaches a node: {body}"
            );
        }
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}

// conformance: CP-35 track-1
#[tokio::test]
async fn the_columns_are_the_same_whichever_node_answers_first() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;
    let hold = Fault::Delay(Duration::from_millis(1_500));

    nodes.a.proxy.set_fault(hold);
    let b_first = exchange(&app, post_aql(&plain_query(), &[])?).await?;
    nodes.a.proxy.clear_fault();
    nodes.b.proxy.set_fault(hold);
    let a_first = exchange(&app, post_aql(&plain_query(), &[])?).await?;
    nodes.b.proxy.clear_fault();

    assert_eq!(StatusCode::OK, b_first.status, "{}", b_first.text);
    assert_eq!(StatusCode::OK, a_first.status, "{}", a_first.text);
    let (b_first, a_first) = (b_first.federated()?, a_first.federated()?);
    assert_eq!(
        b_first.columns, a_first.columns,
        "CP-35: columns[] renders the client's AQL, whichever node answered first"
    );
    assert_eq!(
        b_first.sorted_rows(),
        a_first.sorted_rows(),
        "the same rows either way"
    );
    Ok(())
}
