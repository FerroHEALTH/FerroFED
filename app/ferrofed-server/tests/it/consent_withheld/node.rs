// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node's own consent refusal, a `403` whose ITS-REST `Error` carries a
//! `code` the registry lists in `consent_refusal_codes` (§11.1, N27), where
//! the deployment withholds consent exclusions (Regulation (EU) 2025/327
//! Art 8).
//!
//! A federated query reports the node `not-resolved`, with the record a
//! member that does not know the patient has; a read by subject, a routed
//! read and an ask-all probe answer `404 subject-unavailable`, the answer for
//! an EHR this request may not reach whatever the reason (RFC 9110 §15.5.5).
//! The node request metrics count the refusal as `consent-denied` either way.
//! With disclosure, every path answers as before.

use axum::body::Body;
use ferrofed_server::config::settings::ConsentDisclosure;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    AT_A, AT_B, BOTH, Crossref, Denies, NOWHERE, TestResult, ask, by_subject, ehr_node,
    gateway_with, names_consent, record_of,
};
use crate::facade::{Answer, EHR_A, EHR_B, node_answering, statuses, wire};
use crate::metrics::{count, parse};
use crate::support::{call, error_body, send};

/// The consent refusal code the registry lists for node B.
const REFUSAL_CODE: &str = "consent-refused";

/// The registry line that lists [`REFUSAL_CODE`] for node B's endpoint.
fn refusal_codes() -> String {
    format!("consent_refusal_codes = [\"{REFUSAL_CODE}\"]\n")
}

/// The ITS-REST `Error` a node refusing on consent grounds answers with.
fn refusal() -> ResponseTemplate {
    let error = format!(
        r#"{{"message":"synthetic consent refusal","validationErrors":[],"code":"{REFUSAL_CODE}"}}"#
    );
    ResponseTemplate::new(403).set_body_raw(error.into_bytes(), "application/json")
}

/// A node refusing every request on consent grounds.
async fn node_refusing() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(refusal())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_B}")))
        .respond_with(refusal())
        .mount(&server)
        .await;
    server
}

/// The node requests to `endpoint` the metrics of `state` counted with
/// `outcome`.
fn node_requests(
    state: &AppState,
    endpoint: &str,
    outcome: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let samples = parse(&state.metrics().render()?)?;
    Ok(count(
        &samples,
        "ferrofed_node_requests_total",
        &[("endpoint", endpoint), ("outcome", outcome)],
    ))
}

/// `GET {base}/v1/ehr/{EHR_B}`, naming node B in the targeting header when
/// `targeted`.
fn read_of_b(targeted: bool) -> Result<Request<Body>, http::Error> {
    let mut request =
        Request::get(format!("/v1/ehr/{EHR_B}")).header(header::ACCEPT, "application/json");
    if targeted {
        request = request.header("openEHR-federation-endpoint", "node-b-pub");
    }
    request.body(Body::empty())
}

// conformance: CP-36
#[tokio::test]
async fn a_node_refusal_is_reported_as_a_member_without_the_patient() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(BOTH), Denies(false));
    let (app, state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        scripted,
        ConsentDisclosure::Withheld,
    )?;

    let (status, headers, text) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "§11.3: nothing fails: {text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-resolved")],
        statuses(&answer),
        "Art 8: the node's refusal reads as a member without the patient"
    );
    assert!(!answer.meta.federation.complete, "N16, N37");
    assert!(!names_consent(&text), "{text}");
    assert!(!text.contains(REFUSAL_CODE), "{text}");
    for line in &headers {
        assert!(!names_consent(line), "{line}");
    }
    assert_eq!(
        Some("1".to_owned()),
        node_requests(&state, "node-b-pub", "consent-denied")?,
        "the operator still counts the refusal"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_node_refusal_reads_exactly_as_a_member_that_does_not_know_the_patient() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_refusing().await;
    let refused = (Crossref::Knows(BOTH), Denies(false));
    let (app, _state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        refused,
        ConsentDisclosure::Withheld,
    )?;
    let (_, _, hidden) = ask(app).await?;

    let unknown = (Crossref::Knows(AT_A), Denies(false));
    let (app, _state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        unknown,
        ConsentDisclosure::Withheld,
    )?;
    let (_, _, absent) = ask(app).await?;

    assert_eq!(
        record_of(&absent, "node-b-pub")?,
        record_of(&hidden, "node-b-pub")?,
        "Art 8: no latency_ms and no error text tells the refusal apart"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn with_disclosure_a_node_refusal_stays_consent_denied() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(BOTH), Denies(false));
    let (app, state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        scripted,
        ConsentDisclosure::Disclosed,
    )?;

    let (status, _, text) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer),
        "N27: the specification's report"
    );
    assert!(
        record_of(&text, "node-b-pub")?.contains_key("latency_ms"),
        "N40"
    );
    assert_eq!(
        Some("1".to_owned()),
        node_requests(&state, "node-b-pub", "consent-denied")?
    );
    Ok(())
}

/// A node holding no EHR, which answers `GET /v1/ehr/{ehr_id}` with `404`.
async fn node_without_ehrs() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    server
}

// conformance: CP-36
#[tokio::test]
async fn a_read_by_subject_the_holder_refuses_answers_as_one_no_member_holds() -> TestResult {
    let a = ehr_node("cdr-a.example.org", EHR_A).await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(AT_B), Denies(false));
    let withheld = ConsentDisclosure::Withheld;
    let (app, state) = gateway_with((&a, &b), &refusal_codes(), scripted, withheld)?;

    let response = send(app, by_subject()?).await?;
    let status = response.status();
    let headers: Vec<String> = response
        .headers()
        .iter()
        .map(|(name, value)| format!("{name}: {}", String::from_utf8_lossy(value.as_bytes())))
        .collect();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert_eq!(StatusCode::NOT_FOUND, status, "RFC 9110 §15.5.5: {text}");
    let refused = error_body(&text)?;
    assert_eq!("subject-unavailable", refused.code);
    assert!(
        !names_consent(&text) && !text.contains(REFUSAL_CODE),
        "{text}"
    );
    for line in &headers {
        assert!(
            !names_consent(line) && !line.contains("node-b"),
            "the answer names no acting endpoint: {line}"
        );
    }
    assert_eq!(
        Some("1".to_owned()),
        node_requests(&state, "node-b-pub", "consent-denied")?
    );

    let nowhere = (Crossref::Knows(NOWHERE), Denies(false));
    let (app, _state) = gateway_with((&a, &b), &refusal_codes(), nowhere, withheld)?;
    let (status, text) = call(app, by_subject()?).await?;
    let absent = error_body(&text)?;
    assert_eq!(
        (StatusCode::NOT_FOUND, refused.code, refused.message),
        (status, absent.code, absent.message),
        "Art 8: the refused read reads as one no member holds"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_routed_read_the_node_refuses_answers_subject_unavailable() -> TestResult {
    let a = node_without_ehrs().await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));
    let (app, state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        scripted,
        ConsentDisclosure::Withheld,
    )?;

    let (status, text) = call(app, read_of_b(true)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "RFC 9110 §15.5.5: {text}");
    assert_eq!("subject-unavailable", error_body(&text)?.code);
    assert!(
        !names_consent(&text) && !text.contains(REFUSAL_CODE),
        "{text}"
    );
    assert_eq!(
        Some("1".to_owned()),
        node_requests(&state, "node-b-pub", "consent-denied")?
    );
    Ok(())
}

#[tokio::test]
async fn with_disclosure_a_routed_read_passes_the_refusal_through() -> TestResult {
    let a = node_without_ehrs().await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));
    let (app, _state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        scripted,
        ConsentDisclosure::Disclosed,
    )?;

    let (status, text) = call(app, read_of_b(true)?).await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "§11.2: the node's own answer passes through: {text}"
    );
    assert!(text.contains(REFUSAL_CODE), "{text}");
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn an_ask_all_probe_the_holder_refuses_answers_as_one_no_member_holds() -> TestResult {
    let a = node_without_ehrs().await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));
    let withheld = ConsentDisclosure::Withheld;
    let (app, state) = gateway_with((&a, &b), &refusal_codes(), scripted, withheld)?;
    let (status, text) = call(app, read_of_b(false)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let refused = error_body(&text)?;
    assert_eq!("subject-unavailable", refused.code);
    assert!(!names_consent(&text) && !text.contains("node-b"), "{text}");
    assert_eq!(
        Some("1".to_owned()),
        node_requests(&state, "node-b-pub", "consent-denied")?
    );

    let empty = node_without_ehrs().await;
    let (app, _state) = gateway_with((&a, &empty), &refusal_codes(), scripted, withheld)?;
    let (status, text) = call(app, read_of_b(false)?).await?;
    let absent = error_body(&text)?;
    assert_eq!(
        (StatusCode::NOT_FOUND, refused.code, refused.message),
        (status, absent.code, absent.message),
        "Art 8: a probe the holder refuses reads as one no member answers"
    );
    assert!(wire(&a).await?.contains(EHR_B), "node A was probed");
    Ok(())
}

#[tokio::test]
async fn with_disclosure_an_ask_all_probe_the_holder_refuses_is_a_node_error() -> TestResult {
    let a = node_without_ehrs().await;
    let b = node_refusing().await;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));
    let (app, _state) = gateway_with(
        (&a, &b),
        &refusal_codes(),
        scripted,
        ConsentDisclosure::Disclosed,
    )?;
    let (status, text) = call(app, read_of_b(false)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert_eq!("node-error", error_body(&text)?.code);
    Ok(())
}
