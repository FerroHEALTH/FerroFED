// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The differential run over twin copies: each node holds the patient's EHR
//! with the same hospital composition committed to it, the seed of track 5,
//! de-duplication and `DISTINCT`, `ORDER BY`, `LIMIT`, `OFFSET` and the
//! aggregates (§9.5, §10, §11.6, §16.3 track 5).

use std::error::Error;

use ferrofed_testkit::containers;
use ferrofed_testkit::seed::{self, DemoComposition};

use crate::e2e::differential::observe::Shape;
use crate::e2e::differential::{Bench, Call, Step, differ};
use crate::e2e::scenario::{clear, patient_predicate};
use crate::e2e::{EHR_A, EHR_B, TestResult, plan};

/// The patient's compositions, projecting `select`, followed by `tail`.
fn compositions(select: &str, tail: &str) -> String {
    format!(
        "SELECT {select} FROM EHR e CONTAINS COMPOSITION c WHERE {} {tail}",
        patient_predicate()
    )
}

/// Returns a step of track 5.
fn step(
    id: &'static str,
    what: &'static str,
    aql: &str,
    shape: Shape,
) -> Result<Step, Box<dyn Error>> {
    Ok(Step {
        id,
        track: "5",
        what,
        call: Call::aql(aql)?,
        shape,
        fault: None,
    })
}

/// The steps of track 5.
fn steps() -> Result<Vec<Step>, Box<dyn Error>> {
    let uid = "c/uid/value AS uid";
    let count = "COUNT(c/uid/value) AS n";
    Ok(vec![
        step(
            "t5-duplicates",
            "duplicates pass through by default",
            &compositions("c/name/value AS name", ""),
            Shape::Rows,
        )?,
        step(
            "t5-distinct",
            "DISTINCT folds the copies at the Tier",
            &compositions("DISTINCT c/name/value AS name", ""),
            Shape::Rows,
        )?,
        step(
            "t5-order-ascending-limit",
            "ORDER BY ascending with LIMIT over the union",
            &compositions(uid, "ORDER BY c/uid/value ASC LIMIT 1"),
            Shape::OrderedRows,
        )?,
        step(
            "t5-order-descending-limit",
            "ORDER BY descending with LIMIT over the union",
            &compositions(uid, "ORDER BY c/uid/value DESC LIMIT 1"),
            Shape::OrderedRows,
        )?,
        step(
            "t5-offset",
            "LIMIT with OFFSET over the union, never pushed down",
            &compositions(uid, "ORDER BY c/uid/value ASC LIMIT 1 OFFSET 1"),
            Shape::OrderedRows,
        )?,
        step(
            "t5-undirected-count",
            "an undirected COUNT across both nodes",
            &compositions(count, ""),
            Shape::Rows,
        )?,
        step(
            "t5-directed-count",
            "a COUNT directed to node A",
            &format!(
                "SELECT {count} FROM ENDPOINT [\"node-a-pub\"] CONTAINS EHR e \
                 CONTAINS COMPOSITION c WHERE {}",
                patient_predicate()
            ),
            Shape::Rows,
        )?,
    ])
}

#[tokio::test]
async fn twin_copies_differ_only_as_adjudicated() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
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
    let bench = Box::pin(Bench::start(nodes)).await?;
    Box::pin(differ(&bench, "twin", steps()?)).await
}
