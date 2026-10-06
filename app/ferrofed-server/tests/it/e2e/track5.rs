// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 5, de-duplication and `DISTINCT`, over two FerroEHR nodes that each
//! hold a copy of the same hospital composition in the patient's EHR: the
//! default passes duplicates through, `DISTINCT`, `ORDER BY` and `LIMIT` are
//! applied at the Tier over the union, `OFFSET` is never pushed down, and an
//! undirected aggregate is recombined or refused while a directed one is
//! answered (§9.5, §10, §11.6, §16.3 track 5; N9, N13, N14, N15, N39; CP-8,
//! CP-9, CP-10, CP-32).
//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track5`]); this suite adds
//! the node-side AQL and the delay that changes which node answers last.
//! The opt-in `object_id` dedup of an imported composition needs a node
//! holding a copy of another node's version, which the harness seed does
//! not create; it is scored over mock nodes.

use std::time::Duration;

use ferrofed_server::conformance::scenarios::track5;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::proxy::Fault;
use ferrofed_testkit::seed::{self, DemoComposition};
use http::StatusCode;

use crate::e2e::scenario::{
    Options, clear, exchange, fixture, gateway_with, in_process, nobody_asked, post_aql, queries,
};
use crate::e2e::{EHR_A, EHR_B, TestResult, plan};

/// The two nodes, each holding the patient's EHR with the same hospital
/// composition committed to it, and both journals cleared.
async fn twin_copies() -> Result<TwoNodes, Box<dyn std::error::Error>> {
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstHospital),
    )
    .await?;
    clear(&nodes);
    Ok(nodes)
}

// conformance: CP-8 CP-9 track-5
#[tokio::test]
async fn duplicates_pass_through_by_default_and_distinct_folds_them_at_the_tier() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = twin_copies().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    track5::duplicates_and_distinct(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    Ok(())
}

// conformance: CP-8 CP-32 track-5
#[tokio::test]
async fn order_by_and_limit_return_the_global_top_rows_deterministically() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = twin_copies().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;
    let fixture = fixture(Some(1), Some(1))?;

    let union = track5::order_and_limit(&in_process(&app), &fixture).await?;
    assert_eq!(2, union.len(), "one composition per node");
    for node in [&nodes.a, &nodes.b] {
        for capture in queries(node) {
            let body = String::from_utf8_lossy(&capture.body);
            assert!(
                !body.contains("OFFSET"),
                "CP-32: OFFSET is never pushed down: {body}"
            );
        }
    }

    let top = track5::compositions(
        &fixture,
        "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c \
         ORDER BY c/uid/value ASC LIMIT 1",
    )?;
    for slow in [&nodes.a, &nodes.b] {
        slow.proxy
            .set_fault(Fault::Delay(Duration::from_millis(1_500)));
        let reply = exchange(&app, post_aql(&top, &[])?).await?;
        slow.proxy.clear_fault();
        assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
        assert_eq!(
            union.first().cloned().into_iter().collect::<Vec<_>>(),
            reply.federated()?.rows,
            "CP-32: the same top row whichever node answers last"
        );
    }
    Ok(())
}

// conformance: CP-10 CP-32 track-5
#[tokio::test]
async fn an_undirected_aggregate_is_recombined_or_refused_and_a_directed_one_answered() -> TestResult
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = twin_copies().await?;
    let fixture = fixture(Some(1), Some(1))?;

    let declared = tempfile::tempdir()?;
    let app = gateway_with(declared.path(), &nodes, &Options::default())?;
    track5::aggregate_recombined(&in_process(&app), &fixture).await?;

    let undeclared = tempfile::tempdir()?;
    let strict = gateway_with(
        undeclared.path(),
        &nodes,
        &Options {
            federation: "decomposable_aggregates = []".to_owned(),
            ..Options::default()
        },
    )?;
    clear(&nodes);
    track5::aggregate_refused(&in_process(&strict), &fixture).await?;
    nobody_asked(&nodes, "CP-32: no per-node aggregate rows are asked for");

    track5::aggregate_directed(&in_process(&strict), &fixture, "node-a-pub").await?;
    Ok(())
}
