// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ORDER BY` with `LIMIT` re-applied at the Tier against mock nodes
//! (§11.6.1, N13, N39): the global top `n` from interleaved node answers, the
//! deterministic tie-break whatever order the nodes answer in, and a node
//! whose `n` rows disagree with the federation order reported `node-error`
//! (§11.1; no specification governs the order check: our own design), which
//! fails the query `424` under all-or-nothing and leaves the other nodes' rows
//! under best-effort (§11.4, N37). A page at `OFFSET k` is sliced from the
//! `k + n` rows each node was sent (§11.6.2).

use std::error::Error;
use std::time::Duration;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::{Completion, FederatedAnswer, Plan, Verdict};
use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_federation::outcome::{EndpointOutcome, ErrorDetail};
use openehr_federation::status::EndpointStatus;

use super::{TestResult, budget, federation, json, node, rows_text, run, validated_body};

/// A synthetic node query in the shape the rewrite writes: the key, then the
/// uid as the last `ORDER BY` key, under the client's `LIMIT n`.
const NODE_AQL: &str = "SELECT c/context/start_time/value, c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398' ORDER BY c/context/start_time/value ASC, c/uid/value ASC LIMIT 2";

/// A mock node answering `(key, uid)` rows after `delay`.
async fn ordered_node(rows: &[(i64, &str)], delay: Duration) -> Server {
    let rows: Vec<String> = rows
        .iter()
        .map(|(key, uid)| format!("[{key},\"{uid}\"]"))
        .collect();
    let body = format!(
        r##"{{"q":"node","columns":[{{"name":"#0"}},{{"name":"#1"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    node(json(200, &body).set_delay(delay)).await
}

/// The façade asked for `ORDER BY` node column 0 `LIMIT limit`, with the uid
/// in node column 1.
fn ordered_plan(endpoints: &[&str], limit: u64) -> Result<Plan, Box<dyn Error>> {
    let mut plan = Plan::new().ordered(ResultOrder::new(
        vec![SortKey::new(0, Direction::Ascending)],
        vec![1],
        Some(limit),
    ));
    for id in endpoints {
        plan = plan.dispatch(EndpointId::new(*id)?, NodeQuery::new(NODE_AQL))?;
    }
    Ok(plan)
}

fn record<'a>(
    answer: &'a FederatedAnswer,
    id: &str,
) -> Result<&'a EndpointOutcome, Box<dyn Error>> {
    Ok(answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.id().as_str() == id)
        .ok_or_else(|| format!("no record for {id}"))?)
}

fn error_text(record: &EndpointOutcome) -> Result<&str, Box<dyn Error>> {
    match record.outcome().error() {
        Some(ErrorDetail::Text(text)) => Ok(text),
        other => Err(format!("an unexpected error: {other:?}").into()),
    }
}

// conformance: CP-8 CP-32
#[tokio::test]
async fn interleaved_node_answers_merge_into_the_global_top_n() -> TestResult {
    let a = ordered_node(&[(1, "a1"), (4, "a2")], Duration::ZERO).await;
    let b = ordered_node(&[(2, "b1"), (3, "b2")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let plan = ordered_plan(&["node-a-pub", "node-b-pub"], 2)?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(answer.federation().complete());
    assert_eq!(
        rows_text(&answer)?,
        r#"[[1,"a1"],[2,"b1"]]"#,
        "N39: the top 2 of the union, never node-a's first 2 then node-b's"
    );
    assert_eq!(
        record(&answer, "node-a-pub")?.row_count(),
        Some(2),
        "§9.5: the rows the endpoint contributed before the Tier LIMIT"
    );
    assert_eq!(record(&answer, "node-b-pub")?.row_count(), Some(2));
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn tied_keys_merge_identically_whichever_node_answers_first() -> TestResult {
    let slow = Duration::from_millis(150);
    let mut answers = Vec::new();
    for (a_delay, b_delay) in [(slow, Duration::ZERO), (Duration::ZERO, slow)] {
        let a = ordered_node(&[(5, "a1"), (5, "a2")], a_delay).await;
        let b = ordered_node(&[(5, "b1")], b_delay).await;
        let snapshot = federation(&[("node-b-pub", &b.uri()), ("node-a-pub", &a.uri())])?;
        let plan = ordered_plan(&["node-b-pub", "node-a-pub"], 3)?;
        let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
        assert_eq!(answer.status(), StatusCode::OK);
        answers.push(rows_text(&answer)?);
    }
    assert_eq!(
        answers, [r#"[[5,"a1"],[5,"a2"],[5,"b1"]]"#; 2],
        "§11.6.1: ties break on endpoint_id, then uid, whatever the arrival order"
    );
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn a_cut_out_of_the_federation_order_fails_the_query_424() -> TestResult {
    let disagreeing = ordered_node(&[(3, "a1"), (1, "a2")], Duration::ZERO).await;
    let b = ordered_node(&[(2, "b1")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &disagreeing.uri()), ("node-b-pub", &b.uri())])?;
    let plan = ordered_plan(&["node-a-pub", "node-b-pub"], 2)?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.verdict(), Verdict::NodeFailed);
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY, "N37");
    assert!(!answer.federation().complete());
    assert!(answer.rows().is_empty(), "a failing query returns no rows");
    let refused = record(&answer, "node-a-pub")?;
    assert_eq!(
        refused.status(),
        EndpointStatus::NodeError,
        "§11.1: a node out of the Tier order"
    );
    assert!(
        refused.outcome().latency_ms().is_some(),
        "N40: it was dispatched"
    );
    assert_eq!(refused.row_count(), None, "none of its rows were used");
    assert_eq!(
        error_text(refused)?,
        "result order disagrees with the federation order"
    );
    assert_eq!(
        record(&answer, "node-b-pub")?.status(),
        EndpointStatus::Active
    );
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn a_node_answering_past_its_limit_fails_the_query_424() -> TestResult {
    let over = ordered_node(&[(1, "x"), (2, "y")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &over.uri())])?;
    let answer = run(
        &snapshot,
        ordered_plan(&["node-a-pub"], 1)?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY);
    assert_eq!(
        error_text(record(&answer, "node-a-pub")?)?,
        "the node returned more rows than the LIMIT it was sent",
        "N39: the node was sent LIMIT 1"
    );
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn under_best_effort_a_disagreeing_node_leaves_the_others_top_n() -> TestResult {
    let disagreeing = ordered_node(&[(3, "a1"), (1, "a2")], Duration::ZERO).await;
    let b = ordered_node(&[(2, "b1"), (4, "b2")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &disagreeing.uri()), ("node-b-pub", &b.uri())])?;
    let plan = ordered_plan(&["node-a-pub", "node-b-pub"], 2)?.completing(Completion::BestEffort);
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.status(), StatusCode::OK, "§11.4 best-effort");
    assert!(!answer.federation().complete());
    assert_eq!(
        record(&answer, "node-a-pub")?.status(),
        EndpointStatus::NodeError
    );
    assert_eq!(
        rows_text(&answer)?,
        r#"[[2,"b1"],[4,"b2"]]"#,
        "the answering node's top 2, none of the refused node's rows"
    );
    validated_body(answer)?;
    Ok(())
}

/// The façade asked for `ORDER BY` node column 0 `LIMIT n OFFSET k`: every
/// node was sent `LIMIT k + n` (`window`) and the Tier skips `k` (§11.6.2).
fn paged_plan(endpoints: &[&str], window: u64, offset: u64) -> Result<Plan, Box<dyn Error>> {
    let mut plan = Plan::new().ordered(
        ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            vec![1],
            Some(window),
        )
        .with_offset(offset),
    );
    for id in endpoints {
        plan = plan.dispatch(EndpointId::new(*id)?, NodeQuery::new(NODE_AQL))?;
    }
    Ok(plan)
}

// conformance: CP-32
#[tokio::test]
async fn a_bounded_offset_page_is_sliced_from_k_plus_n_rows_per_node() -> TestResult {
    let a = ordered_node(
        &[(1, "a1"), (4, "a2"), (5, "a3"), (8, "a4")],
        Duration::ZERO,
    )
    .await;
    let b = ordered_node(&[(2, "b1"), (3, "b2"), (6, "b3")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let plan = paged_plan(&["node-a-pub", "node-b-pub"], 4, 2)?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        rows_text(&answer)?,
        r#"[[3,"b2"],[4,"a2"]]"#,
        "§11.6.2: LIMIT 2 OFFSET 2 is rows 2 and 3 of the merged order"
    );
    assert_eq!(
        record(&answer, "node-a-pub")?.row_count(),
        Some(4),
        "§9.5: the k + n rows the endpoint contributed"
    );
    assert_eq!(record(&answer, "node-b-pub")?.row_count(), Some(3));
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn a_bounded_window_out_of_the_federation_order_fails_the_query_424() -> TestResult {
    let disagreeing = ordered_node(&[(1, "a1"), (5, "a2"), (4, "a3")], Duration::ZERO).await;
    let b = ordered_node(&[(2, "b1")], Duration::ZERO).await;
    let snapshot = federation(&[("node-a-pub", &disagreeing.uri()), ("node-b-pub", &b.uri())])?;
    let plan = paged_plan(&["node-a-pub", "node-b-pub"], 3, 1)?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY, "N37");
    assert!(answer.rows().is_empty());
    let refused = record(&answer, "node-a-pub")?;
    assert_eq!(
        refused.status(),
        EndpointStatus::NodeError,
        "§11.1: a node out of the Tier order"
    );
    assert_eq!(
        error_text(refused)?,
        "result order disagrees with the federation order"
    );
    validated_body(answer)?;
    Ok(())
}
