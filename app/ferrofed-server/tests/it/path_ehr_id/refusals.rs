// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reads and writes no step routes, and a malformed path `ehr_id` (CP-33).

use std::time::Instant;

use axum::body::Body;
use http::{Method, Request, StatusCode};
use wiremock::{MockServer, ResponseTemplate};

use crate::declared::composition_at;
use crate::facade::{EHR_A, gateway_within, registry};
use crate::support::{SLACK, asked, error_body, millis, mount};

use super::{
    ENDPOINT_A, ENDPOINT_B, TestResult, VERSION_A, answer, holder, over, probe_at, refused_with,
    stranger,
};

// conformance: CP-33
#[tokio::test]
async fn a_read_no_member_holds_is_a_404_no_destination() -> TestResult {
    let a = stranger().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(format!("/v1/ehr/{EHR_A}/ehr_status")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2");
    assert_eq!("no-destination", error_body(&text)?.code);
    assert!(acting.is_none());
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "probed, never sent the read"
    );
    assert_eq!(vec![probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn two_members_holding_the_ehr_id_are_a_409_naming_both_and_neither_is_read() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, code, text, a, b) =
        refused_with(Request::get(&resource).body(Body::empty())?, holder().await).await?;
    assert_eq!(
        (StatusCode::CONFLICT, "ehr-id-collision"),
        (status, code.as_str()),
        "§12.5.2, N42"
    );
    assert!(
        text.contains(ENDPOINT_A) && text.contains(ENDPOINT_B),
        "the claimants are listed: {text}"
    );
    assert!(!text.contains(EHR_A), "the message quotes no path: {text}");
    for server in [&a, &b] {
        assert_eq!(
            vec![probe_at()],
            asked(server).await?,
            "no claimant is read"
        );
    }
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_member_that_does_not_answer_the_probe_in_time_leaves_the_owner_unknown() -> TestResult {
    let per_node = SLACK;
    let abandoned_by = per_node + SLACK;
    let overall = abandoned_by + SLACK;
    let slow = MockServer::start().await;
    mount(
        &slow,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(404).set_delay(overall + SLACK),
    )
    .await;
    let a = holder().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_within(
        dir.path(),
        &registry(&a.uri(), &slow.uri(), ""),
        "",
        "",
        (millis(per_node)?, millis(overall)?),
    )?;
    let started = Instant::now();
    let (status, acting, text) = answer(
        app,
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    let elapsed = started.elapsed();
    assert!(acting.is_none(), "no endpoint acted: {text}");
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-timeout"),
        (status, error_body(&text)?.code.as_str()),
        "a time-out means unknown, never absent (§11.5, §11.2): {text}"
    );
    assert!(
        text.contains(ENDPOINT_B),
        "the silent member is named: {text}"
    );
    assert!(
        elapsed < abandoned_by,
        "the member is abandoned at the per-node timeout of {per_node:?}, before the overall \
         budget of {overall:?} and never waiting for the member: {elapsed:?}"
    );
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "the one claimant is not read"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_member_answering_the_probe_with_an_error_leaves_the_owner_unknown() -> TestResult {
    let failing = MockServer::start().await;
    mount(
        &failing,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(503),
    )
    .await;
    let (status, code, text, a, _) = refused_with(
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
        failing,
    )
    .await?;
    assert_eq!(
        (StatusCode::FAILED_DEPENDENCY, "node-error"),
        (status, code.as_str()),
        "§11.2: {text}"
    );
    assert!(text.contains("node-b-pub (answered 503)"), "{text}");
    assert_eq!(vec![probe_at()], asked(&a).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_write_no_earlier_step_routes_is_a_400_and_probes_nobody() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    for (verb, at) in [
        (Method::POST, format!("/v1/ehr/{EHR_A}/composition")),
        (Method::PUT, composition_at(&Method::PUT, VERSION_A)),
        (
            Method::DELETE,
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ),
    ] {
        let request = Request::builder()
            .method(verb.clone())
            .uri(at)
            .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, "target-required".to_owned()),
            (status, error_body(&text)?.code),
            "{verb} (§12.5.1, N41)"
        );
        assert!(acting.is_none());
    }
    assert!(
        asked(&a).await?.is_empty(),
        "no probe is sent for a write (N41)"
    );
    assert!(
        asked(&b).await?.is_empty(),
        "no probe is sent for a write (N41)"
    );
    Ok(())
}

#[tokio::test]
async fn a_malformed_path_ehr_id_is_a_400_before_any_routing() -> TestResult {
    for segment in ["not%20an%20ehr_id", "%FF%FE"] {
        let a = holder().await;
        let b = holder().await;
        let dir = tempfile::tempdir()?;
        let request = Request::get(format!("/v1/ehr/{segment}/composition/{VERSION_A}"))
            .header("openEHR-federation-endpoint", ENDPOINT_A)
            .body(Body::empty())?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{segment}: §12.5: {text}");
        assert_eq!("ehr-id-invalid", error_body(&text)?.code, "{segment}");
        assert!(acting.is_none());
        assert!(
            asked(&a).await?.is_empty() && asked(&b).await?.is_empty(),
            "{segment}"
        );
    }
    Ok(())
}
