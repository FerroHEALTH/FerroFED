// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pure decision under each completion strategy, the plan's refusals and
//! the budget, with no node involved (§11.4, §11.5, N37, N38).

use std::time::Duration;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::{Budget, BudgetError, Completion, Plan, PlanError, Verdict, decide};
use ferrofed_registry::id::EndpointId;
use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn failure() -> ErrorDetail {
    ErrorDetail::Text("synthetic failure".to_owned())
}

fn record(id: &str, outcome: Outcome) -> Result<EndpointOutcome, Box<dyn std::error::Error>> {
    Ok(EndpointOutcome::new(
        openehr_federation::id::EndpointId::new(id)?,
        outcome,
    ))
}

fn active() -> Outcome {
    Outcome::Active { latency_ms: 5 }
}

fn time_out() -> Outcome {
    Outcome::TimeOut {
        latency_ms: 5,
        error: failure(),
    }
}

fn offline() -> Outcome {
    Outcome::Offline {
        latency_ms: 5,
        error: failure(),
    }
}

fn node_error() -> Outcome {
    Outcome::NodeError {
        latency_ms: 5,
        error: failure(),
    }
}

fn not_resolved() -> Outcome {
    Outcome::NotResolved { error: failure() }
}

fn consent_denied() -> Outcome {
    Outcome::ConsentDenied {
        refused_by: ConsentRefusal::PreFilter,
        error: None,
    }
}

fn decided(
    outcomes: Vec<Outcome>,
    completion: Completion,
) -> Result<Verdict, Box<dyn std::error::Error>> {
    let mut endpoints = Vec::new();
    for (index, outcome) in outcomes.into_iter().enumerate() {
        endpoints.push(record(&format!("node_{index}"), outcome)?);
    }
    Ok(decide(&endpoints, completion))
}

fn verdict_of(outcomes: Vec<Outcome>) -> Result<Verdict, Box<dyn std::error::Error>> {
    decided(outcomes, Completion::AllOrNothing)
}

fn best_effort(outcomes: Vec<Outcome>) -> Result<Verdict, Box<dyn std::error::Error>> {
    decided(outcomes, Completion::BestEffort)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_decision_table_of_all_or_nothing() -> TestResult {
    assert_eq!(verdict_of(vec![])?, Verdict::Answered);
    assert_eq!(verdict_of(vec![active(), active()])?, Verdict::Answered);
    assert_eq!(verdict_of(vec![active(), time_out()])?, Verdict::Unanswered);
    assert_eq!(verdict_of(vec![active(), offline()])?, Verdict::Unanswered);
    assert_eq!(
        verdict_of(vec![active(), node_error()])?,
        Verdict::NodeFailed
    );
    assert_eq!(
        verdict_of(vec![node_error(), time_out()])?,
        Verdict::Unanswered,
        "504 takes precedence over 424"
    );
    assert_eq!(
        verdict_of(vec![time_out(), node_error()])?,
        Verdict::Unanswered,
        "precedence does not depend on the order"
    );
    assert_eq!(
        verdict_of(vec![not_resolved(), not_resolved()])?,
        Verdict::Answered
    );
    assert_eq!(
        verdict_of(vec![
            active(),
            Outcome::ConsentDenied {
                refused_by: ConsentRefusal::PreFilter,
                error: None
            }
        ])?,
        Verdict::Answered
    );
    assert_eq!(
        verdict_of(vec![active(), Outcome::Excluded { error: None }])?,
        Verdict::Answered
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_decision_table_of_best_effort() -> TestResult {
    assert_eq!(
        best_effort(vec![active(), active()])?,
        Verdict::Answered,
        "nothing missing is a plain answer in either mode"
    );
    for missing in [time_out(), offline(), node_error()] {
        assert_eq!(
            best_effort(vec![active(), missing])?,
            Verdict::Partial,
            "a failure is reported, never a failed query"
        );
    }
    assert_eq!(
        best_effort(vec![node_error(), time_out()])?,
        Verdict::Partial
    );
    assert_eq!(
        best_effort(vec![active(), not_resolved(), consent_denied()])?,
        Verdict::Answered,
        "the answers of §11.3 are no failure in either mode"
    );
    assert_eq!(
        best_effort(vec![active(), Outcome::NotLocalized { error: None }])?,
        Verdict::Answered
    );
    Ok(())
}

#[test]
fn the_default_completion_is_all_or_nothing() {
    assert_eq!(Completion::default(), Completion::AllOrNothing);
}

#[test]
fn the_statuses_map_to_their_http_status() {
    assert_eq!(Verdict::Answered.status(), http::StatusCode::OK);
    assert_eq!(
        Verdict::Unanswered.status(),
        http::StatusCode::GATEWAY_TIMEOUT
    );
    assert_eq!(
        Verdict::NodeFailed.status(),
        http::StatusCode::FAILED_DEPENDENCY
    );
    assert_eq!(Verdict::Partial.status(), http::StatusCode::OK);
    assert!(!Verdict::Answered.failed());
    assert!(!Verdict::Partial.failed());
    assert!(Verdict::Unanswered.failed());
    assert!(Verdict::NodeFailed.failed());
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_plan_names_each_endpoint_once() -> TestResult {
    let endpoint = EndpointId::new("node-a-pub")?;
    let plan = Plan::new().dispatch(endpoint.clone(), NodeQuery::new("SELECT 1"))?;
    assert_eq!(
        plan.clone()
            .dispatch(endpoint.clone(), NodeQuery::new("SELECT 2"))
            .err(),
        Some(PlanError::Duplicate {
            endpoint: endpoint.clone()
        })
    );
    assert_eq!(
        plan.settle(endpoint.clone(), not_resolved()).err(),
        Some(PlanError::Duplicate { endpoint })
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_status_only_a_request_produces_is_never_settled() -> TestResult {
    for outcome in [active(), time_out(), offline(), node_error()] {
        let status = outcome.status();
        let endpoint = EndpointId::new("node-a-pub")?;
        assert_eq!(
            Plan::new().settle(endpoint.clone(), outcome).err(),
            Some(PlanError::DispatchedStatus { endpoint, status })
        );
    }
    let refused_by_node = Outcome::ConsentDenied {
        refused_by: ConsentRefusal::Node { latency_ms: 3 },
        error: None,
    };
    assert!(
        Plan::new()
            .settle(EndpointId::new("node-a-pub")?, refused_by_node)
            .is_err(),
        "a node's own consent refusal needs a request"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_budget_applies_both_timeouts() -> TestResult {
    assert_eq!(
        Budget::new(Duration::ZERO, Duration::from_secs(1)).err(),
        Some(BudgetError::Zero { which: "per-node" })
    );
    assert_eq!(
        Budget::new(Duration::from_secs(1), Duration::ZERO).err(),
        Some(BudgetError::Zero { which: "overall" })
    );
    let budget = Budget::new(Duration::from_secs(9), Duration::from_secs(2))?;
    assert_eq!(
        budget.per_node(),
        Duration::from_secs(2),
        "the per-node timeout never runs past the overall budget"
    );
    Ok(())
}
