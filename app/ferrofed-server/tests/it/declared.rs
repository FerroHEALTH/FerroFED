// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Declared values on the single-node route, against two mock nodes: a path
//! identifier or a query value travels only when it matches what the ITS-REST
//! operation declares for it, and a mismatch is a `400` that asks no node;
//! `Accept`, `Content-Type` and `Prefer` reach the node as values the
//! operation lists, and an `Accept` or `Content-Type` that names none is the
//! `406` or `415` a node would answer (§5.4.1, N33; RFC 9110 §12.5.1, §8.3).
//! Every assertion on what a node received reads the node's own capture (§16,
//! track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use openehr_federation::headers::ENDPOINT;

use crate::facade::{EHR_A, PATIENT};
use crate::path_ehr_id::{ENDPOINT_A, answer, holder, over, probe_at, stranger};
use crate::support::{asked, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The composition resource of [`EHR_A`] that `verb` addresses for the
/// version `version`: `PUT` names the versioned object, the version's object
/// id, and every other verb the version itself.
pub(crate) fn composition_at(verb: &http::Method, version: &str) -> String {
    // NOTE: ITS-REST EHR API, composition_update takes "only … a HIER_OBJECT_ID … (i.e. a
    // versioned_object_uid)" with format uuid, and names the preceding version in If-Match.
    let uid = if *verb == http::Method::PUT {
        version.split("::").next().unwrap_or(version)
    } else {
        version
    };
    format!("/v1/ehr/{EHR_A}/composition/{uid}")
}

/// The query string of every request `server` received, in order.
async fn queries(server: &Server) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| request.url.query().map(str::to_owned))
        .collect())
}

/// Asserts that `text` is a `400` body with the code
/// `parameter-value-invalid` that quotes no identifier.
fn refused(status: StatusCode, text: &str) -> TestResult {
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("parameter-value-invalid", error_body(text)?.code, "{text}");
    assert!(!text.contains(PATIENT), "the 400 quotes nothing: {text}");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_malformed_version_at_time_is_refused_and_no_node_is_asked() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for targeted in [true, false] {
        let a = holder().await;
        let b = stranger().await;
        let dir = tempfile::tempdir()?;
        let mut request = Request::get(format!("{resource}?version_at_time={PATIENT}"));
        if targeted {
            request = request.header(ENDPOINT, ENDPOINT_A);
        }
        let (status, _, text) =
            answer(over(dir.path(), &a, &b)?, request.body(Body::empty())?).await?;
        refused(status, &text)?;
        assert!(text.contains("query parameter 1"), "{text}");
        for server in [&a, &b] {
            assert!(
                asked(server).await?.is_empty(),
                "targeted {targeted}: no node is asked, not even by the probe"
            );
        }
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_well_formed_version_at_time_reaches_the_node_byte_identical() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let query = "version_at_time=2015-01-20T19:30:22.765%2B01:00";
    let request = Request::get(format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}?{query}"))
        .header(ENDPOINT, ENDPOINT_A)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(vec![Some(query.to_owned())], queries(&a).await?);
    assert!(asked(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn the_listed_values_of_a_commit_reach_the_node_and_no_client_text() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let request = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(ENDPOINT, ENDPOINT_A)
        .header("content-type", "application/json; charset=UTF-8")
        .header(
            "prefer",
            format!("return=representation; patient={PATIENT}, patient={PATIENT}"),
        )
        .header(
            "accept",
            format!("*/*; q=0.5, text/html; patient={PATIENT}"),
        )
        .body(Body::from("{}"))?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let received = a.received_requests().await.ok_or("recording is on")?;
    let [commit] = received.as_slice() else {
        return Err(format!("one request at node A, not {}", received.len()).into());
    };
    for (name, listed) in [
        ("content-type", "application/json"),
        ("prefer", "return=representation"),
        ("accept", "application/json"),
    ] {
        let values: Vec<&str> = commit
            .headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        assert_eq!(
            vec![listed],
            values,
            "{name} reaches the node as the listed value"
        );
    }
    let sent = format!("{:?}", commit.headers);
    assert!(!sent.contains(PATIENT), "§5.4.1, N33: {sent}");
    Ok(())
}

// conformance: CP-24
#[tokio::test]
async fn a_body_sent_without_a_content_type_travels_with_the_one_its_operation_declares()
-> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let sent = r#"{"_type":"COMPOSITION", "name":{"value":"synthétic"}}"#;
    let request = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(ENDPOINT, ENDPOINT_A)
        .body(Body::from(sent))?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let received = a.received_requests().await.ok_or("recording is on")?;
    let [commit] = received.as_slice() else {
        return Err(format!("one request at node A, not {}", received.len()).into());
    };
    assert_eq!(sent.as_bytes(), commit.body.as_slice(), "byte-identical");
    assert_eq!(
        Some("application/json"),
        commit
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        "ITS-REST EHR API: the media type composition_create's body is declared in"
    );
    assert!(asked(&b).await?.is_empty());
    Ok(())
}

/// The status and code a routed directory read with the client header
/// `name: value` is answered with, and whether a node was asked.
async fn directory_with(
    name: &'static str,
    value: &str,
) -> Result<(StatusCode, String, bool), Box<dyn Error>> {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let request = Request::get(format!("/v1/ehr/{EHR_A}/directory"))
        .header(ENDPOINT, ENDPOINT_A)
        .header(name, value)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    let asked_any = !asked(&a).await?.is_empty() || !asked(&b).await?.is_empty();
    let code = error_body(&text)?.code;
    assert!(!text.contains(PATIENT), "the answer quotes nothing: {text}");
    Ok((status, code, asked_any))
}

// conformance: CP-26
#[tokio::test]
async fn an_accept_that_admits_nothing_listed_is_a_406_that_asks_no_node() -> TestResult {
    for value in [
        "text/html".to_owned(),
        format!("application/json; patient={PATIENT}"),
    ] {
        assert_eq!(
            (
                StatusCode::NOT_ACCEPTABLE,
                "media-type-not-acceptable".to_owned(),
                false
            ),
            directory_with("accept", &value).await?,
            "RFC 9110 §12.4.1: {value}"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn an_unlisted_content_type_is_a_415_that_asks_no_node() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    for value in [
        "text/plain".to_owned(),
        format!("application/json; patient={PATIENT}"),
    ] {
        let request = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
            .header(ENDPOINT, ENDPOINT_A)
            .header("content-type", value.as_str())
            .body(Body::from("{}"))?;
        let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            status,
            "RFC 9110 §15.5.16: {text}"
        );
        assert_eq!("media-type-unsupported", error_body(&text)?.code);
        assert!(!text.contains(PATIENT), "{text}");
    }
    for server in [&a, &b] {
        assert!(asked(server).await?.is_empty(), "nothing is sent");
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_malformed_path_uid_is_refused_by_position_and_no_node_is_asked() -> TestResult {
    for resource in [
        format!("/v1/ehr/{EHR_A}/ehr_status/{PATIENT}"),
        format!("/v1/ehr/{EHR_A}/versioned_composition/{PATIENT}"),
        format!("/v1/ehr/{EHR_A}/composition/{PATIENT}%20one"),
    ] {
        let a = holder().await;
        let b = stranger().await;
        let dir = tempfile::tempdir()?;
        let request = Request::get(&resource)
            .header(ENDPOINT, ENDPOINT_A)
            .body(Body::empty())?;
        let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
        refused(status, &text)?;
        assert!(text.contains("path parameter 2"), "{resource}: {text}");
        for server in [&a, &b] {
            assert!(
                asked(server).await?.is_empty(),
                "{resource}: nothing is sent"
            );
        }
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_free_text_parameter_passes_unclassified() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let query = format!("path={PATIENT}");
    let request = Request::get(format!("/v1/ehr/{EHR_A}/directory?{query}"))
        .header(ENDPOINT, ENDPOINT_A)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "node A's own answer: {text}");
    assert_eq!(
        vec![Some(query)],
        queries(&a).await?,
        "§5.4.1, N33: free text cannot be classified, so it travels as sent"
    );
    Ok(())
}

#[tokio::test]
async fn the_probe_leaves_out_a_value_its_own_operation_does_not_admit() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let flat = "application/openehr.wt.flat+json";
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let request = Request::get(&resource)
        .header("accept", flat)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![probe_at(), ("GET".to_owned(), resource)],
        asked(&a).await?
    );
    let accepts: Vec<Option<String>> = a
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| {
            request
                .headers
                .get("accept")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .collect();
    let [probe, read] = accepts.as_slice() else {
        return Err(format!("two requests at node A, not {}", accepts.len()).into());
    };
    assert_eq!(
        Some("application/json"),
        probe.as_deref(),
        "the probe's operation lists no {flat}, so it sends its first listed type"
    );
    assert_eq!(
        Some(flat),
        read.as_deref(),
        "the read carries the client's Accept"
    );
    Ok(())
}
