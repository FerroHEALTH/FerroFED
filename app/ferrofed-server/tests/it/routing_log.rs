// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Identifier hygiene on the log of a routed request (§5.4.1, §5.4.3, N33):
//! the client's `x-request-id` is free text that can carry a patient
//! identifier, so no event of the routed path names it. Every refusal, node
//! failure and ask-all outcome is logged under the gateway's own id, the one
//! the request line records.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::unreachable;
use http::Request;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{EHR_A, PATIENT, gateway, registry};
use crate::request_log::logged;
use crate::support::{self, request_lines};

type TestResult = Result<(), Box<dyn Error>>;

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// A request of `uri` that names itself with the patient identifier, and
/// names `endpoint` as its target when one is given.
fn named_by_the_patient(uri: &str, endpoint: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::get(uri).header("x-request-id", format!("req-{PATIENT}"));
    if let Some(endpoint) = endpoint {
        request = request.header("openEHR-federation-endpoint", endpoint);
    }
    request.body(Body::empty())
}

/// Asserts that `text` holds `expected` event messages, each under a
/// gateway id some request line records, and the patient identifier nowhere.
fn logged_under_the_gateways_id(text: &str, expected: &[&str]) -> TestResult {
    assert!(
        !text.contains(PATIENT),
        "the client's id reached the log: {text}"
    );
    let ids: Vec<String> = request_lines(text)?
        .into_iter()
        .filter_map(|line| line.request_id)
        .collect();
    let lines = support::lines(text)?;
    for message in expected {
        let event = lines
            .iter()
            .find(|line| line.message == *message)
            .ok_or_else(|| format!("{message:?} is logged: {text}"))?;
        let id = event.request_id.as_deref().ok_or("under an id")?;
        assert!(
            ids.iter().any(|recorded| recorded == id),
            "{message:?} names the id a request line records: {text}"
        );
    }
    Ok(())
}

// conformance: CP-26
#[test]
fn no_event_of_a_routed_request_names_the_clients_id() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (holder, other) = runtime.block_on(async {
        let holder = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/ehr/{EHR_A}")))
            .respond_with(ResponseTemplate::new(200))
            .mount(&holder)
            .await;
        let other = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/ehr/{EHR_A}")))
            .respond_with(ResponseTemplate::new(200))
            .mount(&other)
            .await;
        (holder, other)
    });
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let dir = tempfile::tempdir()?;

    // A query parameter refused before routing, a security event.
    let to_a = gateway(
        dir.path(),
        &registry(&holder.uri(), &other.uri(), ""),
        "",
        "",
    )?;
    let text = logged(
        &to_a,
        "info",
        vec![named_by_the_patient(
            &format!("{resource}?patient={PATIENT}"),
            Some("node-a-pub"),
        )?],
    )?;
    logged_under_the_gateways_id(
        &text,
        &["a routed request carried a query parameter that is never forwarded, and was refused"],
    )?;

    // A targeted node that cannot be reached.
    let dead = gateway(
        dir.path(),
        &registry(unreachable::BASE, &other.uri(), ""),
        "",
        "",
    )?;
    let text = logged(
        &dead,
        "info",
        vec![named_by_the_patient(&resource, Some("node-a-pub"))?],
    )?;
    logged_under_the_gateways_id(&text, &["the routed request failed"])?;

    // An ask-all probe with a member that cannot be reached.
    let text = logged(&dead, "info", vec![named_by_the_patient(&resource, None)?])?;
    logged_under_the_gateways_id(&text, &["the ask-all probe named no owner"])?;

    // An ask-all probe that finds two claimants, and a malformed ehr_id.
    let text = logged(
        &to_a,
        "trace",
        vec![
            named_by_the_patient(&resource, None)?,
            named_by_the_patient(
                &format!("/v1/ehr/not%20an%20id/composition/{VERSION_A}"),
                None,
            )?,
        ],
    )?;
    logged_under_the_gateways_id(&text, &[])?;
    Ok(())
}
