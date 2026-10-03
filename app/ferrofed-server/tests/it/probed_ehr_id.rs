// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The form of a path `ehr_id` the ask-all probe asks every member about
//! (§5.4.1, N33, §12.5.1).
//!
//! The probe carries the `ehr_id` to members the client never named, so only
//! a bare UUID is probed: any other `HIER_OBJECT_ID` form admits a value the
//! gateway cannot tell from a patient identifier. Such an `ehr_id` is routed
//! by the targeting headers, a binding or the index, and a read nothing
//! routes is a `400` that asks nobody. Every assertion on what a node
//! received reads the node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::{MockServer, ResponseTemplate};

use crate::facade::EHR_A;
use crate::path_ehr_id::{ENDPOINT_A, TestResult, answer, holder, over, probe_at, stranger};
use crate::support::{asked, error_body, mount};

/// An `ehr_id` in ISO OID form, as a member minting OID `ehr_id`s would
/// issue it, inside the `2.999` example arc.
const OID_EHR: &str = "2.999.7.1.42";

// conformance: CP-26 track-10
#[tokio::test]
async fn a_read_whose_ehr_id_is_no_uuid_and_that_nothing_routes_is_refused_and_probes_nobody()
-> TestResult {
    for segment in [
        "12345",
        OID_EHR,
        "2.999.1.1::12345",
        "synthetic-12345.example.org",
        "2222aaaa-2222-4222-8222-222222222222::12345",
    ] {
        let a = holder().await;
        let b = holder().await;
        let dir = tempfile::tempdir()?;
        let request = Request::get(format!("/v1/ehr/{segment}/ehr_status")).body(Body::empty())?;
        let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, "probe-requires-uuid".to_owned()),
            (status, error_body(&text)?.code),
            "{segment}: a value the gateway cannot tell from a patient identifier is never probed (§5.4.1, N33)"
        );
        assert!(acting.is_none(), "{segment}");
        assert!(
            !text.contains(segment),
            "{segment}: the message quotes no path: {text}"
        );
        assert!(
            asked(&a).await?.is_empty() && asked(&b).await?.is_empty(),
            "{segment}: no member is asked anything"
        );
    }
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_read_whose_ehr_id_is_a_uuid_is_still_resolved_by_the_probe() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref(), "§12.5.1 step 4");
    for server in [&a, &b] {
        assert_eq!(
            vec![probe_at()],
            asked(server).await?,
            "every member is probed for the UUID alone"
        );
    }
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn an_ehr_id_that_is_no_uuid_is_forwarded_to_the_named_node_alone_and_then_indexed()
-> TestResult {
    let a = MockServer::start().await;
    mount(
        &a,
        "GET",
        format!("/v1/ehr/{OID_EHR}"),
        ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"ehr_id":{{"value":"{OID_EHR}"}}}}"#).into_bytes(),
            "application/json",
        ),
    )
    .await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let named = Request::get(format!("/v1/ehr/{OID_EHR}"))
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .body(Body::empty())?;
    let (status, acting, text) = answer(app.clone(), named).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the client named the node (§12.5.1 step 1)"
    );
    let (status, acting, text) = answer(
        app,
        Request::get(format!("/v1/ehr/{OID_EHR}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the node's answer taught the index (§12.5.1 step 3)"
    );
    let read = ("GET".to_owned(), format!("/v1/ehr/{OID_EHR}"));
    assert_eq!(vec![read.clone(), read], asked(&a).await?);
    assert!(
        asked(&b).await?.is_empty(),
        "the member the client did not name is never asked (§5.4.1, N33)"
    );
    Ok(())
}
