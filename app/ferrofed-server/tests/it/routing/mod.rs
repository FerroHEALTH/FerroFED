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

mod failures;
mod hygiene;
mod pass_through;
mod selection;

use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, Method, Request, Response, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, PATIENT, gateway, registry};
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
use crate::support::CLIENT_TOKEN;

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
async fn node(verb: &str, at: String, answer: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// A node that answers nothing, so it can only show it was never asked.
async fn silent() -> Server {
    Server::start().await
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
        .header(header::AUTHORIZATION, format!("Bearer {}", *CLIENT_TOKEN))
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
async fn only_request(server: &Server) -> Result<wiremock::Request, Box<dyn Error>> {
    let mut requests = server.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "the node is asked exactly once");
    requests.pop().ok_or_else(|| "one request".into())
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
