// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Content-Type` of a query `POST`: `POST {base}/v1/query/aql` and the
//! stored-query `POST {base}/v1/query/{name}` take their body as
//! `application/json`, the one media type ITS-REST 1.1.0 lists for both. A
//! `Content-Type` naming another is a `415` that asks no node (RFC 9110
//! §15.5.16), and a body sent without one is read as the listed media type.
//!
//! The security event of a refused query parameter holds for every caller,
//! a request the gateway answers itself included (§5.4.3).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::unreachable;
use http::{Request, StatusCode, header};
use wiremock::MockServer;

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, dev_gateway, node_answering, patient_query, received,
    registry,
};
use crate::request_log::logged;
use crate::support::{self, call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// The qualified name the stored query is held under.
const NAME: &str = "org.example::media";

/// The `Content-Type` values no query `POST` takes: another media type, and
/// the listed one with a parameter other than a `utf-8` charset.
const UNLISTED: [&str; 4] = [
    "text/plain",
    "application/xml",
    "application/json; charset=latin1",
    "application/json; profile=x",
];

/// A request of `uri` carrying `body`, with `content_type` when one is given.
fn posted(
    uri: &str,
    body: String,
    content_type: Option<&str>,
) -> Result<Request<Body>, http::Error> {
    let mut request = Request::post(uri);
    if let Some(value) = content_type {
        request = request.header(header::CONTENT_TYPE, value);
    }
    request.body(Body::from(body))
}

/// Node A and node B, each answering the federated query with one row.
async fn nodes() -> (MockServer, MockServer) {
    (
        node_answering("uid-at-a::cdr-a.example.org::1").await,
        node_answering("uid-at-b::cdr-b.example.org::1").await,
    )
}

/// Asserts that neither `a` nor `b` received anything.
async fn asked_neither(a: &MockServer, b: &MockServer) -> TestResult {
    assert!(received(a).await?.is_empty(), "node A was asked");
    assert!(received(b).await?.is_empty(), "node B was asked");
    Ok(())
}

#[tokio::test]
async fn an_adhoc_query_in_an_unlisted_media_type_is_415_and_asks_no_node() -> TestResult {
    for content_type in UNLISTED {
        let (a, b) = nodes().await;
        let dir = tempfile::tempdir()?;
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let request = posted("/v1/query/aql", body(&patient_query())?, Some(content_type))?;
        let (status, text) = call(app, request).await?;
        assert_eq!(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            status,
            "{content_type}: {text}"
        );
        let error = error_body(&text)?;
        assert_eq!("media-type-unsupported", error.code, "{content_type}");
        assert!(error.message.contains("application/json"), "{text}");
        assert!(!text.contains(PATIENT), "the answer quotes nothing: {text}");
        asked_neither(&a, &b).await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_adhoc_query_in_the_listed_media_type_or_none_is_answered() -> TestResult {
    for content_type in [
        Some("application/json"),
        Some("Application/JSON; charset=UTF-8"),
        None,
    ] {
        let (a, b) = nodes().await;
        let dir = tempfile::tempdir()?;
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let request = posted("/v1/query/aql", body(&patient_query())?, content_type)?;
        let (status, text) = call(app, request).await?;
        assert_eq!(StatusCode::OK, status, "{content_type:?}: {text}");
        assert_eq!(1, received(&a).await?.len(), "{content_type:?}");
        assert_eq!(1, received(&b).await?.len(), "{content_type:?}");
    }
    Ok(())
}

/// A registry gateway over `a` and `b` holding [`NAME`], a query naming the
/// patient through `$patient`.
async fn holding(
    dir: &std::path::Path,
    a: &MockServer,
    b: &MockServer,
) -> Result<Router, Box<dyn Error>> {
    let app = crate::stored::gateway(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let put = Request::put(format!("/v1/definition/query/{NAME}/1.0.0"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql))?;
    let (status, text) = call(app.clone(), put).await?;
    assert_eq!(StatusCode::OK, status, "stored: {text}");
    Ok(app)
}

/// The `Query` body binding the patient.
fn bound() -> String {
    format!(r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#)
}

#[tokio::test]
async fn a_stored_query_post_in_an_unlisted_media_type_is_415_and_asks_no_node() -> TestResult {
    for content_type in UNLISTED {
        for path in [
            format!("/v1/query/{NAME}"),
            format!("/v1/query/{NAME}/1.0.0"),
        ] {
            let (a, b) = nodes().await;
            let dir = tempfile::tempdir()?;
            let app = holding(dir.path(), &a, &b).await?;
            let (status, text) = call(app, posted(&path, bound(), Some(content_type))?).await?;
            assert_eq!(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                status,
                "{path} {content_type}: {text}"
            );
            assert_eq!("media-type-unsupported", error_body(&text)?.code);
            assert!(!text.contains(PATIENT), "the answer quotes nothing: {text}");
            asked_neither(&a, &b).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_stored_query_post_in_the_listed_media_type_or_none_is_answered() -> TestResult {
    for content_type in [Some("application/json; charset=utf-8"), None] {
        let (a, b) = nodes().await;
        let dir = tempfile::tempdir()?;
        let app = holding(dir.path(), &a, &b).await?;
        let request = posted(&format!("/v1/query/{NAME}"), bound(), content_type)?;
        let (status, text) = call(app, request).await?;
        assert_eq!(StatusCode::OK, status, "{content_type:?}: {text}");
        assert_eq!(1, received(&a).await?.len(), "{content_type:?}");
        assert_eq!(1, received(&b).await?.len(), "{content_type:?}");
    }
    Ok(())
}

#[test]
fn a_refused_query_parameter_of_a_definition_put_is_the_same_security_event() -> TestResult {
    let dir = tempfile::tempdir()?;
    let app = crate::stored::gateway(
        dir.path(),
        &registry(
            unreachable::BASE,
            &format!("{}/node-b", unreachable::BASE),
            "",
        ),
        &[],
    )?;
    let put = Request::put(format!(
        "/v1/definition/query/{NAME}/1.0.0?query_type=AQL&patient={PATIENT}"
    ))
    .header(header::CONTENT_TYPE, "text/plain")
    .body(Body::from(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    ))?;
    let text = logged(&app, "info", vec![put])?;
    let lines = support::lines(&text)?;
    let event = lines
        .iter()
        .find(|line| {
            line.message
                == "a request carried a query parameter the gateway does not admit for its operation, and was refused"
        })
        .ok_or_else(|| format!("the security event is logged: {text}"))?;
    let request = support::request_lines(&text)?
        .into_iter()
        .next()
        .ok_or("one request line")?;
    assert_eq!(
        Some(StatusCode::BAD_REQUEST.as_u16()),
        request.status,
        "{text}"
    );
    assert_eq!(
        request.request_id, event.request_id,
        "the event names the request line's id: {text}"
    );
    assert!(!text.contains(PATIENT), "the value reached the log: {text}");
    Ok(())
}
