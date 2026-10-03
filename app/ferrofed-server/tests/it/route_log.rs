// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The route a request line names for a request under `{base}/v1/`: the
//! template of the ITS-REST operation it addresses, as `openehr-its` names it,
//! and never the concrete path, which carries an `ehr_id` or a version uid
//! (§5.4.3). A path that names no route is logged as `<unmatched>`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_server::request_log::UNMATCHED;
use http::{Method, Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::base_url::{gateway_at, under};
use crate::facade::EHR_A;
use crate::request_log::logged;
use crate::support::request_lines;

type TestResult = Result<(), Box<dyn Error>>;

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// A request of `verb` to `uri` naming node A as its target.
fn to_a(verb: Method, uri: &str) -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method(verb)
        .uri(uri)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .body(Body::empty())
}

/// Node A, answering a read of the EHR and of one composition in it, and
/// node B, answering nothing.
fn nodes() -> Result<(MockServer, MockServer), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(runtime.block_on(async {
        let a = MockServer::start().await;
        for resource in [
            format!("/v1/ehr/{EHR_A}"),
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ] {
            Mock::given(method("GET"))
                .and(path(resource))
                .respond_with(ResponseTemplate::new(200))
                .mount(&a)
                .await;
        }
        (a, MockServer::start().await)
    }))
}

/// The routes the request lines of `text` name, in order.
fn routes(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(request_lines(text)?
        .into_iter()
        .map(|line| line.route.unwrap_or_default())
        .collect())
}

#[test]
fn a_routed_read_logs_its_route_template_and_never_its_identifiers() -> TestResult {
    let (a, b) = nodes()?;
    let dir = tempfile::tempdir()?;
    let app = gateway_at(dir.path(), "/", &a.uri(), &b.uri())?;
    let text = logged(
        &app,
        "info",
        vec![
            to_a(Method::GET, &format!("/v1/ehr/{EHR_A}"))?,
            to_a(
                Method::GET,
                &format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
            )?,
        ],
    )?;
    assert_eq!(
        vec![
            "/v1/ehr/{ehr_id}",
            "/v1/ehr/{ehr_id}/composition/{uid_based_id}"
        ],
        routes(&text)?,
        "{text}"
    );
    let statuses: Vec<Option<u16>> = request_lines(&text)?
        .iter()
        .map(|line| line.status)
        .collect();
    assert_eq!(
        vec![Some(StatusCode::OK.as_u16()); 2],
        statuses,
        "both were routed: {text}"
    );
    assert!(!text.contains(EHR_A), "the ehr_id reached the log: {text}");
    assert!(
        !text.contains("8849182c"),
        "the version uid reached the log: {text}"
    );
    Ok(())
}

#[test]
fn a_path_that_names_no_route_logs_unmatched() -> TestResult {
    let (a, b) = nodes()?;
    let dir = tempfile::tempdir()?;
    let app = gateway_at(dir.path(), "/", &a.uri(), &b.uri())?;
    let text = logged(
        &app,
        "info",
        vec![
            to_a(Method::GET, "/v1/nothing/SYNTHETIC-PATH-ID")?,
            to_a(Method::PATCH, &format!("/v1/ehr/{EHR_A}"))?,
            to_a(Method::GET, "/elsewhere/SYNTHETIC-PATH-ID")?,
        ],
    )?;
    assert_eq!(vec![UNMATCHED; 3], routes(&text)?, "{text}");
    assert!(!text.contains("SYNTHETIC-PATH-ID"), "{text}");
    assert!(!text.contains(EHR_A), "{text}");
    Ok(())
}

#[test]
fn under_a_base_path_the_route_names_the_base_once() -> TestResult {
    let base = "/fed/openehr";
    let (a, b) = nodes()?;
    let dir = tempfile::tempdir()?;
    let app = gateway_at(dir.path(), base, &a.uri(), &b.uri())?;
    let text = logged(
        &app,
        "info",
        vec![
            to_a(Method::GET, &under(base, &format!("/v1/ehr/{EHR_A}")))?,
            to_a(Method::GET, &under(base, "/v1/query/aql"))?,
            to_a(Method::GET, &under(base, "/health"))?,
            to_a(Method::GET, &format!("/v1/ehr/{EHR_A}"))?,
        ],
    )?;
    assert_eq!(
        vec![
            "/fed/openehr/v1/ehr/{ehr_id}",
            "/fed/openehr/v1/query/aql",
            "/fed/openehr/health",
            UNMATCHED,
        ],
        routes(&text)?,
        "the base is named once, and a path outside it names no route: {text}"
    );
    assert!(!text.contains(EHR_A), "{text}");
    Ok(())
}
