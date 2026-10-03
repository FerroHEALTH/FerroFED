// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node forwarding against a mock node (§7a.3, N22, N31, N33): the
//! body arrives byte for byte, only the client headers and query parameters
//! the ITS-REST operation declares travel, each held to the kind the
//! operation declares for it, the outbound gate reads the URL and
//! the headers of a forwarded request, and the answer comes back as the node
//! sent it. Asserted on what the mock node received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{DispatchOptions, NodeClient};
use ferrofed_engine::forward::{ClientRequest, ForwardError};
use ferrofed_engine::hygiene::{Part, Withheld};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, Method, StatusCode};
use openehr_its::rest::client::ReqwestTransport;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient identifier.
const PATIENT: &str = "Sentinel-7731";

/// A node-local `ehr_id`.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

fn client(url: &str) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let snapshot = RegistrySnapshot::from_toml_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    ))?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?)
}

fn options_under(
    withheld: Withheld,
    outbound: OutboundId,
) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline)
        .with_request_id(outbound)
        .with_withheld(Arc::new(withheld)))
}

fn options(withheld: Withheld) -> Result<DispatchOptions, Box<dyn Error>> {
    options_under(withheld, OutboundId::mint())
}

fn patient() -> Withheld {
    Withheld::new([SecretString::from(PATIENT)])
}

async fn node(verb: &str, at: &str, answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

fn request(verb: Method, at: &str, headers: HeaderMap, body: &[u8]) -> ClientRequest {
    ClientRequest {
        method: verb,
        path: at.to_owned(),
        query: None,
        headers,
        body: body.to_vec(),
    }
}

async fn received(server: &MockServer) -> Result<Vec<wiremock::Request>, Box<dyn Error>> {
    Ok(server.received_requests().await.ok_or("recording is on")?)
}

// conformance: CP-24
#[tokio::test]
async fn the_body_arrives_byte_for_byte_and_only_the_declared_headers_travel() -> TestResult {
    let at = format!("/ehr/{EHR}/composition");
    let server = node(
        "POST",
        &format!("/v1{at}"),
        ResponseTemplate::new(201).insert_header("ETag", "\"u::cdr-a.example.org::1\""),
    )
    .await;
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse()?);
    headers.insert("authorization", "Bearer client-token".parse()?);
    headers.insert("x-request-id", "req-client-1".parse()?);
    headers.insert("x-patient", PATIENT.parse()?);
    let body = format!(
        "{{\"_type\":\"COMPOSITION\",  \"identifiers\":[{{\"_type\":\"DV_IDENTIFIER\",\"id\":\"{PATIENT}\"}}]}}"
    );
    let outbound = OutboundId::mint();
    let forwarded = client(&server.uri())?
        .forward(
            request(Method::POST, &at, headers, body.as_bytes()),
            &options_under(Withheld::none(), outbound)?,
        )
        .await?;
    assert_eq!(StatusCode::CREATED, forwarded.status());
    assert_eq!(
        Some("\"u::cdr-a.example.org::1\""),
        forwarded
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
    );
    let requests = received(&server).await?;
    let sent = requests.first().ok_or("one request")?;
    assert_eq!(body.as_bytes(), sent.body.as_slice(), "byte for byte (N33)");
    let request_ids: Vec<&[u8]> = sent
        .headers
        .get_all("x-request-id")
        .iter()
        .map(http::HeaderValue::as_bytes)
        .collect();
    assert_eq!(
        vec![outbound.to_string().as_bytes()],
        request_ids,
        "the node receives the minted id alone, never the client's (N33)"
    );
    assert_eq!(
        Some(b"application/json".as_slice()),
        sent.headers
            .get("content-type")
            .map(http::HeaderValue::as_bytes)
    );
    assert!(sent.headers.get("authorization").is_none());
    assert!(sent.headers.get("x-patient").is_none());
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_header_the_operation_declares_travels_and_one_it_does_not_is_stripped() -> TestResult {
    let uid = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
    let at = format!("/ehr/{EHR}/composition/{uid}");
    let server = MockServer::start().await;
    for verb in ["PUT", "GET"] {
        Mock::given(method(verb))
            .and(path(format!("/v1{at}")))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
    }
    let client = client(&server.uri())?;
    let mut headers = HeaderMap::new();
    headers.insert("if-match", format!("\"{uid}\"").parse()?);
    headers.insert("x-patient", PATIENT.parse()?);
    for verb in [Method::PUT, Method::GET] {
        let sent = request(verb, &at, headers.clone(), b"");
        client.forward(sent, &options(Withheld::none())?).await?;
    }
    let requests = received(&server).await?;
    let [update, read] = requests.as_slice() else {
        return Err(format!("two requests, got {}", requests.len()).into());
    };
    assert_eq!(
        Some(format!("\"{uid}\"").as_bytes()),
        update
            .headers
            .get("if-match")
            .map(http::HeaderValue::as_bytes),
        "PUT composition declares If-Match (ITS-REST, EHR API)"
    );
    assert!(
        read.headers.get("if-match").is_none(),
        "GET composition declares no If-Match, so it is stripped"
    );
    for sent in [update, read] {
        assert!(sent.headers.get("x-patient").is_none());
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_query_parameter_the_operation_does_not_declare_is_refused_unsent() -> TestResult {
    let server = MockServer::start().await;
    let at = format!("/ehr/{EHR}/composition");
    let mut commit = request(Method::POST, &at, HeaderMap::new(), b"{}");
    commit.query = Some("version_at_time=2026-01-01T00:00:00Z".to_owned());
    let refused = client(&server.uri())?
        .forward(commit, &options(Withheld::none())?)
        .await;
    assert!(
        matches!(
            &refused,
            Err(ForwardError::QueryParameter(unlisted)) if unlisted.position == 1
        ),
        "{refused:?}"
    );
    assert!(received(&server).await?.is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_request_that_names_no_operation_is_never_sent() -> TestResult {
    let server = MockServer::start().await;
    let unrouted = request(Method::PATCH, &format!("/ehr/{EHR}"), HeaderMap::new(), b"");
    let refused = client(&server.uri())?
        .forward(unrouted, &options(Withheld::none())?)
        .await;
    assert!(
        matches!(&refused, Err(ForwardError::Unrouted)),
        "{refused:?}"
    );
    assert!(received(&server).await?.is_empty(), "nothing is sent");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_withheld_identifier_in_the_path_or_a_forwarded_header_is_never_sent() -> TestResult {
    let server = MockServer::start().await;
    let client = client(&server.uri())?;
    let in_path = request(
        Method::GET,
        &format!("/ehr/{PATIENT}"),
        HeaderMap::new(),
        b"",
    );
    let refused = client.forward(in_path, &options(patient())?).await;
    assert!(
        matches!(
            &refused,
            Err(ForwardError::Withheld {
                part: Part::Url,
                ..
            })
        ),
        "{refused:?}"
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "openehr-audit-details",
        format!("committer.id={PATIENT}").parse()?,
    );
    let in_header = request(
        Method::POST,
        &format!("/ehr/{EHR}/composition"),
        headers,
        b"",
    );
    let refused = client.forward(in_header, &options(patient())?).await;
    assert!(
        matches!(
            &refused,
            Err(ForwardError::Withheld {
                part: Part::Header("openehr-audit-details"),
                ..
            })
        ),
        "{refused:?}"
    );
    let shown = refused.err().ok_or("refused")?.to_string();
    assert!(!shown.contains(PATIENT), "{shown}");
    assert!(received(&server).await?.is_empty(), "nothing is sent");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_withheld_identifier_in_the_body_of_a_commit_is_sent_unchanged() -> TestResult {
    let at = format!("/ehr/{EHR}/composition");
    let server = node("POST", &format!("/v1{at}"), ResponseTemplate::new(201)).await;
    let body = format!("{{\"identifiers\":[{{\"id\":\"{PATIENT}\"}}]}}");
    client(&server.uri())?
        .forward(
            request(Method::POST, &at, HeaderMap::new(), body.as_bytes()),
            &options(patient())?,
        )
        .await?;
    let requests = received(&server).await?;
    assert_eq!(
        Some(body.as_bytes()),
        requests.first().map(|sent| sent.body.as_slice()),
        "the gate never reads a write body (§5.4 scope note)"
    );
    Ok(())
}

#[tokio::test]
async fn a_401_is_the_node_refusing_the_onward_credentials() -> TestResult {
    let at = format!("/ehr/{EHR}");
    let server = node(
        "GET",
        &format!("/v1{at}"),
        ResponseTemplate::new(401)
            .set_body_raw(br#"{"message":"no"}"#.to_vec(), "application/json"),
    )
    .await;
    let refused = client(&server.uri())?
        .forward(
            request(Method::GET, &at, HeaderMap::new(), b""),
            &options(Withheld::none())?,
        )
        .await;
    match refused {
        Err(ForwardError::Refused { status, body, .. }) => {
            assert_eq!(StatusCode::UNAUTHORIZED, status);
            assert_eq!(Some("no"), body.message());
        }
        other => return Err(format!("a 401 is Refused: {other:?}").into()),
    }
    Ok(())
}

/// The status a node that answers every directory read `200` gives a read of
/// the directory at `query` with `headers`, and what the node received.
async fn directory_read(
    query: &str,
    headers: HeaderMap,
) -> Result<(Result<StatusCode, ForwardError>, Vec<wiremock::Request>), Box<dyn Error>> {
    let at = format!("/ehr/{EHR}/directory");
    let server = node("GET", &format!("/v1{at}"), ResponseTemplate::new(200)).await;
    let mut read = request(Method::GET, &at, headers, b"");
    read.query = Some(query.to_owned());
    let answered = client(&server.uri())?
        .forward(read, &options(Withheld::none())?)
        .await
        .map(|answer| answer.status());
    Ok((answered, received(&server).await?))
}

// conformance: CP-26
#[tokio::test]
async fn a_malformed_date_time_is_refused_and_no_node_is_asked() -> TestResult {
    let (answered, sent) =
        directory_read(&format!("version_at_time={PATIENT}"), HeaderMap::new()).await?;
    match answered {
        Err(ForwardError::Value(malformed)) => {
            let shown = malformed.to_string();
            assert!(shown.contains("query parameter 1"), "{shown}");
            assert!(!shown.contains(PATIENT), "{shown}");
        }
        other => return Err(format!("a malformed date-time is refused: {other:?}").into()),
    }
    assert!(sent.is_empty(), "nothing is sent");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_well_formed_date_time_is_forwarded_byte_identical() -> TestResult {
    let query = "version_at_time=2015-01-20T19:30:22.765%2B01:00&path=folders%2Fone";
    let (answered, sent) = directory_read(query, HeaderMap::new()).await?;
    assert_eq!(StatusCode::OK, answered?);
    let [one] = sent.as_slice() else {
        return Err(format!("one request reaches the node, not {}", sent.len()).into());
    };
    assert_eq!(Some(query), one.url.query());
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn an_enumerated_header_outside_its_values_is_refused_unsent() -> TestResult {
    let mut headers = HeaderMap::new();
    headers.insert(
        "accept",
        format!("application/json; patient={PATIENT}").parse()?,
    );
    let (answered, sent) = directory_read("", headers).await?;
    assert!(
        matches!(&answered, Err(ForwardError::Value(_))),
        "{answered:?}"
    );
    assert!(sent.is_empty(), "nothing is sent");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_free_text_parameter_passes_unclassified() -> TestResult {
    let query = format!("path={PATIENT}");
    let (answered, sent) = directory_read(&query, HeaderMap::new()).await?;
    assert_eq!(StatusCode::OK, answered?);
    assert_eq!(
        1,
        sent.len(),
        "§5.4.1, N33: free text cannot be classified, so it travels"
    );
    Ok(())
}
