// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No identifier or client credential reaches the node in a header or parameter (CP-26).

use axum::body::Body;
use http::{Method, StatusCode, header};
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, PATIENT, wire};
use crate::support::{error_body, send};

use super::{
    CLIENT_TOKEN, ONWARD_TOKEN, TestResult, VERSION_A, gateway_over, node, only_request, parts,
    silent, to_a,
};

// conformance: CP-26
#[tokio::test]
async fn an_identifier_in_a_client_header_never_reaches_the_node_nor_does_its_authorization()
-> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let a = node("GET", resource.clone(), ResponseTemplate::new(200)).await;
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let mut request = to_a(Method::GET, &resource, Body::empty())?;
    let fields = request.headers_mut();
    fields.insert("x-patient", PATIENT.parse()?);
    fields.insert(header::COOKIE, format!("patient={PATIENT}").parse()?);
    fields.insert(header::FORWARDED, format!("for={PATIENT}").parse()?);
    fields.insert("x-request-id", format!("req-{PATIENT}").parse()?);
    let (status, answer, _) =
        parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        Some(format!("req-{PATIENT}").as_bytes()),
        answer.get("x-request-id").map(http::HeaderValue::as_bytes),
        "the client's id names the request in the answer only"
    );
    let received = only_request(&a).await?;
    let all = wire(&a).await?;
    assert!(
        !all.contains(PATIENT),
        "the identifier reaches no part of the request: {all}"
    );
    assert!(
        !all.contains(CLIENT_TOKEN),
        "the client's credential never reaches a node: {all}"
    );
    assert!(
        !all.contains_ignoring_ascii_case("openehr-federation"),
        "the federation's own headers stay at the gateway: {all}"
    );
    let ids: Vec<&[u8]> = received
        .headers
        .get_all("x-request-id")
        .iter()
        .map(http::HeaderValue::as_bytes)
        .collect();
    let [id] = ids.as_slice() else {
        return Err(format!("one x-request-id at the node, got {}", ids.len()).into());
    };
    uuid::Uuid::try_parse_ascii(id)?;
    assert_eq!(
        Some(format!("Bearer {ONWARD_TOKEN}").as_str()),
        received
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        "the node sees the endpoint's own onward credential (§13)"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn an_identifier_in_a_query_parameter_is_refused_before_anything_is_sent() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for query in [
        format!("patient={PATIENT}"),
        format!("version_at_time=2026-01-01T00:00:00Z&subject_id={PATIENT}"),
        PATIENT.to_owned(),
        "endpoint=node-b-pub".to_owned(),
        "path=/folders/0".to_owned(),
    ] {
        let a = node("GET", resource.clone(), ResponseTemplate::new(200)).await;
        let b = silent().await;
        let dir = tempfile::tempdir()?;
        let request = to_a(Method::GET, &format!("{resource}?{query}"), Body::empty())?;
        let (status, _, body) =
            parts(send(gateway_over(dir.path(), &a.uri(), &b.uri(), "")?, request).await?).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        assert_eq!("query-parameter-refused", error_body(&text)?.code);
        assert!(!text.contains(PATIENT), "the 400 quotes nothing: {text}");
        for server in [&a, &b] {
            assert!(
                server
                    .received_requests()
                    .await
                    .ok_or("recording")?
                    .is_empty(),
                "{query}: nothing is sent"
            );
        }
    }
    Ok(())
}
