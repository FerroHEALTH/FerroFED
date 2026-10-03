// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a routed request carries to its node and what comes back unchanged (CP-24).

use axum::body::Body;
use http::{Method, Request, StatusCode, header};
use wiremock::ResponseTemplate;

use crate::declared::composition_at;
use crate::facade::EHR_A;
use crate::support::send;

use super::{
    ENDPOINT_A, TestResult, VERSION_A, composition, digest, gateway_over, names_node_a, node,
    only_request, parts, silent, to_a,
};

// conformance: CP-24
#[tokio::test]
async fn a_committed_composition_lands_byte_identical_and_location_and_etag_pass_through()
-> TestResult {
    let composition_path = format!("/v1/ehr/{EHR_A}/composition");
    let location =
        format!("https://cdr-a.example.org/openehr/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let node_body = format!("{{\"uid\" : {{\"value\":\"{VERSION_A}\"}} }}");
    let a = node(
        "POST",
        composition_path.clone(),
        ResponseTemplate::new(201)
            .insert_header("Location", location.as_str())
            .insert_header("ETag", format!("\"{VERSION_A}\"").as_str())
            .set_body_raw(node_body.clone().into_bytes(), "application/json"),
    )
    .await;
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let sent = composition();
    let request = Request::post(&composition_path)
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Prefer", "return=representation")
        .header(
            "openehr-audit-details",
            "committer.name=\"Synthetic clinician\"",
        )
        .body(Body::from(sent.clone()))?;
    let (status, headers, body) =
        parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;

    assert_eq!(
        StatusCode::CREATED,
        status,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let received = only_request(&a).await?;
    assert_eq!(
        digest(sent.as_bytes()),
        digest(&received.body),
        "the commit body reaches the node byte-identical, its DV_IDENTIFIER included (N22, N33, track 10)"
    );
    assert_eq!(sent.as_bytes(), received.body.as_slice());
    assert_eq!(
        Some(location.as_str()),
        headers.get(header::LOCATION).and_then(|v| v.to_str().ok()),
        "Location passes through unmodified (N31)"
    );
    assert_eq!(
        Some(format!("\"{VERSION_A}\"").as_str()),
        headers.get(header::ETAG).and_then(|v| v.to_str().ok()),
        "ETag passes through unmodified (N31)"
    );
    assert_eq!(
        node_body.as_bytes(),
        body.as_slice(),
        "the node's body as sent"
    );
    names_node_a(&headers, "POST");
    for (name, value) in [
        ("content-type", "application/json"),
        ("prefer", "return=representation"),
        (
            "openehr-audit-details",
            "committer.name=\"Synthetic clinician\"",
        ),
    ] {
        assert_eq!(
            Some(value),
            received.headers.get(name).and_then(|v| v.to_str().ok()),
            "the ITS-REST header {name} is forwarded as sent"
        );
    }
    assert!(
        b.received_requests().await.ok_or("recording")?.is_empty(),
        "node B is never asked"
    );
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn every_routed_answer_names_the_acting_endpoint_and_its_system_id() -> TestResult {
    // NOTE: ITS-REST 1.1.0 EHR API: only `PUT` of a composition declares
    // `If-Match`, so it travels there and is stripped from `GET` and `DELETE`.
    for (verb, answer, declares_if_match) in [
        (
            Method::GET,
            ResponseTemplate::new(200).set_body_raw(b"{}".to_vec(), "application/json"),
            false,
        ),
        (
            Method::PUT,
            ResponseTemplate::new(200).insert_header("ETag", "\"v::cdr-a.example.org::2\""),
            true,
        ),
        (Method::DELETE, ResponseTemplate::new(204), false),
    ] {
        let resource = composition_at(&verb, VERSION_A);
        let a = node(verb.as_str(), resource.clone(), answer).await;
        let b = silent().await;
        let dir = tempfile::tempdir()?;
        let request = Request::builder()
            .method(verb.clone())
            .uri(&resource)
            .header("openEHR-federation-endpoint", ENDPOINT_A)
            .header(header::IF_MATCH, format!("\"{VERSION_A}\""))
            .body(Body::empty())?;
        let (status, headers, _) =
            parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
        assert!(status.is_success(), "{verb}: {status}");
        names_node_a(&headers, verb.as_str());
        let received = only_request(&a).await?;
        let expected = format!("\"{VERSION_A}\"");
        assert_eq!(
            declares_if_match.then_some(expected.as_bytes()),
            received
                .headers
                .get(header::IF_MATCH)
                .map(http::HeaderValue::as_bytes),
            "{verb}: If-Match reaches the node as sent exactly where the operation declares it"
        );
    }
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn the_nodes_own_404_and_500_pass_through_as_the_node_sent_them() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for (status, said) in [
        (
            StatusCode::NOT_FOUND,
            &br#"{"message":"synthetic: no such version"}"#[..],
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            &br#"{"message":"synthetic node failure"}"#[..],
        ),
    ] {
        let a = node(
            "GET",
            resource.clone(),
            ResponseTemplate::new(status.as_u16()).set_body_raw(said.to_vec(), "application/json"),
        )
        .await;
        let b = silent().await;
        let dir = tempfile::tempdir()?;
        let request = to_a(Method::GET, &resource, Body::empty())?;
        let (answered, headers, body) =
            parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
        assert_eq!(status, answered, "the node's status passes through (§11.2)");
        assert_eq!(
            said,
            body.as_slice(),
            "the node's body passes through (§11.2)"
        );
        names_node_a(&headers, status.as_str());
    }
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_redirect_is_the_nodes_answer_and_is_never_followed() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let elsewhere = "https://elsewhere.example.org/v1/ehr";
    let a = node(
        "GET",
        resource.clone(),
        ResponseTemplate::new(303).insert_header("Location", elsewhere),
    )
    .await;
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let request = to_a(Method::GET, &resource, Body::empty())?;
    let (status, headers, _) =
        parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
    assert_eq!(StatusCode::SEE_OTHER, status);
    assert_eq!(
        Some(elsewhere),
        headers.get(header::LOCATION).and_then(|v| v.to_str().ok()),
        "the node's Location is passed on unmodified (N31)"
    );
    names_node_a(&headers, "303");
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_query_parameter_its_rest_defines_is_forwarded_as_received() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/ehr_status");
    let a = node("GET", resource.clone(), ResponseTemplate::new(200)).await;
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let query = "version_at_time=2026-01-01T00%3A00%3A00Z";
    let request = to_a(Method::GET, &format!("{resource}?{query}"), Body::empty())?;
    let (status, headers, _) =
        parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
    assert_eq!(StatusCode::OK, status);
    names_node_a(&headers, "GET ehr_status");
    assert_eq!(Some(query), only_request(&a).await?.url.query());
    Ok(())
}
