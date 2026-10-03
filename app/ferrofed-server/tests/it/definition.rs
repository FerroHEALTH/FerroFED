// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The definition area against two mock nodes (§7a.1, §12.6, §12.7, N43,
//! N31, N33; CP-34): every request under `{base}/v1/definition/` reaches the
//! one endpoint the targeting headers name, byte-identical, and is answered
//! as that node answered, its errors included; without a target, or with a
//! target that names no one endpoint, it is refused and no node is asked; no
//! two nodes' answers are ever combined into one catalogue. Where the
//! stored-query registry is offered, it keeps answering stored-query
//! definitions itself. Every assertion on what a node received reads the
//! node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Method, Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, EHR_B, PATIENT, gateway, registry, wire};
use crate::support::{acted, asked, error_body, exchange, field, mount, refused_at_neither};

type TestResult = Result<(), Box<dyn Error>>;

const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// The ADL 1.4 template collection (ITS-REST Definition API).
const ADL14: &str = "/v1/definition/template/adl1.4";

/// The ADL 2 template collection (ITS-REST Definition API).
const ADL2: &str = "/v1/definition/template/adl2";

/// A synthetic template id.
const TEMPLATE: &str = "synthetic.vital_signs.v1";

/// The qualified name of a stored query a node holds.
const QUERY: &str = "org.example::synthetic_compositions";

/// The client's own credential, which no node ever sees.
use crate::support::CLIENT_TOKEN;

/// A synthetic operational template, spaced and encoded as a re-serialisation
/// would not keep it.
fn template() -> String {
    format!(
        "<template xmlns=\"http://schemas.openehr.org/v1\">\n  <template_id><value>{TEMPLATE}</value></template_id>\n   <concept>Synthétic   vital signs</concept>\n</template>\n"
    )
}

/// The gateway over node A and node B, with no stored-query registry.
fn over(dir: &std::path::Path, a: &Server, b: &Server) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", "")
}

/// A request of `verb` to `at` with the body `sent`, naming `target` in the
/// endpoint header when given.
fn definition(
    verb: &Method,
    at: &str,
    target: Option<&str>,
    sent: &str,
) -> Result<Request<Body>, http::Error> {
    let mut request = Request::builder().method(verb.clone()).uri(at);
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    if !sent.is_empty() {
        request = request.header(header::CONTENT_TYPE, media_type(at));
    }
    request.body(Body::from(sent.to_owned()))
}

/// The `Content-Type` ITS-REST lists for a body sent to `at`: an ADL 1.4
/// template is XML, an ADL 2 template and an AQL definition are text.
fn media_type(at: &str) -> &'static str {
    if at.starts_with(ADL14) {
        "application/xml"
    } else {
        "text/plain"
    }
}

/// Every operation of the definition area ITS-REST 1.1.0 declares, as the
/// method, the path and the body a client sends.
fn every_operation() -> Vec<(Method, String, String)> {
    vec![
        (Method::POST, ADL14.to_owned(), template()),
        (Method::GET, ADL14.to_owned(), String::new()),
        (Method::GET, format!("{ADL14}/{TEMPLATE}"), String::new()),
        (
            Method::GET,
            format!("{ADL14}/{TEMPLATE}/example"),
            String::new(),
        ),
        (Method::POST, ADL2.to_owned(), template()),
        (Method::GET, ADL2.to_owned(), String::new()),
        (Method::GET, format!("{ADL2}/{TEMPLATE}"), String::new()),
        (
            Method::GET,
            format!("{ADL2}/{TEMPLATE}/example"),
            String::new(),
        ),
        (
            Method::GET,
            format!("{ADL2}/{TEMPLATE}/1.0.0"),
            String::new(),
        ),
        (
            Method::GET,
            format!("/v1/definition/query/{QUERY}"),
            String::new(),
        ),
        (
            Method::GET,
            format!("/v1/definition/query/{QUERY}/1.0.0"),
            String::new(),
        ),
    ]
}

// conformance: CP-34
#[tokio::test]
async fn a_template_upload_with_a_target_reaches_only_that_node_byte_identical() -> TestResult {
    for at in [ADL14, ADL2] {
        let created = format!("https://cdr-b.example.org/openehr{at}/{TEMPLATE}");
        let b = Server::start().await;
        mount(
            &b,
            "POST",
            at.to_owned(),
            ResponseTemplate::new(201)
                .insert_header("Location", created.as_str())
                .insert_header("ETag", format!("\"{TEMPLATE}\"").as_str()),
        )
        .await;
        let a = Server::start().await;
        let dir = tempfile::tempdir()?;
        let sent = template();
        let mut request = definition(&Method::POST, at, Some(ENDPOINT_B), &sent)?;
        let fields = request.headers_mut();
        fields.insert("x-patient", PATIENT.parse()?);
        fields.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", *CLIENT_TOKEN).parse()?,
        );
        fields.insert(header::COOKIE, format!("patient={PATIENT}").parse()?);
        let (status, headers, _) = exchange(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(StatusCode::CREATED, status, "{at}");
        acted(&headers, ENDPOINT_B, "cdr-b.example.org");
        assert_eq!(Some(created.as_str()), field(&headers, "location"), "{at}");
        let requests = b.received_requests().await.ok_or("recording is on")?;
        let [upload] = requests.as_slice() else {
            return Err(format!("{at}: one upload at node B, not {}", requests.len()).into());
        };
        assert_eq!(
            sent.as_bytes(),
            upload.body.as_slice(),
            "{at}: byte-identical"
        );
        assert_eq!(
            Some(media_type(at)),
            field(&upload.headers, "content-type"),
            "{at}: a declared media type travels with the body"
        );
        let composed = wire(&b).await?;
        assert!(!composed.contains(PATIENT), "N33: {composed}");
        assert!(!composed.contains(CLIENT_TOKEN.as_str()), "N33: {composed}");
        assert!(
            !composed.contains_ignoring_ascii_case("openehr-federation"),
            "the federation's own headers stay at the gateway: {composed}"
        );
        assert!(asked(&a).await?.is_empty(), "{at}: node A is never asked");
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn a_definition_request_without_a_target_is_refused_and_no_node_is_asked() -> TestResult {
    let mut operations = every_operation();
    operations.push((
        Method::PUT,
        format!("/v1/definition/query/{QUERY}"),
        "SELECT c FROM EHR e CONTAINS COMPOSITION c".to_owned(),
    ));
    operations.push((
        Method::PUT,
        format!("/v1/definition/query/{QUERY}/1.0.0"),
        "SELECT c FROM EHR e CONTAINS COMPOSITION c".to_owned(),
    ));
    for (verb, at, sent) in operations {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        refused_at_neither(
            over(dir.path(), &a, &b)?,
            definition(&verb, &at, None, &sent)?,
            "target-required",
            (&a, &b),
        )
        .await?;
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn a_star_an_unknown_endpoint_and_several_endpoints_are_refused() -> TestResult {
    for (verb, at, sent) in [
        (Method::POST, ADL14.to_owned(), template()),
        (Method::GET, ADL2.to_owned(), String::new()),
    ] {
        for (named, code) in [
            ("*", "endpoint-unknown"),
            ("node-x-pub", "endpoint-unknown"),
            ("node-a-pub, node-b-pub", "endpoint-several"),
            ("node-a-pub,node-b-pub", "endpoint-several"),
        ] {
            let a = Server::start().await;
            let b = Server::start().await;
            let dir = tempfile::tempdir()?;
            refused_at_neither(
                over(dir.path(), &a, &b)?,
                definition(&verb, &at, Some(named), &sent)?,
                code,
                (&a, &b),
            )
            .await?;
        }
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn a_template_list_is_the_one_named_nodes_answer_and_never_a_union() -> TestResult {
    let at_a = format!(r#"[{{"template_id":"{TEMPLATE}","concept":"at A"}}]"#);
    let at_b = r#"[{"template_id":"synthetic.other.v1","concept":"at B"}]"#;
    let a = Server::start().await;
    let b = Server::start().await;
    for (server, listed) in [(&a, at_a.as_str()), (&b, at_b)] {
        mount(
            server,
            "GET",
            ADL14.to_owned(),
            ResponseTemplate::new(200).set_body_raw(listed.as_bytes().to_vec(), "application/json"),
        )
        .await;
    }
    let dir = tempfile::tempdir()?;
    let request = Request::get(ADL14)
        .header(ENDPOINT, ENDPOINT_A)
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())?;
    let (status, headers, body) = exchange(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status);
    acted(&headers, ENDPOINT_A, "cdr-a.example.org");
    assert_eq!(
        at_a.as_bytes(),
        body.as_slice(),
        "node A's list, byte for byte (§12.6, N43)"
    );
    assert_eq!(
        vec![("GET".to_owned(), ADL14.to_owned())],
        asked(&a).await?,
        "node A is asked once"
    );
    assert!(
        asked(&b).await?.is_empty(),
        "node B is never asked, so nothing of it can be merged in"
    );
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn a_nodes_own_error_passes_through_as_the_node_sent_it() -> TestResult {
    let missing = br#"{"message":"synthetic: template not found"}"#.to_vec();
    let rejected =
        br#"{"message":"synthetic: template invalid","validationErrors":["at0000"]}"#.to_vec();
    let exists = br#"{"message":"synthetic: template already exists"}"#.to_vec();
    for (verb, at, sent, answered, body) in [
        (
            Method::GET,
            format!("{ADL14}/{TEMPLATE}"),
            String::new(),
            404,
            missing,
        ),
        (Method::POST, ADL14.to_owned(), template(), 400, rejected),
        (Method::POST, ADL2.to_owned(), template(), 409, exists),
    ] {
        let a = Server::start().await;
        let b = Server::start().await;
        mount(
            &b,
            verb.as_str(),
            at.clone(),
            ResponseTemplate::new(answered).set_body_raw(body.clone(), "application/json"),
        )
        .await;
        let dir = tempfile::tempdir()?;
        let request = definition(&verb, &at, Some(ENDPOINT_B), &sent)?;
        let (status, headers, received) = exchange(over(dir.path(), &a, &b)?, request).await?;
        assert_eq!(
            StatusCode::from_u16(answered)?,
            status,
            "{verb} {at}: the node's status (§11.2, §12.6)"
        );
        assert_eq!(body, received, "{verb} {at}: the node's body, unmasked");
        acted(&headers, ENDPOINT_B, "cdr-b.example.org");
        assert!(
            asked(&a).await?.is_empty(),
            "{verb} {at}: node A is never asked"
        );
    }
    Ok(())
}

// conformance: CP-34 CP-26
#[tokio::test]
async fn a_definition_request_carries_only_what_its_operation_declares() -> TestResult {
    let a = Server::start().await;
    mount(
        &a,
        "GET",
        ADL14.to_owned(),
        ResponseTemplate::new(200).set_body_raw(b"[]".to_vec(), "application/json"),
    )
    .await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let query = format!("template_id={TEMPLATE}&offset=0&fetch=10");
    let request = Request::get(format!("{ADL14}?{query}"))
        .header(ENDPOINT, ENDPOINT_A)
        .header(header::ACCEPT, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", *CLIENT_TOKEN))
        .header("x-patient", PATIENT)
        .header(header::FORWARDED, format!("for={PATIENT}"))
        .body(Body::empty())?;
    let (status, _, _) = exchange(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status);
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let [list] = requests.as_slice() else {
        return Err(format!("one request at node A, not {}", requests.len()).into());
    };
    assert_eq!(
        Some(query.as_str()),
        list.url.query(),
        "declared, as received"
    );
    let composed = wire(&a).await?;
    assert!(!composed.contains(PATIENT), "§5.4.1, N33: {composed}");
    assert!(
        !composed.contains(CLIENT_TOKEN.as_str()),
        "§5.4.1, N33: {composed}"
    );
    assert!(!composed.contains_ignoring_ascii_case("openehr-federation"));
    assert!(asked(&b).await?.is_empty());
    for undeclared in [format!("patient={PATIENT}"), format!("ehr_id={EHR_A}")] {
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let request = Request::get(format!("{ADL14}?{undeclared}"))
            .header(ENDPOINT, ENDPOINT_A)
            .body(Body::empty())?;
        refused_at_neither(
            over(dir.path(), &a, &b)?,
            request,
            "query-parameter-refused",
            (&a, &b),
        )
        .await?;
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn without_the_registry_a_stored_query_definition_is_the_named_nodes() -> TestResult {
    let at = format!("/v1/definition/query/{QUERY}");
    let a = Server::start().await;
    mount(&a, "PUT", at.clone(), ResponseTemplate::new(200)).await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let sent = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c\n";
    let request = Request::put(&at)
        .header(ENDPOINT, ENDPOINT_A)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(sent))?;
    let (status, headers, _) = exchange(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "§12.7 registry-not-offered");
    acted(&headers, ENDPOINT_A, "cdr-a.example.org");
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let [stored] = requests.as_slice() else {
        return Err(format!("one request at node A, not {}", requests.len()).into());
    };
    assert_eq!(sent.as_bytes(), stored.body.as_slice(), "byte-identical");
    assert_eq!(
        Some("text/plain"),
        field(&stored.headers, "content-type"),
        "the declared media type travels"
    );
    assert!(asked(&b).await?.is_empty(), "node B is never asked");
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn without_the_registry_a_versioned_stored_query_put_is_the_named_nodes() -> TestResult {
    let at = format!("/v1/definition/query/{QUERY}/1.0.0");
    let sent = "SELECT c/uid/value\n  FROM EHR e CONTAINS COMPOSITION c -- synthétic\n";
    for stated in [Some("text/plain"), Some("Text/Plain; charset=UTF-8"), None] {
        let a = Server::start().await;
        mount(&a, "PUT", at.clone(), ResponseTemplate::new(200)).await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let mut request = Request::put(&at).header(ENDPOINT, ENDPOINT_A);
        if let Some(stated) = stated {
            request = request.header(header::CONTENT_TYPE, stated);
        }
        let request = request.body(Body::from(sent))?;
        let (status, headers, body) = exchange(over(dir.path(), &a, &b)?, request).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(
            StatusCode::OK,
            status,
            "{stated:?}: §12.7 registry-not-offered: {text}"
        );
        acted(&headers, ENDPOINT_A, "cdr-a.example.org");
        let requests = a.received_requests().await.ok_or("recording is on")?;
        let [stored] = requests.as_slice() else {
            return Err(
                format!("{stated:?}: one request at node A, not {}", requests.len()).into(),
            );
        };
        assert_eq!(
            sent.as_bytes(),
            stored.body.as_slice(),
            "{stated:?}: byte-identical"
        );
        assert_eq!(
            Some("text/plain"),
            field(&stored.headers, "content-type"),
            "{stated:?}: the media type the operation's body is declared in"
        );
        assert!(
            asked(&b).await?.is_empty(),
            "{stated:?}: node B is never asked"
        );
    }
    Ok(())
}

// conformance: CP-34
#[tokio::test]
async fn without_the_registry_a_versioned_stored_query_put_in_another_media_type_is_refused()
-> TestResult {
    let a = Server::start().await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let request = Request::put(format!("/v1/definition/query/{QUERY}/1.0.0"))
        .header(ENDPOINT, ENDPOINT_A)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"q":"SELECT c FROM EHR e CONTAINS COMPOSITION c"}"#,
        ))?;
    let (status, _, body) = exchange(over(dir.path(), &a, &b)?, request).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{text}");
    assert_eq!("media-type-unsupported", error_body(&text)?.code, "{text}");
    assert!(asked(&a).await?.is_empty(), "node A received nothing");
    assert!(asked(&b).await?.is_empty(), "node B received nothing");
    Ok(())
}

// conformance: CP-34 CP-26
#[tokio::test]
async fn a_malformed_declared_value_is_refused_before_the_missing_target() -> TestResult {
    let cases = [
        (
            Request::get(format!("{ADL14}?offset=first")).body(Body::empty())?,
            StatusCode::BAD_REQUEST,
            "parameter-value-invalid",
        ),
        (
            Request::get(ADL14)
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())?,
            StatusCode::NOT_ACCEPTABLE,
            "media-type-not-acceptable",
        ),
        (
            Request::post(ADL14)
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(template()))?,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "media-type-unsupported",
        ),
        (
            Request::post("/v1/ehr")
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from("{}"))?,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "media-type-unsupported",
        ),
    ];
    for (request, refused, code) in cases {
        let at = request.uri().to_string();
        let a = Server::start().await;
        let b = Server::start().await;
        let dir = tempfile::tempdir()?;
        let (status, _, body) = exchange(over(dir.path(), &a, &b)?, request).await?;
        let text = String::from_utf8(body)?;
        assert_eq!(refused, status, "{at}: {text}");
        assert_eq!(code, error_body(&text)?.code, "{at}: {text}");
        assert!(asked(&a).await?.is_empty(), "{at}: node A received nothing");
        assert!(asked(&b).await?.is_empty(), "{at}: node B received nothing");
    }
    Ok(())
}

// conformance: CP-34 CP-40
#[tokio::test]
async fn the_registry_keeps_its_stored_queries_and_templates_still_route_to_one_node() -> TestResult
{
    let a = Server::start().await;
    mount(
        &a,
        "GET",
        ADL2.to_owned(),
        ResponseTemplate::new(200).set_body_raw(b"[]".to_vec(), "application/json"),
    )
    .await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let app = crate::stored::gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    for target in [None, Some(ENDPOINT_B)] {
        let version = if target.is_some() { "1.0.1" } else { "1.0.0" };
        let mut request = Request::put(format!("/v1/definition/query/{QUERY}/{version}"))
            .header(header::CONTENT_TYPE, "text/plain");
        if let Some(target) = target {
            request = request.header(ENDPOINT, target);
        }
        let request = request.body(Body::from("SELECT c FROM EHR e CONTAINS COMPOSITION c"))?;
        let (status, _, body) = exchange(app.clone(), request).await?;
        let text = String::from_utf8(body)?;
        if target.is_some() {
            assert_eq!(
                StatusCode::BAD_REQUEST,
                status,
                "§12.7: distribution is not offered, never answered as a plain store: {text}"
            );
            assert_eq!("stored-query-fan-out-unsupported", error_body(&text)?.code);
        } else {
            assert_eq!(
                StatusCode::OK,
                status,
                "§12.7: stored at the gateway: {text}"
            );
        }
    }
    let (status, headers, _) = exchange(
        app.clone(),
        Request::get(format!("/v1/definition/query/{QUERY}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the registry lists without a target"
    );
    assert_eq!(None, field(&headers, ENDPOINT), "the gateway answered");
    assert!(asked(&b).await?.is_empty(), "no definition reached node B");
    assert!(asked(&a).await?.is_empty(), "no definition reached node A");
    let (status, headers, _) = exchange(
        app.clone(),
        definition(&Method::GET, ADL2, Some(ENDPOINT_A), "")?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "§7a.2 definition-area-split");
    acted(&headers, ENDPOINT_A, "cdr-a.example.org");
    let (status, _, body) = exchange(app, definition(&Method::GET, ADL2, None, "")?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!(
        "target-required",
        error_body(&String::from_utf8(body)?)?.code
    );
    assert_eq!(
        vec![("GET".to_owned(), ADL2.to_owned())],
        asked(&a).await?,
        "one template list, at node A"
    );
    assert!(asked(&b).await?.is_empty());
    Ok(())
}
