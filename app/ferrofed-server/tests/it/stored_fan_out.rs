// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query registry's definitions at the members, against three
//! mock nodes (§12.7, N44, N43, CP-40): distribution off by default and
//! never offered without the registry; offered, a `PUT` naming `*` stores at
//! the registry first and then reaches each member independently, answered
//! per node with partial success as `207` and nothing rolled back; a
//! definition carrying the `FROM ENDPOINT` directive is refused for
//! distribution; an invocation always runs the registry's AQL; and a `GET`
//! naming members reports per member whether its copy matches. Every
//! assertion on what a node received reads the node's own capture (§16,
//! track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::state::AppState;
use http::{Method, Request, StatusCode, header};
use openehr_federation::meta::FederationMeta;
use openehr_federation::options::{DefinitionBehaviour, OptionsRoot};
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::generated::definition::StoredQuery;
use serde::Deserialize;
use wiremock::{MockServer, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, registry, schema, wire};
use crate::support::{asked, call, error_body, exchange, field, mount};
use crate::template_fan_out::schema::validate_federation;

type TestResult = Result<(), Box<dyn Error>>;

/// The endpoint header (§8.4).
const ENDPOINT: &str = "openEHR-federation-endpoint";

/// The qualified name every fixture stores under.
const NAME: &str = "org.example::fanned_compositions";

/// The version every fixture stores.
const VERSION: &str = "1.0.0";

/// A comment the client writes into its definition, which no node receives.
const COMMENT: &str = "SYNTHETIC-COMMENT-5c1e";

/// A body a node answers with, which the gateway's answer never copies.
const NODE_BODY: &str = "SYNTHETIC-NODE-BODY-2d7a";

/// A literal only a member's differing copy carries.
const NODE_COPY: &str = "SYNTHETIC-NODE-COPY-7f3a";

/// The third member, beside node A and node B of [`registry`].
const THIRD: &str = r#"
[[organisation]]
id = "org-c"

[[node]]
id = "node-c"
organisation = "org-c"
system_id = "cdr-c.example.org"
"#;

/// The node path of the stored definition (ITS-REST Definition API).
fn node_path() -> String {
    format!("/v1/definition/query/{NAME}/{VERSION}")
}

/// A definition naming the patient through `$patient`, written with a
/// comment and loose layout.
fn definition() -> String {
    format!(
        "SELECT c/uid/value\n  FROM EHR e CONTAINS COMPOSITION c -- {COMMENT}\n \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The registry of node A, node B and node C at `a`, `b` and `c`.
fn three(a: &MockServer, b: &MockServer, c: &MockServer) -> String {
    let endpoint = format!(
        "{THIRD}\n[[endpoint]]\nid = \"node-c-pub\"\nnode = \"node-c\"\nurl = \"{}\"\n\
         connection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-c\"\n",
        c.uri()
    );
    registry(&a.uri(), &b.uri(), &endpoint)
}

/// A gateway over `registry` offering the registry in `dir`, with
/// `federation` in its `[federation]` table and the patient known at node A
/// and node B.
fn gateway(dir: &Path, registry: &str, federation: &str) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let document = toml::Value::String(document.display().to_string());
    let store = toml::Value::String(dir.join("definitions.redb").display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n{federation}\n\n\
         [stored_queries]\npath = {store}\n{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(ferrofed_server::router(
        Arc::new(AppState::build(&settings)?),
        &crate::facade::settings_with_room(),
    ))
}

/// The gateway over the three nodes with definition fan-out offered.
fn offered(dir: &Path, nodes: [&MockServer; 3]) -> Result<Router, Box<dyn Error>> {
    let [a, b, c] = nodes;
    gateway(dir, &three(a, b, c), "fan_out_stored_queries = true")
}

/// `PUT` of `aql` at [`NAME`] and [`VERSION`], naming `target` when given.
fn put(aql: &str, target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::put(format!("/v1/definition/query/{NAME}/{VERSION}"))
        .header(header::CONTENT_TYPE, "text/plain");
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    request.body(Body::from(aql.to_owned()))
}

/// `GET` of [`NAME`] at [`VERSION`], naming `target` when given.
fn get(target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::get(format!("/v1/definition/query/{NAME}/{VERSION}"));
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    request.body(Body::empty())
}

/// A node answering the definition `PUT` with `status` and its own body.
async fn storing(status: u16) -> MockServer {
    let server = MockServer::start().await;
    let answer = ResponseTemplate::new(status).set_body_raw(
        format!(r#"{{"message":"{NODE_BODY}"}}"#).into_bytes(),
        "application/json",
    );
    mount(&server, "PUT", node_path(), answer).await;
    server
}

/// A node holding `aql` as its copy of the definition.
async fn holding(aql: &str) -> Result<MockServer, Box<dyn Error>> {
    let server = MockServer::start().await;
    let copy = StoredQuery {
        name: NAME.to_owned(),
        r#type: "AQL".to_owned(),
        version: VERSION.to_owned(),
        saved: "2026-10-03T00:00:00Z".to_owned(),
        q: aql.to_owned(),
        additional_properties: BTreeMap::new(),
    };
    let answer =
        ResponseTemplate::new(200).set_body_raw(serde_json::to_vec(&copy)?, "application/json");
    mount(&server, "GET", node_path(), answer).await;
    Ok(server)
}

/// The answer to a distribution or a drift report: the registry's
/// definition and the per-member record.
#[derive(Debug, Deserialize)]
struct Reported {
    #[serde(flatten)]
    definition: StoredQuery,
    meta: Meta,
}

/// The `meta` of [`Reported`].
#[derive(Debug, Deserialize)]
struct Meta {
    federation: FederationMeta,
}

/// Each endpoint `meta` reports, with its status, in answer order.
fn statuses(meta: &FederationMeta) -> Vec<(String, EndpointStatus)> {
    meta.endpoints()
        .iter()
        .map(|outcome| (outcome.id().as_str().to_owned(), outcome.status()))
        .collect()
}

/// The `code` of the structured error `meta` reports for `endpoint`.
fn drift_code(meta: &FederationMeta, endpoint: &str) -> Option<String> {
    let reported = meta
        .endpoints()
        .iter()
        .find(|outcome| outcome.id().as_str() == endpoint)?;
    let Some(ErrorDetail::Object(members)) = reported.outcome().error() else {
        return None;
    };
    let code = members.get("code")?;
    serde_json::from_str::<String>(code.get()).ok()
}

/// Asserts that `server` received exactly one definition `PUT` carrying
/// `aql` as `text/plain`, and nothing else: nothing was rolled back.
async fn stored_once(server: &MockServer, aql: &str) -> TestResult {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let [only] = requests.as_slice() else {
        return Err(format!("one request, not {}", requests.len()).into());
    };
    assert_eq!(Method::PUT.as_str(), only.method.as_str(), "no rollback");
    assert_eq!(node_path(), only.url.path(), "the ITS-REST path");
    assert_eq!(Some("query_type=AQL"), only.url.query(), "the query type");
    assert_eq!(aql.as_bytes(), only.body.as_slice(), "the registry's copy");
    let composed = wire(server).await?;
    assert!(
        !composed.contains(COMMENT),
        "§5.4.2: no comment reaches a node"
    );
    assert!(!composed.contains(PATIENT), "N33");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn with_the_setting_off_a_put_naming_members_stores_at_the_registry_only() -> TestResult {
    let (a, b, c) = (storing(200).await, storing(200).await, storing(200).await);
    for federation in ["", "fan_out_stored_queries = false"] {
        let dir = tempfile::tempdir()?;
        let app = gateway(dir.path(), &three(&a, &b, &c), federation)?;
        let (status, text) = call(app.clone(), put(&definition(), Some("*"))?).await?;
        assert_eq!(StatusCode::OK, status, "§12.7: stored: {text}");
        let (status, text) = call(app, get(Some("*"))?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        let read: StoredQuery = serde_json::from_str(&text)?;
        assert_eq!(VERSION, read.version, "the registry answers alone");
    }
    for node in [&a, &b, &c] {
        assert!(
            asked(node).await?.is_empty(),
            "§12.7: nothing is distributed"
        );
    }
    Ok(())
}

// conformance: CP-40
#[test]
fn config_check_refuses_definition_fan_out_without_the_registry() -> TestResult {
    let text = "[registry]\ndocument = \"/etc/ferrofed/registry.toml\"\n\n\
                [federation]\nfan_out_stored_queries = true\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("§12.7: fan-out without the registry is refused")?;
    assert!(
        matches!(refused, ConfigError::StoredQueryFanOutWithoutRegistry),
        "{refused:?}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn with_one_member_failing_the_distribution_is_partial_and_the_registry_holds_it()
-> TestResult {
    let (a, b, c) = (storing(200).await, storing(200).await, storing(500).await);
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    let (status, headers, body) = exchange(app.clone(), put(&definition(), Some("*"))?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(
        StatusCode::MULTI_STATUS,
        status,
        "§12.6 item 3: a partial success, never overall success: {text}"
    );
    validate_federation(&text)?;
    assert!(
        !text.contains(NODE_BODY),
        "§12.6 item 2: no node body: {text}"
    );
    let answer: Reported = serde_json::from_str(&text)?;
    assert_eq!(
        (NAME, VERSION),
        (
            answer.definition.name.as_str(),
            answer.definition.version.as_str()
        ),
        "the answer names the registry's definition"
    );
    let meta = &answer.meta.federation;
    assert!(!meta.complete(), "§12.6 item 3: complete is false: {text}");
    assert_eq!(
        vec![
            ("node-a-pub".to_owned(), EndpointStatus::Active),
            ("node-b-pub".to_owned(), EndpointStatus::Active),
            ("node-c-pub".to_owned(), EndpointStatus::NodeError),
        ],
        statuses(meta),
        "§9.5: one entry per member, the failing one named: {text}"
    );
    assert_eq!(
        Some(VERSION),
        field(&headers, "location"),
        "the stored version"
    );
    assert_eq!(Some("node-a-pub, node-b-pub"), field(&headers, ENDPOINT));
    for node in [&a, &b, &c] {
        stored_once(node, &answer.definition.q).await?;
    }
    let (status, text) = call(app.clone(), get(None)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§12.7: the registry holds it: {text}"
    );
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(answer.definition.q, held.q, "the registry's copy stands");
    let (status, _) = call(app, put(&definition(), Some("*"))?).await?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "§12.7: the version is immutable"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn with_every_member_failing_the_registry_still_holds_the_definition() -> TestResult {
    let (a, b, c) = (storing(409).await, storing(400).await, storing(409).await);
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    let (status, text) = call(app.clone(), put(&definition(), Some("*"))?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "§11.2: every member refused: {text}"
    );
    let answer: Reported = serde_json::from_str(&text)?;
    assert_eq!(
        NAME, answer.definition.name,
        "the registry's definition: {text}"
    );
    let (status, _) = call(app, get(None)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§12.7: a failed distribution removes nothing"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_targeted_definition_is_refused_for_distribution_and_stays_executable() -> TestResult {
    let (a, b, c) = (storing(200).await, storing(200).await, storing(200).await);
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    for directive in [r#"ENDPOINT p ["node-a-pub"]"#, r#"ORGANISATION ["org-a"]"#] {
        let targeted = definition().replacen("FROM ", &format!("FROM {directive} CONTAINS "), 1);
        let (status, text) = call(app.clone(), put(&targeted, Some("*"))?).await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            status,
            "§12.7: {directive}: {text}"
        );
        assert_eq!("definition-endpoint-targeted", error_body(&text)?.code);
        let (status, _) = call(app.clone(), get(None)?).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    }
    for node in [&a, &b, &c] {
        assert!(
            asked(node).await?.is_empty(),
            "§12.7: nothing is distributed"
        );
    }
    let targeted = definition().replacen("FROM ", r#"FROM ENDPOINT p ["node-a-pub"] CONTAINS "#, 1);
    let (status, text) = call(app, put(&targeted, None)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§12.7: it MAY still be stored: {text}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn an_invocation_runs_the_registry_aql_even_where_a_nodes_copy_differs() -> TestResult {
    let differing = format!(
        "SELECT c/name/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = '{NODE_COPY}'"
    );
    let (a, b) = (holding(&differing).await?, holding(&differing).await?);
    let c = storing(200).await;
    for node in [&a, &b] {
        let rows = format!(
            r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["{NODE_COPY}"]]}}"##
        );
        let answer = ResponseTemplate::new(200).set_body_raw(rows.into_bytes(), "application/json");
        mount(node, "POST", format!("/v1/query/{NAME}"), answer).await;
        let rows = r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-from-the-registry-aql"]]}"##;
        let answer =
            ResponseTemplate::new(200).set_body_raw(rows.as_bytes().to_vec(), "application/json");
        mount(node, "POST", "/v1/query/aql".to_owned(), answer).await;
    }
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    let (status, text) = call(app.clone(), put(&definition(), None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let invoke = Request::post(format!("/v1/query/{NAME}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#
        )))?;
    let (status, text) = call(app, invoke).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(
        !text.contains(NODE_COPY),
        "§12.7 drift: never a node's copy: {text}"
    );
    for node in [&a, &b] {
        let paths: Vec<String> = asked(node).await?.into_iter().map(|(_, at)| at).collect();
        assert_eq!(
            vec!["/v1/query/aql".to_owned()],
            paths,
            "the registry's AQL inline"
        );
        let composed = wire(node).await?;
        assert!(
            composed.contains("c/uid/value"),
            "the registry's projection: {composed}"
        );
        assert!(!composed.contains(NODE_COPY), "§12.7: {composed}");
    }
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn the_drift_report_names_matching_differing_and_missing_members() -> TestResult {
    let differing = format!(
        "SELECT c/name/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = '{NODE_COPY}'"
    );
    let a = holding(&definition()).await?;
    let b = holding(&differing).await?;
    let c = MockServer::start().await;
    let dir = tempfile::tempdir()?;
    let app = offered(dir.path(), [&a, &b, &c])?;
    let (status, text) = call(app.clone(), put(&definition(), None)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (status, headers, body) = exchange(app.clone(), get(Some("*"))?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::MULTI_STATUS, status, "§12.7 drift: {text}");
    validate_federation(&text)?;
    assert!(
        !text.contains(NODE_COPY),
        "no node's copy is copied: {text}"
    );
    let answer: Reported = serde_json::from_str(&text)?;
    assert_eq!(
        VERSION, answer.definition.version,
        "the registry's definition"
    );
    let meta = &answer.meta.federation;
    assert_eq!(
        vec![
            ("node-a-pub".to_owned(), EndpointStatus::Active),
            ("node-b-pub".to_owned(), EndpointStatus::NodeError),
            ("node-c-pub".to_owned(), EndpointStatus::NodeError),
        ],
        statuses(meta),
        "§12.7 drift: per node: {text}"
    );
    assert_eq!(
        Some("definition-differs".to_owned()),
        drift_code(meta, "node-b-pub")
    );
    assert_eq!(
        Some("definition-missing".to_owned()),
        drift_code(meta, "node-c-pub")
    );
    assert_eq!(
        Some("node-a-pub"),
        field(&headers, ENDPOINT),
        "the matching member"
    );
    for node in [&a, &b, &c] {
        assert_eq!(
            vec![("GET".to_owned(), node_path())],
            asked(node).await?,
            "one read of the version, nothing written"
        );
    }
    let (status, text) = call(app, get(Some("node-a-pub"))?).await?;
    assert_eq!(StatusCode::OK, status, "every named member matches: {text}");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn options_declares_definition_fan_out_only_where_offered_beside_the_registry() -> TestResult
{
    let (a, b, c) = (storing(200).await, storing(200).await, storing(200).await);
    for (federation, offered) in [
        ("", false),
        ("fan_out_stored_queries = false", false),
        ("fan_out_stored_queries = true", true),
    ] {
        let dir = tempfile::tempdir()?;
        let app = gateway(dir.path(), &three(&a, &b, &c), federation)?;
        let (status, text) = call(app, Request::options("/").body(Body::empty())?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        schema::validate_options(&text)?;
        let body: OptionsRoot = serde_json::from_str(&text)?;
        assert_eq!(
            DefinitionBehaviour::new(false)
                .with_stored_query_registry(true)?
                .with_stored_query_fan_out(offered)?,
            body.federation.definition,
            "§7a.2, §12.7: {federation:?}"
        );
        let described = &body.federation.its_rest.definition;
        assert_eq!(offered, described.contains("distributed"), "{described}");
    }
    Ok(())
}
