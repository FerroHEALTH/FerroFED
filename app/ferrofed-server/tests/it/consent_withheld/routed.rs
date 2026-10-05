// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every answer a routed read can give where the deployment withholds consent
//! exclusions (Regulation (EU) 2025/327 Art 8): a node's consent refusal, a
//! read of an `ehr_id` the pre-filter excluded in a query, and an ask-all
//! probe a member refuses on consent.
//!
//! Each is compared with the answer for a resource no node has, on the same
//! path: the status, the code, the message and every header but the request
//! id are the same, and nothing names consent, the refusal code, the node's
//! own refusal text or the refusing endpoint beyond what the request named.
//! A routed refusal and a routed absence each take one node round trip, and a
//! probe asks every member either way, so neither differs in timing class.

use std::collections::BTreeMap;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::settings::ConsentDisclosure;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{AT_A, BOTH, Crossref, Denies, NOWHERE, TestResult, gateway_with};
use crate::facade::{EHR_B, body, node_answering, patient_query, post, received};
use crate::support::{error_body, send};

/// The consent refusal code the registry lists for node B.
const REFUSAL_CODE: &str = "consent-refused";

/// The text of node B's own refusal, which the answer never quotes.
const REFUSAL_TEXT: &str = "synthetic refusal: the patient restricted access";

/// The registry line that lists [`REFUSAL_CODE`] for node B's endpoint.
fn refusal_codes() -> String {
    format!("consent_refusal_codes = [\"{REFUSAL_CODE}\"]\n")
}

/// A node answering `GET /v1/ehr/{EHR_B}` with `answer`.
async fn node_answering_b_with(answer: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_B}")))
        .respond_with(answer)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    server
}

/// Node B's consent refusal.
fn refusal() -> ResponseTemplate {
    let error =
        format!(r#"{{"message":"{REFUSAL_TEXT}","validationErrors":[],"code":"{REFUSAL_CODE}"}}"#);
    ResponseTemplate::new(403).set_body_raw(error.into_bytes(), "application/json")
}

/// Node B's own `404` for an EHR it does not hold.
fn absent() -> ResponseTemplate {
    let error = r#"{"message":"synthetic: no EHR with this ehr_id","validationErrors":[]}"#;
    ResponseTemplate::new(404).set_body_raw(error.as_bytes().to_vec(), "application/json")
}

/// What a client can see of an answer: the status, every header but the
/// request id, the code, the message, and the whole text.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    status: StatusCode,
    headers: BTreeMap<String, String>,
    code: String,
    message: String,
}

/// The answer of `app` to `request`, with its text.
async fn seen(
    app: Router,
    request: Request<Body>,
) -> Result<(Seen, String), Box<dyn std::error::Error>> {
    let response = send(app, request).await?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .filter(|(name, _)| !name.as_str().contains("request-id"))
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    let error = error_body(&text)?;
    Ok((
        Seen {
            status,
            headers,
            code: error.code,
            message: error.message,
        },
        text,
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

/// Asserts that `text` and `seen` name no consent, no refusal code, none of
/// the node's own refusal text, and no endpoint the request did not name.
fn names_nothing(seen: &Seen, text: &str, named: bool) {
    let shown = format!("{seen:?} {text}");
    assert!(!shown.to_ascii_lowercase().contains("consent"), "{shown}");
    assert!(!shown.contains(REFUSAL_CODE), "{shown}");
    assert!(!shown.contains(REFUSAL_TEXT), "{shown}");
    if !named {
        assert!(!shown.contains("node-b"), "{shown}");
    }
}

// conformance: CP-36
#[tokio::test]
async fn a_targeted_read_the_node_refuses_reads_exactly_as_one_the_node_cannot_find() -> TestResult
{
    let withheld = ConsentDisclosure::Withheld;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));

    let a = node_answering_b_with(absent()).await;
    let b = node_answering_b_with(refusal()).await;
    let (app, _state) = gateway_with((&a, &b), &refusal_codes(), scripted, withheld)?;
    let (refused, text) = seen(app, read_of_b(true)?).await?;
    names_nothing(&refused, &text, true);

    let missing = node_answering_b_with(absent()).await;
    let (app, _state) = gateway_with((&a, &missing), &refusal_codes(), scripted, withheld)?;
    let (not_found, text) = seen(app, read_of_b(true)?).await?;
    assert!(
        !text.contains("no EHR with this ehr_id"),
        "the gateway answers: {text}"
    );

    assert_eq!(StatusCode::NOT_FOUND, refused.status);
    assert_eq!("subject-unavailable", refused.code);
    assert_eq!(
        not_found, refused,
        "Art 8: status, code, message and headers are those of an absent EHR"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn an_ask_all_probe_a_member_refuses_reads_exactly_as_one_no_member_answers() -> TestResult {
    let withheld = ConsentDisclosure::Withheld;
    let scripted = (Crossref::Knows(NOWHERE), Denies(false));
    let a = node_answering_b_with(absent()).await;

    let b = node_answering_b_with(refusal()).await;
    let (app, _state) = gateway_with((&a, &b), &refusal_codes(), scripted, withheld)?;
    let (refused, text) = seen(app, read_of_b(false)?).await?;
    names_nothing(&refused, &text, false);

    let missing = node_answering_b_with(absent()).await;
    let (app, _state) = gateway_with((&a, &missing), &refusal_codes(), scripted, withheld)?;
    let (not_found, _) = seen(app, read_of_b(false)?).await?;

    assert_eq!(StatusCode::NOT_FOUND, refused.status);
    assert_eq!(
        not_found, refused,
        "Art 8: the probe reads the refusing member as one without the EHR"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_read_of_an_ehr_id_the_prefilter_excluded_is_left_to_its_node() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering_b_with(refusal()).await;
    let excluded = (Crossref::Knows(BOTH), Denies(true));
    let withheld = ConsentDisclosure::Withheld;
    let (app, _state) = gateway_with((&a, &b), &refusal_codes(), excluded, withheld)?;

    let response = send(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        received(&b).await?.is_empty(),
        "N27a: the query sends node B nothing"
    );

    let (index_routed, text) = seen(app, read_of_b(false)?).await?;
    names_nothing(&index_routed, &text, false);
    assert_eq!(
        1,
        received(&b).await?.len(),
        "§12.5.1: no binding or index entry names the excluded member, so the probe asks it and it decides (N27)"
    );

    let unknown = (Crossref::Knows(AT_A), Denies(false));
    let missing = node_answering_b_with(absent()).await;
    let (app, _state) = gateway_with((&a, &missing), &refusal_codes(), unknown, withheld)?;
    let (not_found, _) = seen(app, read_of_b(false)?).await?;
    assert_eq!(
        not_found, index_routed,
        "Art 8: the read reads as one of an EHR no member holds"
    );
    Ok(())
}
