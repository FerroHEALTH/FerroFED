// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node routing against two mock nodes: a request to an EHR resource
//! under a path `ehr_id` reaches the one node the
//! `openEHR-federation-endpoint` header names, byte-identical, and its answer
//! comes back as the node sent it, `Location` and `ETag` untouched, with the
//! acting endpoint and its `system_id` in the response headers (§7a.1, §7a.3,
//! §9.6, §11.2; N22, N31, N33). Every assertion on what a node received reads
//! the node's own capture, never the gateway's logs (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use http::{HeaderMap, Method, Request, Response, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::facade::{EHR_A, PATIENT, gateway, registry, wire};
use crate::support::{error_body, send};

type TestResult = Result<(), Box<dyn Error>>;

/// The endpoint and the `system_id` of node A in [`registry`].
const ENDPOINT_A: &str = "node-a-pub";
const SYSTEM_A: &str = "cdr-a.example.org";

/// A version uid node A minted, as its `ETag` and `Location` name it.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The onward credential the gateway holds for node A.
const ONWARD_TOKEN: &str = "synthetic-onward-token";

/// The client's own credential, which no node ever sees.
const CLIENT_TOKEN: &str = "synthetic-client-token";

/// A `COMPOSITION` carrying the patient's identifier as a `DV_IDENTIFIER` in
/// its content, written with spacing and member order a re-serialisation
/// would not keep.
fn composition() -> String {
    format!(
        "{{ \"_type\":\"COMPOSITION\",\n  \"name\" : {{\"value\":\"Synthetic summary\",\"_type\":\"DV_TEXT\"}},\n  \"composer\":{{\"_type\":\"PARTY_IDENTIFIED\",\"name\":\"Synthetic clinician\",\n   \"identifiers\":[{{\"_type\":\"DV_IDENTIFIER\",\"issuer\":\"urn:oid:2.999.1\",\"assigner\":\"urn:oid:2.999.1\",\"id\":\"{PATIENT}\",\"type\":\"MR\"}}]}},\n  \"note\": \"é ünïcode   kept\"\n}}\n"
    )
}

/// The digest the capture is compared by.
fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// A node answering `verb` at `at` with `answer`.
async fn node(verb: &str, at: String, answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// A node that answers nothing, so it can only show it was never asked.
async fn silent() -> MockServer {
    MockServer::start().await
}

/// The gateway over node A at `a` and node B at `b`, with an onward bearer
/// token for node A and the registry `extra` appended.
fn gateway_over(
    dir: &std::path::Path,
    a: &str,
    b: &str,
    extra: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(a, b, extra),
        "",
        &format!("[credentials.\"{ENDPOINT_A}\"]\nbearer_token = \"{ONWARD_TOKEN}\"\n"),
    )
}

/// A request of `verb` to `uri` that names node A as its target.
fn to_a(verb: Method, uri: &str, body: Body) -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method(verb)
        .uri(uri)
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .header(header::AUTHORIZATION, format!("Bearer {CLIENT_TOKEN}"))
        .body(body)
}

/// The status, the headers and the body bytes of `response`.
async fn parts(
    response: Response<Body>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), Box<dyn Error>> {
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, bytes.to_vec()))
}

/// Asserts that `headers` name node A as the acting endpoint (N31, §9.6).
fn names_node_a(headers: &HeaderMap, case: &str) {
    assert_eq!(
        Some(ENDPOINT_A),
        headers
            .get("openEHR-federation-endpoint")
            .and_then(|v| v.to_str().ok()),
        "{case}: the acting endpoint (N31)"
    );
    assert_eq!(
        Some(SYSTEM_A),
        headers
            .get("openEHR-federation-system-id")
            .and_then(|v| v.to_str().ok()),
        "{case}: the node's system_id (§9.6)"
    );
}

/// The one request `server` received, or an error.
async fn only_request(server: &MockServer) -> Result<wiremock::Request, Box<dyn Error>> {
    let mut requests = server.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "the node is asked exactly once");
    requests.pop().ok_or_else(|| "one request".into())
}

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
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
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

/// The status and the code of a refused routed request, with nothing sent to
/// either node.
async fn refused(
    request: Request<Body>,
    extra: &str,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let a = silent().await;
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let (status, headers, body) = parts(
        send(
            gateway_over(dir.path(), &a.uri(), &b.uri(), extra)?,
            request,
        )
        .await?,
    )
    .await?;
    let text = String::from_utf8(body)?;
    for server in [&a, &b] {
        assert!(
            server
                .received_requests()
                .await
                .ok_or("recording")?
                .is_empty(),
            "no node is asked: {text}"
        );
    }
    assert!(
        headers.get("openEHR-federation-endpoint").is_none(),
        "nothing was routed: {text}"
    );
    Ok((status, error_body(&text)?.code))
}

#[tokio::test]
async fn a_write_that_names_no_node_is_a_400_target_required_and_probes_nobody() -> TestResult {
    for verb in [Method::POST, Method::PUT, Method::DELETE] {
        let at = if verb == Method::POST {
            format!("/v1/ehr/{EHR_A}/composition")
        } else {
            format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}")
        };
        let request = Request::builder()
            .method(verb.clone())
            .uri(at)
            .body(Body::from(composition()))?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, "target-required".to_owned()),
            refused(request, "").await?,
            "{verb} (§12.5.1, N41)"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_endpoint_header_names_exactly_one_endpoint_the_registry_holds() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for (named, code) in [
        ("node-c-pub", "endpoint-unknown"),
        ("node-a", "endpoint-unknown"),
        ("", "endpoint-unknown"),
        ("node-a-pub, node-b-pub", "endpoint-several"),
    ] {
        let request = Request::get(&resource)
            .header("openEHR-federation-endpoint", named)
            .body(Body::empty())?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, code.to_owned()),
            refused(request, "").await?,
            "{named:?} (§8.4.1)"
        );
    }
    let repeated = Request::get(&resource)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .header("openEHR-federation-endpoint", "node-b-pub")
        .body(Body::empty())?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "endpoint-several".to_owned()),
        refused(repeated, "").await?
    );
    Ok(())
}

#[tokio::test]
async fn a_suspended_endpoint_is_never_contacted() -> TestResult {
    let suspended = "\n[[endpoint]]\nid = \"node-a-old\"\nnode = \"node-a\"\nurl = \"http://127.0.0.1:9\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\nstatus = \"suspended\"\n";
    let request = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", "node-a-old")
        .body(Body::empty())?;
    assert_eq!(
        (StatusCode::NOT_FOUND, "no-destination".to_owned()),
        refused(request, suspended).await?
    );
    Ok(())
}

#[tokio::test]
async fn a_read_that_names_no_node_is_not_routed_yet() -> TestResult {
    let request = Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?;
    assert_eq!(
        (StatusCode::NOT_IMPLEMENTED, "not-implemented".to_owned()),
        refused(request, "").await?
    );
    Ok(())
}

/// The status, the code and the headers of a routed request to node A at
/// `a`, answered by the gateway on the node's behalf.
async fn failed_at(a: &str) -> Result<(StatusCode, String, HeaderMap), Box<dyn Error>> {
    let b = silent().await;
    let dir = tempfile::tempdir()?;
    let request = to_a(Method::GET, &format!("/v1/ehr/{EHR_A}"), Body::empty())?;
    let (status, headers, body) =
        parts(send(gateway_over(dir.path(), a, &b.uri(), "")?, request).await?).await?;
    let text = String::from_utf8(body)?;
    assert!(
        b.received_requests().await.ok_or("recording")?.is_empty(),
        "node B is never asked"
    );
    Ok((status, error_body(&text)?.code, headers))
}

#[tokio::test]
async fn an_unreachable_node_is_a_504_naming_the_endpoint() -> TestResult {
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = closed.local_addr()?.port();
    drop(closed);
    let (status, code, headers) = failed_at(&format!("http://127.0.0.1:{port}")).await?;
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-unreachable"),
        (status, code.as_str()),
        "§11.2"
    );
    names_node_a(&headers, "unreachable");
    Ok(())
}

#[tokio::test]
async fn a_node_that_does_not_answer_in_time_is_a_504_naming_the_endpoint() -> TestResult {
    let a = node(
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(200).set_delay(Duration::from_millis(2600)),
    )
    .await;
    let (status, code, headers) = failed_at(&a.uri()).await?;
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-timeout"),
        (status, code.as_str()),
        "§11.2"
    );
    names_node_a(&headers, "time-out");
    Ok(())
}

#[tokio::test]
async fn a_node_refusing_the_onward_credentials_is_a_424_never_a_challenge_to_the_client()
-> TestResult {
    let a = node(
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr-a\""),
    )
    .await;
    let (status, code, headers) = failed_at(&a.uri()).await?;
    assert_eq!(
        (StatusCode::FAILED_DEPENDENCY, "node-refused"),
        (status, code.as_str())
    );
    assert!(
        headers.get(header::WWW_AUTHENTICATE).is_none(),
        "the node's challenge is not the client's"
    );
    names_node_a(&headers, "401");
    Ok(())
}
