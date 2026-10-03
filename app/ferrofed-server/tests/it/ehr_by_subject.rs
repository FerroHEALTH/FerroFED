// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {base}/v1/ehr?subject_id=…&subject_namespace=…` (ITS-REST 1.1.0
//! `ehr_get_by_subject`) against two mock nodes: the subject is resolved at
//! the gateway and the one member that holds it is sent
//! `GET {base}/v1/ehr/{ehr_id}` under its own `ehr_id`, with no subject value
//! in any carrier (§5.2, §5.4.1, N3, N33). Its answer passes through with the
//! acting endpoint named (N31, §9.6). A subject several members hold is a
//! `409` unless the targeting header names one of them (§8.4, §12.5.2), one
//! no member holds is the operation's own `404` (§11.2), and a resolver that
//! cannot answer is a `424` (§11.2). Every assertion on what a node received
//! reads the node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::unreachable;
use http::{HeaderMap, Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, crossref, gateway, registry, wire,
};
use crate::request_log::logged;
use crate::support::{error_body, request_lines, send};

type TestResult = Result<(), Box<dyn Error>>;

/// The endpoints and `system_id`s of node A and node B in [`registry`].
const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";
const SYSTEM_A: &str = "cdr-a.example.org";
const SYSTEM_B: &str = "cdr-b.example.org";

/// The PIX Manager's ITI-83 operation.
const PIX_OPERATION: &str = "/fhir/Patient/$ihe-pix";

/// The `EHR` node `system` answers for `ehr_id`, spaced as a re-encoding
/// would not keep it.
fn ehr(system: &str, ehr_id: &str) -> String {
    format!(
        "{{ \"system_id\" : {{\"value\":\"{system}\"}},\n  \"ehr_id\":{{\"value\" : \"{ehr_id}\"}},\n  \"time_created\":{{\"value\":\"2026-01-01T00:00:00Z\"}} }}\n"
    )
}

/// A node answering `GET /v1/ehr/{ehr_id}` with its `EHR` and an `ETag`.
async fn node(system: &str, ehr_id: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{ehr_id}\"").as_str())
                .set_body_raw(ehr(system, ehr_id).into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// A node answering `GET /v1/ehr/{ehr_id}` with `404`, as a CDR answers an
/// `ehr_id` it does not hold.
async fn node_not_holding(ehr_id: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(ResponseTemplate::new(404).set_body_raw(
            br#"{"message":"synthetic: no such EHR"}"#.to_vec(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

/// A development gateway over node A at `a` and node B at `b`, resolving
/// the patient at the members `rows` name.
fn resolving(
    dir: &Path,
    a: &str,
    b: &str,
    rows: &[(&str, &str)],
) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &crossref(rows),
    )
}

/// A development gateway whose cross-reference knows only another patient.
fn knowing_another(dir: &Path, a: &str, b: &str) -> Result<Router, Box<dyn Error>> {
    let rows = format!(
        "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"SENTINEL-OTHER-71xw\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    );
    gateway(dir, &registry(a, b, ""), "profile = \"development\"", &rows)
}

/// A production gateway resolving through the PIX Manager at `pix`.
fn pix_gateway(dir: &Path, a: &str, b: &str, pix: &str) -> Result<Router, Box<dyn Error>> {
    let pixm = format!(
        "[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n"
    );
    gateway(dir, &registry(a, b, ""), "", &pixm)
}

/// A PIX Manager that fails every ITI-83 call with `500`.
async fn failing_manager() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PIX_OPERATION))
        .respond_with(
            ResponseTemplate::new(500).set_body_raw(b"{}".to_vec(), "application/fhir+json"),
        )
        .mount(&server)
        .await;
    server
}

/// The request target naming the patient by subject.
fn by_subject() -> String {
    format!("/v1/ehr?subject_id={PATIENT}&subject_namespace={NAMESPACE}")
}

/// `GET` of `uri`, carrying the patient in every client header a node must
/// never see, and naming `target` in `openEHR-federation-endpoint`.
fn get(uri: &str, target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::get(uri)
        .header(header::ACCEPT, "application/json")
        .header("x-patient", PATIENT)
        .header(header::AUTHORIZATION, format!("Bearer {PATIENT}"));
    if let Some(target) = target {
        request = request.header("openEHR-federation-endpoint", target);
    }
    request.body(Body::empty())
}

/// The status, the headers and the body text of `request` through `app`.
async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, HeaderMap, String), Box<dyn Error>> {
    let response = send(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, String::from_utf8(bytes.to_vec())?))
}

/// How many requests `server` received.
async fn asked(server: &MockServer) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

/// Asserts that `server` received exactly one request, `GET` of the node's
/// own EHR under `ehr_id` with no query string, and that its capture holds
/// neither the subject nor its namespace (§5.4.1, N33).
async fn asked_by_ehr_id_alone(server: &MockServer, ehr_id: &str) -> TestResult {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let [sent] = requests.as_slice() else {
        return Err(format!("the node is asked exactly once: {requests:?}").into());
    };
    assert_eq!("GET", sent.method.as_str(), "the read stays a read");
    assert_eq!(
        format!("/v1/ehr/{ehr_id}"),
        sent.url.path(),
        "the node is located by its own ehr_id alone (N33, N34)"
    );
    assert_eq!(None, sent.url.query(), "no query string reaches the node");
    assert!(sent.body.is_empty(), "the read carries no body");
    let captured = wire(server).await?;
    for needle in [
        PATIENT,
        PATIENT_TAIL,
        NAMESPACE,
        "subject_id",
        "subject_namespace",
    ] {
        assert!(
            !captured.contains_ignoring_ascii_case(needle),
            "{needle} reached the node (§5.4.1, N33): {captured}"
        );
    }
    Ok(())
}

/// Asserts that an error `text` quotes neither the subject nor its namespace
/// (§5.4.3).
fn quotes_no_subject(text: &str) {
    for needle in [PATIENT, PATIENT_TAIL, NAMESPACE] {
        assert!(!text.contains(needle), "the answer quotes {needle}: {text}");
    }
}

/// Asserts that `headers` name `endpoint` and `system` as the acting
/// endpoint and its `system_id` (N31, §9.6).
fn names(headers: &HeaderMap, endpoint: &str, system: &str) {
    let field = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    assert_eq!(Some(endpoint), field("openEHR-federation-endpoint"), "N31");
    assert_eq!(Some(system), field("openEHR-federation-system-id"), "§9.6");
}

// conformance: CP-24 CP-26
#[tokio::test]
async fn a_subject_one_member_holds_is_that_members_ehr_read_by_its_ehr_id() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;

    let (status, headers, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        ehr(SYSTEM_A, EHR_A),
        text,
        "the node's EHR passes through byte-identical (N22, N31)"
    );
    assert_eq!(
        Some(format!("\"{EHR_A}\"").as_str()),
        headers
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok()),
        "ETag passes through unmodified (N31)"
    );
    names(&headers, ENDPOINT_A, SYSTEM_A);
    asked_by_ehr_id_alone(&a, EHR_A).await?;
    assert_eq!(
        0,
        asked(&b).await?,
        "a member that does not hold the subject is not asked"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_subject_several_members_hold_is_a_409_listing_them_and_asks_nobody() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;

    let (status, _, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "§12.5.2: {text}");
    let error = error_body(&text)?;
    assert_eq!("subject-several", error.code);
    assert!(
        error.message.contains(ENDPOINT_A) && error.message.contains(ENDPOINT_B),
        "the message lists the endpoints: {}",
        error.message
    );
    quotes_no_subject(&text);
    assert_eq!(
        0,
        asked(&a).await? + asked(&b).await?,
        "the gateway never chooses between the members (§12.5.2)"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn the_targeting_header_names_which_of_several_holders_answers() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;

    let (status, headers, text) = answer(app, get(&by_subject(), Some(ENDPOINT_B))?).await?;
    assert_eq!(StatusCode::OK, status, "§8.4, §12.5.1 step 1: {text}");
    assert_eq!(ehr(SYSTEM_B, EHR_B), text);
    names(&headers, ENDPOINT_B, SYSTEM_B);
    asked_by_ehr_id_alone(&b, EHR_B).await?;
    assert_eq!(
        0,
        asked(&a).await?,
        "the member the header did not name is not asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_targeted_endpoint_that_does_not_hold_the_subject_is_a_404() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;

    let (status, _, text) = answer(app, get(&by_subject(), Some(ENDPOINT_B))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2: {text}");
    assert_eq!("no-destination", error_body(&text)?.code);
    quotes_no_subject(&text);
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[tokio::test]
async fn a_targeting_header_naming_no_registry_endpoint_is_a_400() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;

    let (status, _, text) = answer(app, get(&by_subject(), Some("node-z-pub"))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1: {text}");
    assert_eq!("endpoint-unknown", error_body(&text)?.code);
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[tokio::test]
async fn a_subject_no_member_holds_is_the_operations_own_404() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = knowing_another(dir.path(), &a.uri(), &b.uri())?;

    let (status, _, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "ITS-REST 1.1.0 answers 404 for a subject with no EHR (§11.2): {text}"
    );
    assert_eq!("no-destination", error_body(&text)?.code);
    quotes_no_subject(&text);
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[tokio::test]
async fn a_cross_reference_that_cannot_answer_is_a_424_and_asks_nobody() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let failing = failing_manager().await;
    let dir = tempfile::tempdir()?;
    for (case, pix) in [
        ("failing", failing.uri()),
        ("unreachable", unreachable::BASE.to_owned()),
    ] {
        let app = pix_gateway(dir.path(), &a.uri(), &b.uri(), &pix)?;
        let (status, _, text) = answer(app, get(&by_subject(), None)?).await?;
        assert_eq!(
            StatusCode::FAILED_DEPENDENCY,
            status,
            "{case}: a resolution failure is never a 404 (§11.2): {text}"
        );
        let error = error_body(&text)?;
        assert_eq!("resolution-unavailable", error.code, "{case}");
        assert!(
            error.message.contains(ENDPOINT_A) && error.message.contains(ENDPOINT_B),
            "{case}: the message names the members: {}",
            error.message
        );
        quotes_no_subject(&text);
    }
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[tokio::test]
async fn without_a_resolver_the_read_fails_closed() -> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;

    let (status, _, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert_eq!("resolution-unavailable", error_body(&text)?.code);
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[tokio::test]
async fn the_holders_own_404_passes_through_naming_it() -> TestResult {
    let a = node_not_holding(EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;

    let (status, headers, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "§11.2, node (passed through)"
    );
    assert_eq!(r#"{"message":"synthetic: no such EHR"}"#, text);
    names(&headers, ENDPOINT_A, SYSTEM_A);
    asked_by_ehr_id_alone(&a, EHR_A).await?;
    Ok(())
}

#[tokio::test]
async fn an_unreachable_holder_is_a_504_naming_it() -> TestResult {
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(
        dir.path(),
        unreachable::BASE,
        &b.uri(),
        &[("node-a", EHR_A)],
    )?;

    let (status, headers, text) = answer(app, get(&by_subject(), None)?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "§11.2: {text}");
    assert_eq!("node-unreachable", error_body(&text)?.code);
    names(&headers, ENDPOINT_A, SYSTEM_A);
    quotes_no_subject(&text);
    assert_eq!(0, asked(&b).await?);
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_subject_not_given_once_or_an_undeclared_parameter_is_a_400_that_asks_nobody()
-> TestResult {
    let a = node(SYSTEM_A, EHR_A).await;
    let b = node(SYSTEM_B, EHR_B).await;
    let dir = tempfile::tempdir()?;
    let app = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    let subject = by_subject();
    for (uri, code) in [
        (format!("/v1/ehr?subject_id={PATIENT}"), "patient-invalid"),
        (
            format!("/v1/ehr?subject_namespace={NAMESPACE}"),
            "patient-invalid",
        ),
        ("/v1/ehr".to_owned(), "patient-invalid"),
        (format!("{subject}&subject_id={PATIENT}"), "patient-invalid"),
        (
            format!("/v1/ehr?subject_id=&subject_namespace={NAMESPACE}"),
            "patient-invalid",
        ),
        (
            format!("{subject}&patient={PATIENT}"),
            "query-parameter-refused",
        ),
    ] {
        let (status, _, text) = answer(app.clone(), get(&uri, None)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
        assert_eq!(code, error_body(&text)?.code, "{text}");
        quotes_no_subject(&text);
    }
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "nobody is asked");
    Ok(())
}

#[test]
fn the_subject_reaches_no_log_line() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (a, b) =
        runtime.block_on(async { (node(SYSTEM_A, EHR_A).await, node(SYSTEM_B, EHR_B).await) });
    let dir = tempfile::tempdir()?;
    let one = resolving(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    let both = resolving(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let nowhere = knowing_another(dir.path(), &a.uri(), &b.uri())?;

    let mut text = logged(&one, "trace", vec![get(&by_subject(), None)?])?;
    text.push_str(&logged(&both, "trace", vec![get(&by_subject(), None)?])?);
    text.push_str(&logged(&nowhere, "trace", vec![get(&by_subject(), None)?])?);
    text.push_str(&logged(
        &one,
        "trace",
        vec![get(&format!("{}&patient={PATIENT}", by_subject()), None)?],
    )?);
    let lines = request_lines(&text)?;
    assert_eq!(
        4,
        lines.len(),
        "every request was logged, so the check is not vacuous: {text}"
    );
    assert!(
        text.contains("subject-parameters-consumed"),
        "the consumption is a security event (§5.4.3): {text}"
    );
    for needle in [PATIENT, PATIENT_TAIL, NAMESPACE] {
        assert!(!text.contains(needle), "{needle} reached the log: {text}");
    }
    Ok(())
}
