// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The default all-or-nothing strategy: `504` over `424`, the `not-resolved`
//! carve-out, the overall budget, and a failing answer that still carries the
//! envelope (§11.3 to §11.5, N6, N37, N38).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::REQUEST_ID_HEADER;
use ferrofed_engine::fanout::{FanOutError, Plan, TIMEOUT_POLICY, Verdict, fan_out};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;

use super::{
    TestResult, budget, clients, federation, json, node, plan_for, result_set, rows_text, run,
    statuses, validated_body,
};

#[tokio::test]
async fn every_node_active_is_a_200_with_the_rows_in_endpoint_order() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let b = node(json(
        200,
        &result_set(&["b1::cdr-1.example.org::1", "b2::cdr-1.example.org::1"]),
    ))
    .await;
    let snapshot = federation(&[("node-b-pub", &b.uri()), ("node-a-pub", &a.uri())])?;
    let answer = run(
        &snapshot,
        plan_for(&["node-b-pub", "node-a-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Answered);
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(answer.federation().complete());
    assert_eq!(
        rows_text(&answer)?,
        r#"[["a1::cdr-0.example.org::1"],["b1::cdr-1.example.org::1"],["b2::cdr-1.example.org::1"]]"#,
        "rows follow endpoint id order, not the plan's"
    );
    let records = answer.federation().endpoints();
    let counts: Vec<Option<u64>> = records
        .iter()
        .map(openehr_federation::outcome::EndpointOutcome::row_count)
        .collect();
    assert_eq!(counts, [Some(1), Some(2)]);
    for record in records {
        assert!(record.outcome().latency_ms().is_some(), "{record:?}");
        assert_eq!(record.organisation(), Some("org-a"));
    }
    assert_eq!(
        records.first().and_then(|record| record.system_id()),
        Some("cdr-1.example.org")
    );
    let timeout = answer
        .federation()
        .timeout()
        .ok_or("meta.federation.timeout is missing")?;
    assert_eq!(timeout.per_node_ms, Some(2_000));
    assert_eq!(timeout.overall_ms, Some(5_000));
    assert_eq!(timeout.policy.as_deref(), Some(TIMEOUT_POLICY));
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-30 CP-31
#[tokio::test]
async fn one_node_timing_out_fails_the_query_504_with_the_envelope() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let slow = node(
        json(200, &result_set(&["s1::cdr-1.example.org::1"])).set_delay(Duration::from_secs(3)),
    )
    .await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-s-pub", &slow.uri())])?;
    let answer = run(
        &snapshot,
        plan_for(&["node-a-pub", "node-s-pub"])?,
        budget(300, 2_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::GATEWAY_TIMEOUT);
    assert!(!answer.federation().complete());
    assert_eq!(
        statuses(&answer),
        BTreeMap::from([
            ("node-a-pub".to_owned(), EndpointStatus::Active),
            ("node-s-pub".to_owned(), EndpointStatus::TimeOut),
        ])
    );
    assert!(answer.rows().is_empty(), "a failing query returns no rows");
    let slow_record = answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.status() == EndpointStatus::TimeOut)
        .ok_or("the slow node is not reported")?;
    assert!(slow_record.outcome().error().is_some());
    assert!(
        slow_record.row_count().is_none(),
        "an unresponsive node contributes no rows"
    );
    let body = validated_body(answer)?;
    assert!(body.contains("\"complete\":false"), "{body}");
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn one_node_error_fails_the_query_424_with_the_nodes_error() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let broken = node(json(500, r#"{"message":"the store is not available"}"#)).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-e-pub", &broken.uri())])?;
    let answer = run(
        &snapshot,
        plan_for(&["node-a-pub", "node-e-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::NodeFailed);
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY);
    assert!(!answer.federation().complete());
    assert!(answer.rows().is_empty());
    let error = answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.status() == EndpointStatus::NodeError)
        .and_then(|record| record.outcome().error())
        .ok_or("the node error carries no error")?;
    let ErrorDetail::Text(text) = error else {
        return Err(format!("an unexpected structured error: {error:?}").into());
    };
    assert!(text.contains("500"), "{text}");
    assert!(text.contains("the store is not available"), "{text}");
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn a_time_out_and_a_node_error_together_are_a_504() -> TestResult {
    let slow = node(json(200, &result_set(&[])).set_delay(Duration::from_secs(3))).await;
    let broken = node(json(503, "")).await;
    let snapshot = federation(&[("node-e-pub", &broken.uri()), ("node-s-pub", &slow.uri())])?;
    let answer = run(
        &snapshot,
        plan_for(&["node-e-pub", "node-s-pub"])?,
        budget(300, 2_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Unanswered);
    assert_eq!(answer.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(
        statuses(&answer),
        BTreeMap::from([
            ("node-e-pub".to_owned(), EndpointStatus::NodeError),
            ("node-s-pub".to_owned(), EndpointStatus::TimeOut),
        ])
    );
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn an_unreachable_node_is_offline_and_a_504() -> TestResult {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let refused = format!("http://{}", listener.local_addr()?);
    drop(listener);
    let a = node(json(200, &result_set(&[]))).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-o-pub", &refused)])?;
    let answer = run(
        &snapshot,
        plan_for(&["node-a-pub", "node-o-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(
        statuses(&answer).get("node-o-pub"),
        Some(&EndpointStatus::Offline)
    );
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn every_node_not_resolved_is_a_200_with_no_rows_and_incomplete() -> TestResult {
    let snapshot = federation(&[
        ("node-a-pub", "https://cdr-a.example.org/openehr"),
        ("node-b-pub", "https://cdr-b.example.org/openehr"),
    ])?;
    let not_resolved = || Outcome::NotResolved {
        error: ErrorDetail::Text("the cross-reference holds no ehr_id here".to_owned()),
    };
    let plan = Plan::new()
        .settle(EndpointId::new("node-a-pub")?, not_resolved())?
        .settle(EndpointId::new("node-b-pub")?, not_resolved())?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.verdict(), Verdict::Answered);
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "a patient found nowhere is not a 404 or a 424"
    );
    assert!(answer.rows().is_empty());
    assert!(!answer.federation().complete());
    for record in answer.federation().endpoints() {
        assert_eq!(record.status(), EndpointStatus::NotResolved);
        assert!(
            record.outcome().latency_ms().is_none(),
            "nothing was dispatched"
        );
    }
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn a_not_resolved_node_beside_an_active_one_keeps_the_rows() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let snapshot = federation(&[
        ("node-a-pub", &a.uri()),
        ("node-b-pub", "https://cdr-b.example.org/openehr"),
    ])?;
    let plan = plan_for(&["node-a-pub"])?.settle(
        EndpointId::new("node-b-pub")?,
        Outcome::NotResolved {
            error: ErrorDetail::Text("the cross-reference holds no ehr_id here".to_owned()),
        },
    )?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(!answer.federation().complete());
    assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-31
#[tokio::test]
async fn a_node_answering_after_the_overall_budget_contributes_nothing() -> TestResult {
    let fast = node(json(200, &result_set(&["f1::cdr-0.example.org::1"]))).await;
    let late = node(
        json(200, &result_set(&["l1::cdr-1.example.org::1"]))
            .set_delay(Duration::from_millis(1_500)),
    )
    .await;
    let snapshot = federation(&[("node-f-pub", &fast.uri()), ("node-l-pub", &late.uri())])?;
    let started = Instant::now();
    let answer = run(
        &snapshot,
        plan_for(&["node-f-pub", "node-l-pub"])?,
        budget(10_000, 400)?,
    )
    .await?;
    let waited = started.elapsed();
    assert!(
        waited < Duration::from_millis(1_200),
        "the fan-out waited {waited:?}, past the overall budget"
    );
    assert_eq!(
        statuses(&answer),
        BTreeMap::from([
            ("node-f-pub".to_owned(), EndpointStatus::Active),
            ("node-l-pub".to_owned(), EndpointStatus::TimeOut),
        ]),
        "abandoning the late node leaves the fast node's outcome alone"
    );
    let late_record = answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.status() == EndpointStatus::TimeOut)
        .ok_or("the late node is not reported")?;
    let latency = late_record
        .outcome()
        .latency_ms()
        .ok_or("the abandoned node carries no latency")?;
    assert!(
        (300..1_200).contains(&latency),
        "the abandoned node's latency is {latency} ms"
    );
    assert!(
        answer.rows().is_empty(),
        "the fast node's rows are not returned under all-or-nothing"
    );
    let asked = late
        .received_requests()
        .await
        .ok_or("request recording is off")?;
    assert_eq!(
        asked.len(),
        1,
        "the late node was asked once, then abandoned"
    );
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn the_request_id_reaches_every_node() -> TestResult {
    let a = node(json(200, &result_set(&[]))).await;
    let b = node(json(200, &result_set(&[]))).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    run(
        &snapshot,
        plan_for(&["node-a-pub", "node-b-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    for server in [&a, &b] {
        let requests = server
            .received_requests()
            .await
            .ok_or("request recording is off")?;
        assert_eq!(requests.len(), 1, "one request per node, no retry");
        let id = requests
            .first()
            .and_then(|request| request.headers.get(REQUEST_ID_HEADER))
            .and_then(|value| value.to_str().ok());
        assert_eq!(id, Some("req-fanout-1"));
    }
    Ok(())
}

#[tokio::test]
async fn a_plan_naming_an_endpoint_the_registry_lacks_is_refused() -> TestResult {
    let snapshot = federation(&[("node-a-pub", "https://cdr-a.example.org/openehr")])?;
    let refused = fan_out(
        &clients(&snapshot)?,
        &snapshot,
        plan_for(&["node-x-pub"])?,
        budget(2_000, 5_000)?,
        None,
    )
    .await;
    assert!(
        matches!(refused, Err(FanOutError::UnknownEndpoint { .. })),
        "{refused:?}"
    );
    Ok(())
}
