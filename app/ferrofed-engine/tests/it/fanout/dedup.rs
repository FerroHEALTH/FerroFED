// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Version-identity dedup against mock nodes (§10.2, §10.3, N15, N36): the
//! imported-composition scenario answers both copies by default and the
//! originating one under the mode, every answer records the mode applied in
//! `meta.federation.dedup`, a failing `424` or `504` envelope included, and a
//! node whose version uid is not an `OBJECT_VERSION_ID` is `node-error`
//! (§11.1). Every body is validated against the result-set schema.

use std::error::Error;
use std::time::Duration;

use ferrofed_engine::dispatch::NodeQuery;
use ferrofed_engine::fanout::{FederatedAnswer, Plan};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::dedup::DedupMode;
use openehr_federation::order::ResultOrder;
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;
use wiremock::MockServer;

use super::{TestResult, budget, federation, json, node, rows_text, run, validated_body};

/// A synthetic node query in the shape the rewrite writes under the mode:
/// the uid and a label, no `ORDER BY`, no `LIMIT`.
const NODE_AQL: &str = "SELECT c/uid/value, c/name/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// The version an import into `node-a-pub` (`cdr-0`) carries: created at
/// `cdr-1.example.org`, the system of `node-b-pub` (§10.3, the scenario).
const IMPORTED: &str = "8849a2f0-1d3c-4e5f-9a7b-000000000001::cdr-1.example.org::1";

/// A composition of `node-a-pub`'s own.
const LOCAL: &str = "8849a2f0-1d3c-4e5f-9a7b-000000000002::cdr-0.example.org::1";

/// A mock node answering two-column rows `(uid, label)`.
async fn versions(rows: &[(&str, &str)]) -> MockServer {
    let rows: Vec<String> = rows
        .iter()
        .map(|(uid, label)| format!("[\"{uid}\",\"{label}\"]"))
        .collect();
    let body = format!(
        r##"{{"q":"node","columns":[{{"name":"#0"}},{{"name":"#1"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    node(json(200, &body)).await
}

/// The plan over `endpoints` under `mode`, the uid in node column 0.
fn plan(endpoints: &[&str], mode: DedupMode) -> Result<Plan, Box<dyn Error>> {
    let order = match mode {
        DedupMode::VersionIdentity => ResultOrder::unordered().with_version_key(0),
        _ => ResultOrder::unordered(),
    };
    let mut plan = Plan::new().ordered(order).deduplicating(mode);
    for id in endpoints {
        plan = plan.dispatch(EndpointId::new(*id)?, NodeQuery::new(NODE_AQL))?;
    }
    Ok(plan)
}

/// The `meta.federation.dedup` member of the validated body.
fn dedup_member(answer: FederatedAnswer) -> Result<String, Box<dyn Error>> {
    let text = validated_body(answer)?;
    let start = text.find(r#""dedup":"#).ok_or("dedup is present")?;
    let rest = text.get(start..).ok_or("in bounds")?;
    let end = rest.find('}').ok_or("a closed object")?;
    Ok(rest.get(..=end).ok_or("in bounds")?.to_owned())
}

async fn scenario() -> Result<(MockServer, MockServer), Box<dyn Error>> {
    let a = versions(&[(IMPORTED, "imported"), (LOCAL, "local")]).await;
    let b = versions(&[(IMPORTED, "original")]).await;
    Ok((a, b))
}

// conformance: CP-9
#[tokio::test]
async fn by_default_the_imported_composition_comes_back_twice() -> TestResult {
    let (a, b) = scenario().await?;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        plan(&["node-a-pub", "node-b-pub"], DedupMode::None)?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(answer.rows().len(), 3, "§10.1, N15: pass-through");
    assert_eq!(
        dedup_member(answer)?,
        r#""dedup":{"mode":"none"}"#,
        "§10.2: the mode applied is recorded, none included"
    );
    Ok(())
}

/// Covers the visibility half of CP-29 (§10.2, §10.3): the suppressed
/// copies stay visible in `meta.federation.dedup`; its write-routing half is
/// #66.
// conformance: CP-9
#[tokio::test]
async fn under_version_identity_the_originating_copy_is_kept_and_the_copy_named() -> TestResult {
    let (a, b) = scenario().await?;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        plan(&["node-a-pub", "node-b-pub"], DedupMode::VersionIdentity)?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        rows_text(&answer)?,
        format!(r#"[["{LOCAL}","local"],["{IMPORTED}","original"]]"#),
        "§10.2: node-b-pub's system created the version, so its copy is the one kept"
    );
    let counts: Vec<Option<u64>> = answer
        .federation()
        .endpoints()
        .iter()
        .map(EndpointOutcome::row_count)
        .collect();
    assert_eq!(counts, [Some(2), Some(1)], "§9.5: counted before dedup");
    assert_eq!(
        dedup_member(answer)?,
        r#""dedup":{"mode":"version-identity","suppressed_rows":1,"suppressed_endpoints":["node-a-pub"]}"#,
        "§10.3, N36: the suppressed endpoint stays visible"
    );
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn a_failing_424_envelope_records_the_mode() -> TestResult {
    let (a, _) = scenario().await?;
    let failing = node(json(500, r#"{"message":"synthetic failure"}"#)).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &failing.uri())])?;
    let answer = run(
        &snapshot,
        plan(&["node-a-pub", "node-b-pub"], DedupMode::VersionIdentity)?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY, "N37");
    assert!(answer.rows().is_empty());
    assert_eq!(
        dedup_member(answer)?,
        r#""dedup":{"mode":"version-identity"}"#,
        "§10.2: the mode, and no suppression over rows the answer does not carry"
    );
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn a_failing_504_envelope_records_the_mode() -> TestResult {
    let (a, _) = scenario().await?;
    let slow = node(json(200, r#"{"rows":[]}"#).set_delay(Duration::from_secs(3))).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &slow.uri())])?;
    let answer = run(
        &snapshot,
        plan(&["node-a-pub", "node-b-pub"], DedupMode::None)?,
        budget(200, 400)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::GATEWAY_TIMEOUT, "N37, N38");
    assert_eq!(dedup_member(answer)?, r#""dedup":{"mode":"none"}"#);
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn a_version_uid_of_the_wrong_form_fails_the_query_424() -> TestResult {
    let a = versions(&[("a::1", "malformed")]).await;
    let b = versions(&[(IMPORTED, "original")]).await;
    let snapshot = federation(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let answer = run(
        &snapshot,
        plan(&["node-a-pub", "node-b-pub"], DedupMode::VersionIdentity)?,
        budget(2_000, 5_000)?,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY);
    let statuses: Vec<EndpointStatus> = answer
        .federation()
        .endpoints()
        .iter()
        .map(EndpointOutcome::status)
        .collect();
    assert_eq!(
        statuses,
        [EndpointStatus::NodeError, EndpointStatus::Active],
        "§11.1: a response the gateway could not use, never a row without a duplicate"
    );
    validated_body(answer)?;
    Ok(())
}
