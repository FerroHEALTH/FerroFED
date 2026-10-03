// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 10 against two mock nodes, each behind the same capturing proxy the
//! end-to-end harness puts in front of a FerroEHR node, so the normal suite
//! judges every case on the same journal the container run does (§16.3
//! track 10; N33, N34; CP-26).

use std::error::Error;

use axum::Router;
use axum::routing::get;
use ferrofed_testkit::proxy::CapturingProxy;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::cases;
use super::{
    Case, EHR_A, EHR_B, ENDPOINT_A, ENDPOINT_B, Member, PATIENT, TestResult, Topology,
    assert_logs_clean, commit_lands_byte_identical, run_all,
};
use crate::support::{Logs, send};

/// Two mock nodes and the proxy in front of each.
struct MockNodes {
    /// Node A and node B, kept alive while their proxies forward to them.
    _servers: [MockServer; 2],
    /// The proxy in front of node A.
    a: CapturingProxy,
    /// The proxy in front of node B.
    b: CapturingProxy,
}

/// A node that answers the ITS-REST query with one row, the EHR read, and a
/// commit with the `Location` and `ETag` of the version it minted.
async fn node(system_id: &str) -> MockServer {
    let server = MockServer::start().await;
    let rows = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["uid::{system_id}::1"]]}}"##
    );
    Mock::given(method("POST"))
        .and(path_regex(r"/v1/query/aql$"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(rows.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/v1/ehr/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"system_id":{{"value":"{system_id}"}}}}"#).into_bytes(),
            "application/json",
        ))
        .mount(&server)
        .await;
    let version = format!("8849182c-82ad-4088-a07f-48ead4180515::{system_id}::1");
    Mock::given(method("POST"))
        .and(path_regex(r"/v1/ehr/[^/]+/composition$"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("etag", format!("\"{version}\"").as_str())
                .insert_header(
                    "location",
                    format!("/v1/ehr/{EHR_A}/composition/{version}").as_str(),
                ),
        )
        .mount(&server)
        .await;
    server
}

impl MockNodes {
    /// Starts both nodes and their proxies.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let a = node("cdr-a.example.org").await;
        let b = node("cdr-b.example.org").await;
        let proxy_a = CapturingProxy::start(a.uri()).await?;
        let proxy_b = CapturingProxy::start(b.uri()).await?;
        Ok(Self {
            _servers: [a, b],
            a: proxy_a,
            b: proxy_b,
        })
    }

    /// The two nodes as the suite addresses them.
    fn topology(&self) -> Topology<'_> {
        Topology {
            a: Member {
                endpoint: ENDPOINT_A,
                proxy: &self.a,
                api_root: self.a.origin().to_owned(),
                ehr_id: EHR_A,
            },
            b: Member {
                endpoint: ENDPOINT_B,
                proxy: &self.b,
                api_root: self.b.origin().to_owned(),
                ehr_id: EHR_B,
            },
            real: false,
        }
    }
}

/// Runs `cases` undirected and directed at node A through a gateway over two
/// mock nodes.
async fn over_mock_nodes(cases: Vec<Case>) -> TestResult {
    let nodes = MockNodes::start().await?;
    let topology = nodes.topology();
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;
    let mut all = cases.clone();
    all.extend(cases::directed(cases));
    run_all(&app, &topology, &all).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_in_an_external_ref_predicate_reaches_no_node() -> TestResult {
    over_mock_nodes(cases::in_external_ref()).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_in_a_party_identified_predicate_reaches_no_node() -> TestResult {
    over_mock_nodes(cases::in_party_identified()).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_in_a_projection_reaches_no_node() -> TestResult {
    over_mock_nodes(cases::in_projection()).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_in_the_query_string_or_a_header_reaches_no_node() -> TestResult {
    over_mock_nodes(cases::in_query_string_or_header()).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_on_the_single_node_route_reaches_no_node() -> TestResult {
    let nodes = MockNodes::start().await?;
    let topology = nodes.topology();
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;
    run_all(&app, &topology, &cases::on_the_route(EHR_A)).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_identifier_in_the_ehr_id_slot_is_never_probed_at_a_node() -> TestResult {
    let nodes = MockNodes::start().await?;
    let topology = nodes.topology();
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;
    run_all(&app, &topology, &cases::in_the_ehr_id_slot()).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_committed_dv_identifier_arrives_at_the_node_byte_identical() -> TestResult {
    let nodes = MockNodes::start().await?;
    let topology = nodes.topology();
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;
    let composition = crate::e2e::composition_carrying(PATIENT)?;
    commit_lands_byte_identical(&app, &topology, &composition).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_panicking_handler_holding_the_identifier_leaves_it_in_no_log_line() -> TestResult {
    let value = PATIENT.value();
    let held = value.clone();
    let router = ferrofed_server::with_middleware(
        Router::new().route(
            "/boom",
            get(move || async move {
                panic!("the handler held {held}");
                #[expect(unreachable_code, reason = "the route panics by design")]
                StatusCode::OK
            }),
        ),
        &crate::support::settings(),
    );
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let response = send(
        router,
        Request::get(format!("/boom?patient={value}"))
            .header("x-request-id", value.as_str())
            .header("x-patient", value.as_str())
            .body(axum::body::Body::empty())?,
    )
    .await?;
    drop(guard);
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, response.status());
    let text = logs.text();
    assert!(
        text.contains("the request handler panicked"),
        "the panic line is in the log, so the search is not vacuous: {text}"
    );
    assert_logs_clean(&text, 1)
}
