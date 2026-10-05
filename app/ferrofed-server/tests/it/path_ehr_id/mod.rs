// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Routing a path `ehr_id` in the order of §12.5.1 (N41, CP-33): the
//! targeting headers, a resolution binding the client session holds, the
//! `ehr_id` index, and for a read only, the ask-all probe of every member.
//!
//! The order itself is read through
//! [`ferrofed_server::facade::owner::located`], where a test can hold a
//! session's bindings; the probe, the index learning and the refusals are
//! driven over HTTP against two mock nodes, and every assertion on what a
//! node received reads the node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod order;
mod probe;
mod refusals;

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::incident::Detection;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::facade::owner::{Located, Step};
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, HeaderValue, Request, StatusCode};
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, gateway, registry};
use crate::support::{error_body, mount, send};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// The endpoints of node A and node B in [`registry`].
pub(crate) const ENDPOINT_A: &str = "node-a-pub";
const ENDPOINT_B: &str = "node-b-pub";

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The client's own credential, which no node ever sees.
use crate::support::CLIENT_TOKEN;

/// The registry of node A and node B, at addresses nothing listens on.
fn snapshot() -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&registry(
        "http://127.0.0.1:9/a",
        "http://127.0.0.1:9/b",
        "",
    ))?)
}

fn ehr() -> Result<EhrId, Box<dyn Error>> {
    Ok(EHR_A.parse()?)
}

fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

/// Headers naming `endpoint` as the explicit target.
fn targeting(endpoint: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "openEHR-federation-endpoint",
        HeaderValue::from_static(endpoint),
    );
    headers
}

/// The endpoint and the step `located` names, or `None` for no owner.
fn named(located: &Located<'_>) -> Option<(String, Step)> {
    match located {
        Located::At { endpoint, step } => Some((endpoint.id().as_str().to_owned(), *step)),
        Located::Unreachable { .. } | Located::Collision(_) | Located::Unknown => None,
    }
}

/// The claiming endpoints and the step `located` names, or `None` for no
/// collision.
pub(crate) fn collision(located: &Located<'_>) -> Option<(Vec<String>, Detection)> {
    match located {
        Located::Collision(claimed) => Some((
            claimed
                .claimants
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            claimed.detection,
        )),
        Located::At { .. } | Located::Unreachable { .. } | Located::Unknown => None,
    }
}

/// A node that holds the EHR of [`EHR_A`] and the composition of
/// [`VERSION_A`] in it.
pub(crate) async fn holder() -> Server {
    let server = Server::start().await;
    mount(
        &server,
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"ehr_id":{{"value":"{EHR_A}"}}}}"#).into_bytes(),
            "application/json",
        ),
    )
    .await;
    mount(
        &server,
        "GET",
        format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"),
        ResponseTemplate::new(200)
            .set_body_raw(br#"{"_type":"COMPOSITION"}"#.to_vec(), "application/json"),
    )
    .await;
    mount(
        &server,
        "POST",
        format!("/v1/ehr/{EHR_A}/composition"),
        ResponseTemplate::new(201),
    )
    .await;
    server
}

/// A node that holds no EHR at all: it answers `404` to everything.
pub(crate) async fn stranger() -> Server {
    Server::start().await
}

/// The gateway over node A at `a` and node B at `b`.
pub(crate) fn over(
    dir: &std::path::Path,
    a: &Server,
    b: &Server,
) -> Result<Router, Box<dyn Error>> {
    gateway(dir, &registry(&a.uri(), &b.uri(), ""), "", "")
}

pub(crate) fn probe_at() -> (String, String) {
    ("GET".to_owned(), format!("/v1/ehr/{EHR_A}"))
}

/// The status, the acting endpoint and the body text of `request` sent to
/// `app`.
pub(crate) async fn answer(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, Option<String>, String), Box<dyn Error>> {
    let response = send(app, request).await?;
    let status = response.status();
    let acting = response
        .headers()
        .get("openEHR-federation-endpoint")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, acting, String::from_utf8(bytes.to_vec())?))
}

/// The status and the code `request` is refused with, against a holder at A
/// and `b`, and every request each node received.
async fn refused_with(
    request: Request<Body>,
    b: Server,
) -> Result<(StatusCode, String, String, Server, Server), Box<dyn Error>> {
    let a = holder().await;
    let dir = tempfile::tempdir()?;
    let (status, acting, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert!(acting.is_none(), "no endpoint acted: {text}");
    let code = error_body(&text)?.code;
    Ok((status, code, text, a, b))
}
