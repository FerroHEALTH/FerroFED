// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node forwarding against a mock node (§7a.3, N22, N31, N33): the
//! body arrives byte for byte, only the named client headers travel, the
//! outbound gate reads the URL and the headers of a forwarded request, and the
//! answer comes back as the node sent it. Asserted on what the mock node
//! received.
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

fn options(withheld: Withheld) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline)
        .with_request_id("req-forward-1")
        .with_withheld(Arc::new(withheld)))
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
async fn the_body_arrives_byte_for_byte_and_only_the_named_headers_travel() -> TestResult {
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
    headers.insert("x-patient", PATIENT.parse()?);
    let body = format!(
        "{{\"_type\":\"COMPOSITION\",  \"identifiers\":[{{\"_type\":\"DV_IDENTIFIER\",\"id\":\"{PATIENT}\"}}]}}"
    );
    let forwarded = client(&server.uri())?
        .forward(
            request(Method::POST, &at, headers, body.as_bytes()),
            &options(Withheld::none())?,
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
    assert_eq!(
        Some("req-forward-1"),
        sent.headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
    );
    assert!(sent.headers.get("authorization").is_none());
    assert!(sent.headers.get("x-patient").is_none());
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
    let in_header = request(Method::GET, &format!("/ehr/{EHR}"), headers, b"");
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
