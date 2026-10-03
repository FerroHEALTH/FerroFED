// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ask-all probe over HTTP, what it teaches the index, and what it never carries.

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, PATIENT, body, dev_gateway, patient_query, post, wire};
use crate::support::{asked, mount};

use super::{
    CLIENT_TOKEN, ENDPOINT_A, ENDPOINT_B, TestResult, VERSION_A, answer, holder, over, probe_at,
    stranger,
};

// conformance: CP-33
#[tokio::test]
async fn a_read_no_earlier_step_routes_goes_to_the_one_member_the_probe_finds() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(&resource).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the acting endpoint (N31)"
    );
    assert_eq!(r#"{"_type":"COMPOSITION"}"#, text, "the owner's answer");
    assert_eq!(
        vec![probe_at(), ("GET".to_owned(), resource)],
        asked(&a).await?,
        "the owner is probed, then sent the read once"
    );
    assert_eq!(
        vec![probe_at()],
        asked(&b).await?,
        "the other member is only probed (§12.5.1 step 4)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_read_of_the_ehr_itself_is_answered_by_the_owners_probe_answer() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(
        over(dir.path(), &a, &b)?,
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    assert!(text.contains(EHR_A), "the owner's EHR: {text}");
    assert_eq!(vec![probe_at()], asked(&a).await?, "asked once");
    assert_eq!(vec![probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_probe_teaches_the_index_and_a_later_write_is_routed_by_it_unprobed() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (read, _, _) = answer(
        app.clone(),
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read);
    let write = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;
    let (status, acting, text) = answer(app, write).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(
        Some(ENDPOINT_A),
        acting.as_deref(),
        "the index names the owner (§12.5.1 step 3)"
    );
    assert_eq!(
        vec![
            probe_at(),
            ("POST".to_owned(), format!("/v1/ehr/{EHR_A}/composition"))
        ],
        asked(&a).await?
    );
    assert_eq!(
        vec![probe_at()],
        asked(&b).await?,
        "the write probes nobody (N41)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn the_index_never_overrides_an_explicit_target() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let (read, _, _) = answer(
        app.clone(),
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read);
    let named = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", ENDPOINT_B)
        .body(Body::empty())?;
    let (status, acting, _) = answer(app, named).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "node B's own 404 (§11.2)");
    assert_eq!(Some(ENDPOINT_B), acting.as_deref(), "step 1 wins (N41)");
    assert_eq!(
        vec![probe_at()],
        asked(&a).await?,
        "node A is asked only by the probe"
    );
    assert_eq!(vec![probe_at(), probe_at()], asked(&b).await?);
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_resolution_teaches_the_index_and_a_follow_up_goes_to_the_resolving_member() -> TestResult
{
    let a = Server::start().await;
    mount(
        &a,
        "POST",
        "/v1/query/aql".to_owned(),
        ResponseTemplate::new(200).set_body_raw(
            br##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##.to_vec(),
            "application/json",
        ),
    )
    .await;
    mount(
        &a,
        "GET",
        format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ResponseTemplate::new(200),
    )
    .await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    let (queried, _, text) = answer(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, queried, "{text}");
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let (status, acting, text) = answer(app, Request::get(&resource).body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    assert_eq!(
        vec![
            ("POST".to_owned(), "/v1/query/aql".to_owned()),
            ("GET".to_owned(), resource)
        ],
        asked(&a).await?,
        "the follow-up is routed by the index the resolution taught, unprobed"
    );
    assert!(asked(&b).await?.is_empty(), "node B is never asked");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn neither_the_probe_nor_the_routed_read_carries_an_identifier_or_the_clients_credential()
-> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let mut request = Request::get(&resource).body(Body::empty())?;
    let fields = request.headers_mut();
    fields.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", *CLIENT_TOKEN).parse()?,
    );
    fields.insert("x-patient", PATIENT.parse()?);
    fields.insert(header::COOKIE, format!("patient={PATIENT}").parse()?);
    fields.insert("x-request-id", format!("req-{PATIENT}").parse()?);
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for server in [&a, &b] {
        let captured = wire(server).await?;
        assert!(!captured.is_empty(), "the member was probed");
        assert!(
            !captured.contains(PATIENT),
            "no identifier reaches a member (N33): {captured:?}"
        );
        assert!(
            !captured.contains(CLIENT_TOKEN.as_str()),
            "the client's credential reaches no member: {captured:?}"
        );
        assert!(
            !captured.contains_ignoring_ascii_case("openehr-federation"),
            "the federation's headers stay at the gateway: {captured:?}"
        );
    }
    Ok(())
}
