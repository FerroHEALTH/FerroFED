// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 5, de-duplication and `DISTINCT`, over two FerroEHR nodes that each
//! hold a copy of the same hospital composition in the patient's EHR: the
//! default passes duplicates through, `DISTINCT`, `ORDER BY` and `LIMIT` are
//! applied at the Tier over the union, `OFFSET` is never pushed down, and an
//! undirected aggregate is recombined or refused while a directed one is
//! answered (§9.5, §10, §11.6, §16.3 track 5; N9, N13, N14, N15, N39; CP-8,
//! CP-9, CP-10, CP-32).
//!
//! The opt-in `object_id` dedup of an imported composition needs a node
//! holding a copy of another node's version, which the harness seed does
//! not create; it is scored over mock nodes.

use std::time::Duration;

use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::proxy::Fault;
use ferrofed_testkit::seed::{self, DemoComposition};
use http::StatusCode;
use serde::Deserialize;

use crate::e2e::scenario::{
    Federated, Options, Reply, clear, exchange, gateway_with, nobody_asked, patient_predicate,
    post_aql, queries,
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

/// The patient's compositions, projecting `select`, followed by `tail`.
fn compositions(select: &str, tail: &str) -> String {
    format!(
        "SELECT {select} FROM EHR e CONTAINS COMPOSITION c WHERE {} {tail}",
        patient_predicate()
    )
}

/// Asserts a `200` and returns the answer.
fn answered(reply: &Reply) -> Result<Federated, Box<dyn std::error::Error>> {
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    reply.federated()
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

    let plain = answered(
        &exchange(
            &app,
            post_aql(&compositions("c/name/value AS name", ""), &[])?,
        )
        .await?,
    )?;
    assert_eq!(2, plain.rows.len(), "CP-9: one row per node");
    assert_eq!(
        plain.rows.first(),
        plain.rows.get(1),
        "CP-9: the default passes the duplicate through"
    );

    let distinct = answered(
        &exchange(
            &app,
            post_aql(&compositions("DISTINCT c/name/value AS name", ""), &[])?,
        )
        .await?,
    )?;
    assert_eq!(
        plain.rows.first().map(|row| vec![row.clone()]),
        Some(distinct.rows),
        "CP-8: DISTINCT folds the copies into one row at the Tier"
    );
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
    let uid = "c/uid/value AS uid";

    let union =
        answered(&exchange(&app, post_aql(&compositions(uid, ""), &[])?).await?)?.sorted_rows();
    assert_eq!(2, union.len(), "one composition per node");
    let (first, last) = (union.first().cloned(), union.last().cloned());

    let top = |tail: &str| compositions(uid, tail);
    for (tail, expected) in [
        ("ORDER BY c/uid/value ASC LIMIT 1", first.clone()),
        ("ORDER BY c/uid/value DESC LIMIT 1", last.clone()),
        ("ORDER BY c/uid/value ASC LIMIT 1 OFFSET 1", last.clone()),
    ] {
        clear(&nodes);
        let rows = answered(&exchange(&app, post_aql(&top(tail), &[])?).await?)?.rows;
        assert_eq!(
            expected.clone().into_iter().collect::<Vec<_>>(),
            rows,
            "CP-32: {tail} over the union of both nodes"
        );
        for node in [&nodes.a, &nodes.b] {
            for capture in queries(node) {
                let body = String::from_utf8_lossy(&capture.body);
                assert!(
                    !body.contains("OFFSET"),
                    "CP-32: OFFSET is never pushed down: {body}"
                );
            }
        }
    }

    for slow in [&nodes.a, &nodes.b] {
        slow.proxy
            .set_fault(Fault::Delay(Duration::from_millis(1_500)));
        let rows = answered(
            &exchange(
                &app,
                post_aql(&top("ORDER BY c/uid/value ASC LIMIT 1"), &[])?,
            )
            .await?,
        )?
        .rows;
        slow.proxy.clear_fault();
        assert_eq!(
            first.clone().into_iter().collect::<Vec<_>>(),
            rows,
            "CP-32: the same top row whichever node answers last"
        );
    }
    Ok(())
}

/// A result set whose cells are counts.
#[derive(Debug, Deserialize)]
struct Counted {
    rows: Vec<Vec<u64>>,
}

// conformance: CP-10 CP-32 track-5
#[tokio::test]
async fn an_undirected_aggregate_is_recombined_or_refused_and_a_directed_one_answered() -> TestResult
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = twin_copies().await?;
    let count = compositions("COUNT(c/uid/value) AS n", "");

    let declared = tempfile::tempdir()?;
    let app = gateway_with(declared.path(), &nodes, &Options::default())?;
    let reply = exchange(&app, post_aql(&count, &[])?).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    crate::facade::schema::validate(&reply.text)?;
    assert_eq!(
        vec![vec![2]],
        serde_json::from_str::<Counted>(&reply.text)?.rows,
        "CP-10, CP-32: one cross-node-correct row, never one row per node"
    );

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
    let refused = exchange(&strict, post_aql(&count, &[])?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        refused.status,
        "CP-10: an undirected aggregate that is not cross-node-correct: {}",
        refused.text
    );
    assert_eq!(
        "undirected-aggregate",
        refused.code()?,
        "CP-32: refused with a reason"
    );
    nobody_asked(&nodes, "CP-32: no per-node aggregate rows are asked for");

    let directed = format!(
        "SELECT COUNT(c/uid/value) AS n FROM ENDPOINT [\"node-a-pub\"] CONTAINS EHR e \
         CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    );
    let single = exchange(&strict, post_aql(&directed, &[])?).await?;
    assert_eq!(StatusCode::OK, single.status, "CP-10: {}", single.text);
    assert_eq!(
        vec![vec![1]],
        serde_json::from_str::<Counted>(&single.text)?.rows,
        "CP-10: a directed single-node aggregate is answered"
    );
    Ok(())
}
