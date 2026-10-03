// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 10 against two mock nodes, each behind the same capturing proxy the
//! end-to-end harness puts in front of a FerroEHR node, so the normal suite
//! judges every case on the same journal the container run does (§16.3
//! track 10; N33, N34; CP-26).

use std::error::Error;
use std::path::Path;
use std::process::Command;

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
use crate::support::{LogLine, Logs, lines, request_lines, send};

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

/// The variable that makes the panic case run as the child process its
/// parent spawns, naming the file the child writes its log to.
const PANIC_CHILD_LOG: &str = "FERROFED_TRACK10_PANIC_LOG";

/// The name of the panic case, as the test binary filters on it.
const PANIC_CASE: &str =
    "a_panicking_handler_holding_the_identifier_leaves_it_on_neither_stderr_nor_the_log";

// conformance: CP-26 track-10
#[tokio::test]
async fn a_panicking_handler_holding_the_identifier_leaves_it_on_neither_stderr_nor_the_log()
-> TestResult {
    if let Some(log) = std::env::var_os(PANIC_CHILD_LOG) {
        return panicking_child(Path::new(&log)).await;
    }
    let dir = tempfile::tempdir()?;
    let log = dir.path().join("log.jsonl");
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, path)| path);
    let output = Command::new(std::env::current_exe()?)
        .args([
            format!("{module}::{PANIC_CASE}").as_str(),
            "--exact",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env(PANIC_CHILD_LOG, &log)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the child ran the case and it passed:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.is_empty(),
        "the panic hook writes nothing to stderr, the payload least of all: {stderr}"
    );
    let needles = super::needles()?;
    let found = needles.in_text(&stdout);
    assert!(
        found.is_empty(),
        "the child's stdout holds {found:?}: {stdout}"
    );
    let text = std::fs::read_to_string(&log)?;
    assert!(
        text.contains("the request handler panicked"),
        "the renderer's line is in the log, so the search is not vacuous: {text}"
    );
    let requests = request_lines(&text)?;
    let [request] = requests.as_slice() else {
        return Err(format!("one request line: {text}").into());
    };
    let hooked: Vec<LogLine> = lines(&text)?
        .into_iter()
        .filter(|line| line.message == "a thread panicked")
        .collect();
    let [hooked] = hooked.as_slice() else {
        return Err(format!("the hook wrote one line: {text}").into());
    };
    assert!(
        hooked
            .location
            .as_deref()
            .is_some_and(|at| at.contains("track10/mock.rs:")),
        "the hook's line names where the handler panicked: {text}"
    );
    assert!(
        hooked.request_id.is_some() && hooked.request_id == request.request_id,
        "the hook's line names the request by the gateway's id: {text}"
    );
    assert_logs_clean(&text, 1)
}

/// Serves a handler that panics holding the identifier, with the identifier
/// in the query string and the headers, under the binary's panic hook, and
/// writes the log to `log`.
#[expect(clippy::panic, reason = "the route panics by design")]
async fn panicking_child(log: &Path) -> TestResult {
    ferrofed_server::panic::install_hook();
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
    std::fs::write(log, logs.text())?;
    assert_eq!(
        StatusCode::INTERNAL_SERVER_ERROR,
        response.status(),
        "the panic is still answered 500"
    );
    Ok(())
}
