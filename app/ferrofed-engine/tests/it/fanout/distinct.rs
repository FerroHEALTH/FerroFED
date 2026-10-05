// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `SELECT DISTINCT` at the Tier against mock nodes (N13): a row two nodes
//! return is answered once, before the `LIMIT` (AQL 1.1.0 §LIMIT), while each
//! endpoint's `row_count` stays what it contributed (§9.5); a node that may
//! have been cut and returned two rows the Tier holds equal is reported
//! `node-error` (§11.1; no specification governs the check: our own design).

use std::error::Error;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::Plan;
use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_federation::status::EndpointStatus;

use super::{TestResult, budget, federation, json, node, rows_text, run, validated_body};

/// A synthetic node query in the shape the rewrite writes under `DISTINCT`:
/// the selected column is the key, and nothing is added.
const NODE_AQL: &str = "SELECT DISTINCT c/name/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398' ORDER BY c/name/value ASC LIMIT 2";

/// A mock node answering one-column rows, each cell as written.
async fn distinct_node(cells: &[&str]) -> Server {
    let rows: Vec<String> = cells.iter().map(|cell| format!("[{cell}]")).collect();
    let body = format!(
        r##"{{"q":"node","columns":[{{"name":"#0"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    node(json(200, &body)).await
}

/// `SELECT DISTINCT` over node column 0, ordered on it, `LIMIT 2`.
fn distinct_plan(endpoints: &[&str]) -> Result<Plan, Box<dyn Error>> {
    let mut plan = Plan::new().ordered(
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            Vec::new(),
            Some(2),
        )
        .with_distinct(vec![0]),
    );
    for id in endpoints {
        plan = plan.dispatch(EndpointId::new(*id)?, NodeQuery::new(NODE_AQL))?;
    }
    Ok(plan)
}

// conformance: CP-8 CP-32
#[tokio::test]
async fn a_row_two_nodes_return_takes_one_slot_of_the_limit() -> TestResult {
    let a = distinct_node(&["\"x\"", "\"z\""]).await;
    let b = distinct_node(&["\"x\"", "\"y\""]).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        distinct_plan(&["node-a-pub", "node-b-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        rows_text(&answer)?,
        r#"[["x"],["y"]]"#,
        "N13, AQL 1.1.0 §LIMIT: x once, then y, never x twice"
    );
    for id in ["node-a-pub", "node-b-pub"] {
        let record = answer
            .federation()
            .endpoints()
            .iter()
            .find(|record| record.id().as_str() == id)
            .ok_or("a record per endpoint")?;
        assert_eq!(
            record.row_count(),
            Some(2),
            "§9.5: {id} contributed 2 rows before the federation-level DISTINCT"
        );
    }
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-8
#[tokio::test]
async fn a_cut_node_returning_one_value_twice_fails_the_query_424() -> TestResult {
    let a = distinct_node(&["1", "1.0"]).await;
    let b = distinct_node(&["2"]).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        distinct_plan(&["node-a-pub", "node-b-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY, "N37");
    assert!(answer.rows().is_empty(), "a failing query returns no rows");
    let statuses: Vec<(String, EndpointStatus)> = answer
        .federation()
        .endpoints()
        .iter()
        .map(|record| (record.id().as_str().to_owned(), record.status()))
        .collect();
    assert_eq!(
        statuses,
        [
            ("node-a-pub".to_owned(), EndpointStatus::NodeError),
            ("node-b-pub".to_owned(), EndpointStatus::Active)
        ],
        "1 and 1.0 are one value, so node-a's LIMIT 2 can hide a second one"
    );
    validated_body(answer)?;
    Ok(())
}
