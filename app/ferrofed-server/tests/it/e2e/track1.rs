// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 1, transparency, over two FerroEHR nodes: an unmodified openEHR
//! client sends a plain patient AQL and gets one single-CDR-shaped
//! `RESULT_SET`, with no endpoint column it did not select, and `columns[]`
//! is the same however the nodes responded (§16.3 track 1; N1, N2, N17, N18;
//! CP-1, CP-2, CP-35).

//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track1`]); this suite adds
//! what only the nodes' capturing proxies and a delay injected at one can
//! show.

use std::error::Error;
use std::time::Duration;

use ferrofed_server::conformance::scenarios::track1;
use ferrofed_testkit::containers;
use ferrofed_testkit::proxy::Fault;
use http::StatusCode;

use crate::e2e::scenario::{
    Options, exchange, fixture, gateway_with, in_process, post_aql, queries, seed_both,
};
use crate::e2e::{EHR_A, EHR_B, TestResult, assert_no_patient_identifier_on_the_wire};

/// The plain patient query a client sends, its one column aliased.
fn plain_query() -> Result<String, Box<dyn Error>> {
    Ok(track1::plain_query(&fixture(Some(1), Some(1))?))
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

    let answer = track1::single_cdr_shaped(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        answer.statuses(),
        "CP-35: the additions nested under meta.federation"
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
    let b_first = exchange(&app, post_aql(&plain_query()?, &[])?).await?;
    nodes.a.proxy.clear_fault();
    nodes.b.proxy.set_fault(hold);
    let a_first = exchange(&app, post_aql(&plain_query()?, &[])?).await?;
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
