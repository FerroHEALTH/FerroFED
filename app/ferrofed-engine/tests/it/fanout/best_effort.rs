// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Best-effort completion, and the status mixes whose answer is the same under
//! either strategy: a failure is reported and the answering nodes' rows come
//! back with a `200` and `complete: false`, while the carve-outs and the scope
//! rule hold in both modes (§11.1, §11.3, §11.4, N6, N37).

use std::collections::BTreeMap;
use std::time::Duration;

use ferrofed_engine::fanout::{Completion, FederatedAnswer, Plan, Verdict};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;

use super::{
    TestResult, budget, federation, json, node, plan_for, result_set, rows_text, run, statuses,
    validated_body,
};

/// Both completion strategies, the default first.
const MODES: [Completion; 2] = [Completion::AllOrNothing, Completion::BestEffort];

/// A plan dispatching to `endpoints` under best-effort.
fn best_effort(endpoints: &[&str]) -> Result<Plan, Box<dyn std::error::Error>> {
    Ok(plan_for(endpoints)?.completing(Completion::BestEffort))
}

/// The record of `endpoint` in the answer, or an error naming it.
fn record_of<'a>(
    answer: &'a FederatedAnswer,
    endpoint: &str,
) -> Result<&'a EndpointOutcome, String> {
    answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.id().as_str() == endpoint)
        .ok_or_else(|| format!("{endpoint} is not reported"))
}

// conformance: CP-30
#[tokio::test]
async fn a_time_out_under_best_effort_is_a_200_with_the_answering_rows() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let slow = node(
        json(200, &result_set(&["s1::cdr-1.example.org::1"])).set_delay(Duration::from_secs(3)),
    )
    .await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-s-pub", &slow.uri())])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-a-pub", "node-s-pub"])?,
        budget(300, 2_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Partial);
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(!answer.federation().complete());
    assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
    let slow_record = record_of(&answer, "node-s-pub")?;
    assert_eq!(slow_record.status(), EndpointStatus::TimeOut);
    assert!(
        slow_record.outcome().error().is_some(),
        "the node is named with its error"
    );
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
async fn a_node_error_under_best_effort_is_a_200_naming_the_nodes_error() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let broken = node(json(500, r#"{"message":"the store is not available"}"#)).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-e-pub", &broken.uri())])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-a-pub", "node-e-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Partial);
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(!answer.federation().complete());
    assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
    let error = record_of(&answer, "node-e-pub")?
        .outcome()
        .error()
        .ok_or("the node error carries no error")?;
    let ErrorDetail::Text(text) = error else {
        return Err(format!("an unexpected structured error: {error:?}").into());
    };
    assert!(text.contains("the store is not available"), "{text}");
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn an_offline_node_under_best_effort_is_a_200_reporting_it() -> TestResult {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let refused = format!("http://{}", listener.local_addr()?);
    drop(listener);
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-o-pub", &refused)])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-a-pub", "node-o-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(!answer.federation().complete());
    assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
    assert_eq!(
        record_of(&answer, "node-o-pub")?.status(),
        EndpointStatus::Offline
    );
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn every_failure_together_under_best_effort_is_still_a_200() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let slow = node(json(200, &result_set(&[])).set_delay(Duration::from_secs(3))).await;
    let broken = node(json(503, "")).await;
    let snapshot = federation(&[
        ("node-a-pub", &a.uri()),
        ("node-e-pub", &broken.uri()),
        ("node-s-pub", &slow.uri()),
    ])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-a-pub", "node-e-pub", "node-s-pub"])?,
        budget(300, 2_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Partial);
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "no 504 or 424 under partial"
    );
    assert_eq!(
        statuses(&answer),
        BTreeMap::from([
            ("node-a-pub".to_owned(), EndpointStatus::Active),
            ("node-e-pub".to_owned(), EndpointStatus::NodeError),
            ("node-s-pub".to_owned(), EndpointStatus::TimeOut),
        ])
    );
    assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn no_node_answering_under_best_effort_is_a_200_with_no_rows() -> TestResult {
    let broken = node(json(500, "")).await;
    let slow = node(json(200, &result_set(&[])).set_delay(Duration::from_secs(3))).await;
    let snapshot = federation(&[("node-e-pub", &broken.uri()), ("node-s-pub", &slow.uri())])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-e-pub", "node-s-pub"])?,
        budget(300, 2_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Partial);
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(answer.rows().is_empty());
    assert!(!answer.federation().complete());
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn every_node_active_under_best_effort_is_a_complete_answer() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let b = node(json(200, &result_set(&["b1::cdr-1.example.org::1"]))).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        best_effort(&["node-a-pub", "node-b-pub"])?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.verdict(), Verdict::Answered);
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(answer.federation().complete(), "nothing was missing");
    assert_eq!(
        rows_text(&answer)?,
        r#"[["a1::cdr-0.example.org::1"],["b1::cdr-1.example.org::1"]]"#
    );
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn a_consent_denied_node_is_a_200_with_the_rest_in_either_mode() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let snapshot = federation(&[
        ("node-a-pub", &a.uri()),
        ("node-c-pub", "https://cdr-c.example.org/openehr"),
    ])?;
    for mode in MODES {
        let plan = plan_for(&["node-a-pub"])?
            .settle(
                EndpointId::new("node-c-pub")?,
                Outcome::ConsentDenied {
                    refused_by: ConsentRefusal::PreFilter,
                    error: None,
                },
            )?
            .completing(mode);
        let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
        assert_eq!(answer.verdict(), Verdict::Answered, "{mode:?}");
        assert_eq!(answer.status(), StatusCode::OK, "never a 424: {mode:?}");
        assert!(!answer.federation().complete(), "{mode:?}");
        assert_eq!(rows_text(&answer)?, r#"[["a1::cdr-0.example.org::1"]]"#);
        assert_eq!(
            record_of(&answer, "node-c-pub")?.status(),
            EndpointStatus::ConsentDenied
        );
        validated_body(answer)?;
    }
    Ok(())
}

// conformance: CP-12 CP-30
#[tokio::test]
async fn every_node_not_resolved_is_a_200_with_no_rows_in_either_mode() -> TestResult {
    let snapshot = federation(&[
        ("node-a-pub", "https://cdr-a.example.org/openehr"),
        ("node-b-pub", "https://cdr-b.example.org/openehr"),
    ])?;
    let not_resolved = || Outcome::NotResolved {
        error: ErrorDetail::Text("the cross-reference holds no ehr_id here".to_owned()),
    };
    for mode in MODES {
        let plan = Plan::new()
            .settle(EndpointId::new("node-a-pub")?, not_resolved())?
            .settle(EndpointId::new("node-b-pub")?, not_resolved())?
            .completing(mode);
        let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
        assert_eq!(answer.verdict(), Verdict::Answered, "{mode:?}");
        assert_eq!(answer.status(), StatusCode::OK, "found nowhere: {mode:?}");
        assert!(answer.rows().is_empty());
        assert!(!answer.federation().complete(), "{mode:?}");
        validated_body(answer)?;
    }
    Ok(())
}

// conformance: CP-30
#[tokio::test]
async fn members_out_of_scope_never_clear_complete_in_either_mode() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let snapshot = federation(&[
        ("node-a-pub", &a.uri()),
        ("node-l-pub", "https://cdr-l.example.org/openehr"),
        ("node-x-pub", "https://cdr-x.example.org/openehr"),
    ])?;
    for mode in MODES {
        let plan = plan_for(&["node-a-pub"])?
            .settle(
                EndpointId::new("node-l-pub")?,
                Outcome::NotLocalized { error: None },
            )?
            .settle(
                EndpointId::new("node-x-pub")?,
                Outcome::Excluded {
                    error: Some(ErrorDetail::Text("the member is suspended".to_owned())),
                },
            )?
            .completing(mode);
        let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
        assert_eq!(answer.verdict(), Verdict::Answered, "{mode:?}");
        assert_eq!(answer.status(), StatusCode::OK);
        assert!(
            answer.federation().complete(),
            "a member never in scope does not make the answer incomplete: {mode:?}"
        );
        assert_eq!(
            statuses(&answer),
            BTreeMap::from([
                ("node-a-pub".to_owned(), EndpointStatus::Active),
                ("node-l-pub".to_owned(), EndpointStatus::NotLocalized),
                ("node-x-pub".to_owned(), EndpointStatus::Excluded),
            ]),
            "every member is reported, in scope or not"
        );
        validated_body(answer)?;
    }
    Ok(())
}

/// §14.1 (`localizer-unavailable`): with every member `not-localized`, the
/// gateway dispatches to no node, `complete` stays `true`, the query does not
/// fail in either mode, and an outage shows only in `error`.
#[tokio::test]
async fn a_candidate_set_localization_left_empty_answers_two_hundred() -> TestResult {
    let snapshot = federation(&[
        ("node-a-pub", "https://cdr-a.example.org/openehr"),
        ("node-b-pub", "https://cdr-b.example.org/openehr"),
    ])?;
    let outage = ErrorDetail::Text("the localizer did not answer".to_owned());
    for error in [None, Some(outage)] {
        for mode in MODES {
            let plan = Plan::new()
                .settle(
                    EndpointId::new("node-a-pub")?,
                    Outcome::NotLocalized {
                        error: error.clone(),
                    },
                )?
                .settle(
                    EndpointId::new("node-b-pub")?,
                    Outcome::NotLocalized {
                        error: error.clone(),
                    },
                )?
                .completing(mode);
            assert!(!plan.has_no_destination(), "§14.1 is not the 404 of §11.2");
            let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
            assert_eq!(answer.status(), StatusCode::OK, "{mode:?}");
            assert!(answer.rows().is_empty());
            assert!(
                answer.federation().complete(),
                "no node in scope failed: {mode:?}"
            );
            validated_body(answer)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_failure_beside_an_out_of_scope_member_still_follows_the_mode() -> TestResult {
    let broken = node(json(500, "")).await;
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let snapshot = federation(&[
        ("node-a-pub", &a.uri()),
        ("node-e-pub", &broken.uri()),
        ("node-l-pub", "https://cdr-l.example.org/openehr"),
    ])?;
    let expected = [
        (Completion::AllOrNothing, StatusCode::FAILED_DEPENDENCY, 0),
        (Completion::BestEffort, StatusCode::OK, 1),
    ];
    for (mode, status, rows) in expected {
        let plan = plan_for(&["node-a-pub", "node-e-pub"])?
            .settle(
                EndpointId::new("node-l-pub")?,
                Outcome::NotLocalized { error: None },
            )?
            .completing(mode);
        let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
        assert_eq!(answer.status(), status, "{mode:?}");
        assert_eq!(answer.rows().len(), rows, "{mode:?}");
        assert!(!answer.federation().complete(), "{mode:?}");
        validated_body(answer)?;
    }
    Ok(())
}
