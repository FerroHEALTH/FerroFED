// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 4, partial results, over two FerroEHR nodes with faults injected at
//! node B's capturing proxy: a node that answers an error, is unreachable or
//! is slow fails the query by default with the status of §11.2 and an
//! envelope naming it, `partial` returns the rest only where offered, a
//! client wait shortens the budget and never extends it, and the two
//! carve-outs of §11.3 answer `200` (§11, §16.3 track 4; N6, N16, N37, N38,
//! N40; CP-11, CP-12, CP-30, CP-31).
//!
//! The members the localizer did not name are scored in `e2e::track3`. The
//! found-nowhere checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track4`]).

use std::time::Duration;

use ferrofed_server::conformance::scenarios::track4;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::proxy::Fault;
use ferrofed_testkit::seed::PatientId;
use http::StatusCode;
use openehr_federation::headers::{COMPLETENESS, COMPLETENESS_PARTIAL};

use crate::e2e::scenario::{
    Options, Reply, asked, dev_rows, development, exchange, fixture, gateway_with, in_process,
    nobody_asked, patient_compositions, post_aql, seed_both, synthetic,
};
use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult};
use crate::support::{SLACK, millis};

/// The per-node timeout of the scenarios that wait for a slow node.
const PER_NODE: Duration = Duration::from_secs(4);

/// The overall budget of those scenarios.
const OVERALL: Duration = Duration::from_secs(6);

/// The `Prefer` request header (RFC 7240 §2).
const PREFER: &str = "prefer";

/// The `Preference-Applied` response header (RFC 7240 §3).
const PREFERENCE_APPLIED: &str = "preference-applied";

/// The two nodes, seeded, and a gateway over them with the budgets of
/// [`PER_NODE`] and [`OVERALL`] and `federation` among its keys.
async fn harness(
    dir: &std::path::Path,
    federation: &str,
) -> Result<(TwoNodes, axum::Router), Box<dyn std::error::Error>> {
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let options = Options {
        per_node_ms: millis(PER_NODE)?,
        overall_ms: millis(OVERALL)?,
        federation: federation.to_owned(),
        ..Options::default()
    };
    let app = gateway_with(dir, &nodes, &options)?;
    Ok((nodes, app))
}

/// Asserts that `reply` failed with `status`, carries the envelope naming
/// node B as `node_b` with its error and node A as active, and holds no row
/// (§11.2, N37).
fn failed_naming_b(reply: &Reply, status: StatusCode, node_b: &str) -> TestResult {
    assert_eq!(status, reply.status, "CP-12, CP-30: {}", reply.text);
    let answer = reply.federated()?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", node_b)],
        answer.statuses(),
        "CP-11, CP-30: the failing response names the failed node"
    );
    assert!(
        answer.endpoint("node-b-pub")?.error.is_some(),
        "CP-30: the failed node carries its error"
    );
    assert!(!answer.meta.federation.complete, "CP-30: complete is false");
    assert!(
        answer.rows.is_empty(),
        "CP-30: all-or-nothing returns no row"
    );
    Ok(())
}

// conformance: CP-11 CP-12 CP-30 track-4
#[tokio::test]
async fn a_node_answering_an_error_fails_the_query_424_and_is_named_node_error() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let (nodes, app) = harness(dir.path(), "").await?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    failed_naming_b(&reply, StatusCode::FAILED_DEPENDENCY, "node-error")
}

// conformance: CP-11 CP-12 CP-30 track-4
#[tokio::test]
async fn an_unreachable_node_fails_the_query_504_and_is_named_offline() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let (nodes, app) = harness(dir.path(), "").await?;
    nodes.b.proxy.set_fault(Fault::Refuse);

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    failed_naming_b(&reply, StatusCode::GATEWAY_TIMEOUT, "offline")
}

// conformance: CP-12 CP-30 CP-31 track-4
#[tokio::test]
async fn a_slow_node_is_abandoned_at_the_per_node_timeout_and_named_time_out() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let (nodes, app) = harness(dir.path(), "").await?;
    nodes.b.proxy.set_fault(Fault::Delay(PER_NODE + SLACK));

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    failed_naming_b(&reply, StatusCode::GATEWAY_TIMEOUT, "time-out")?;
    assert!(
        reply.took < OVERALL + SLACK,
        "CP-31: the overall budget is honoured, took {:?}",
        reply.took
    );
    let answer = reply.federated()?;
    for endpoint in ["node-a-pub", "node-b-pub"] {
        assert!(
            answer.endpoint(endpoint)?.latency_ms.is_some(),
            "CP-31: latency_ms is reported for {endpoint}"
        );
    }

    nodes
        .a
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));
    let both = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    assert_eq!(
        StatusCode::GATEWAY_TIMEOUT,
        both.status,
        "CP-30: one node-error and one time-out answer 504: {}",
        both.text
    );
    assert_eq!(
        vec![("node-a-pub", "node-error"), ("node-b-pub", "time-out")],
        both.federated()?.statuses()
    );
    Ok(())
}

// conformance: CP-31 track-4
#[tokio::test]
async fn a_client_wait_shortens_the_budget_and_never_extends_it() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let (nodes, app) = harness(dir.path(), "").await?;
    let wait = Duration::from_secs(2);
    nodes.b.proxy.set_fault(Fault::Delay(PER_NODE + SLACK));

    let shortened = exchange(
        &app,
        post_aql(&patient_compositions(), &[(PREFER, "wait=2")])?,
    )
    .await?;
    assert_eq!(
        StatusCode::GATEWAY_TIMEOUT,
        shortened.status,
        "{}",
        shortened.text
    );
    assert!(
        shortened.took < wait + SLACK,
        "CP-31: a shorter wait shortens the budget, took {:?}",
        shortened.took
    );
    assert_eq!(Some("wait=2"), shortened.field(PREFERENCE_APPLIED));
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "time-out")],
        shortened.federated()?.statuses()
    );

    let longer = exchange(
        &app,
        post_aql(&patient_compositions(), &[(PREFER, "wait=60")])?,
    )
    .await?;
    assert_eq!(
        StatusCode::GATEWAY_TIMEOUT,
        longer.status,
        "{}",
        longer.text
    );
    assert!(
        longer.took < OVERALL + SLACK,
        "CP-31: a longer wait never extends the budget, took {:?}",
        longer.took
    );
    assert_eq!(None, longer.field(PREFERENCE_APPLIED));
    Ok(())
}

// conformance: CP-30 track-4
#[tokio::test]
async fn partial_returns_the_answering_nodes_rows_only_where_offered() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let (nodes, app) = harness(dir.path(), "best_effort = true").await?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));
    let partial = [(COMPLETENESS, COMPLETENESS_PARTIAL)];

    let reply = exchange(&app, post_aql(&patient_compositions(), &partial)?).await?;
    assert_eq!(StatusCode::OK, reply.status, "CP-30: {}", reply.text);
    let answer = reply.federated()?;
    assert!(!answer.meta.federation.complete, "CP-30: complete is false");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "node-error")],
        answer.statuses()
    );
    assert_eq!(
        1,
        answer.rows.len(),
        "CP-30: the reachable node's rows alone"
    );

    let offered_not = tempfile::tempdir()?;
    let strict = gateway_with(
        offered_not.path(),
        &nodes,
        &Options {
            federation: "best_effort = false".to_owned(),
            ..Options::default()
        },
    )?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let refused = exchange(&strict, post_aql(&patient_compositions(), &partial)?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        refused.status,
        "CP-30: partial against a gateway not offering it is rejected: {}",
        refused.text
    );
    assert_eq!("partial-unsupported", refused.code()?);
    nobody_asked(&nodes, "CP-30: never served all-or-nothing in its place");
    Ok(())
}

// conformance: CP-12 CP-30 track-4
#[tokio::test]
async fn found_nowhere_and_consent_denied_are_200_and_never_424() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let elsewhere = PatientId::new(1, 77);
    let nowhere = tempfile::tempdir()?;
    let unresolved = gateway_with(
        nowhere.path(),
        &nodes,
        &Options {
            resolver: development(&[dev_rows(
                elsewhere,
                &[("node-a", EHR_A), ("node-b", EHR_B)],
                &[],
            )]),
            ..Options::default()
        },
    )?;

    let answer = track4::found_nowhere(
        &in_process(&unresolved),
        &fixture(None, None)?,
        &synthetic(PATIENT)?,
    )
    .await?;
    assert!(!answer.meta.federation.complete, "CP-30: complete is false");
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        answer.statuses()
    );
    nobody_asked(&nodes, "nothing resolved, so nothing is dispatched");

    let denied = tempfile::tempdir()?;
    let prefiltered = gateway_with(
        denied.path(),
        &nodes,
        &Options {
            resolver: development(&[dev_rows(
                PATIENT,
                &[("node-a", EHR_A), ("node-b", EHR_B)],
                &["node-b"],
            )]),
            ..Options::default()
        },
    )?;
    let reply = exchange(&prefiltered, post_aql(&patient_compositions(), &[])?).await?;
    assert_eq!(
        StatusCode::OK,
        reply.status,
        "CP-30: a consent-denied node leaves a 200 with the rest: {}",
        reply.text
    );
    let answer = reply.federated()?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        answer.statuses()
    );
    assert!(!answer.meta.federation.complete, "CP-30: complete is false");
    assert_eq!(1, answer.rows.len(), "node A's rows");
    assert!(
        asked(&nodes.b).is_empty(),
        "the pre-filter dispatches nothing to node B"
    );
    Ok(())
}
