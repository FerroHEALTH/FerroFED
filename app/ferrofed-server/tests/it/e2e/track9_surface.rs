// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 9's self-description, DEMOGRAPHIC, definition and stored-query
//! clauses over two FerroEHR nodes: `OPTIONS {base}/` lists the members
//! behind the gateway, DEMOGRAPHIC answers as declared, a definition request
//! routes to the one node the client names, and a stored query held at the
//! gateway runs by name over both members (§7a, §12.6, §12.7, §16.3 track 9;
//! N30, N32, N43, N44; CP-23, CP-25, CP-34, CP-40).
//!
//! The read and the write of a plain client at a prefixed base are in
//! `e2e::track9`.

use std::error::Error;
use std::path::Path;

use axum::body::Body;
use ferrofed_testkit::containers::{self, API_PATH, TwoNodes};
use ferrofed_testkit::proxy::Fault;
use http::{Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use openehr_federation::options::OptionsRoot;
use serde::Deserialize;

use crate::e2e::scenario::{
    FederationMeta, Options, asked, clear, exchange, gateway_with, nobody_asked, seed_both,
};
use crate::e2e::track9::seeded;
use crate::e2e::{PATIENT, TestResult};

/// The qualified name the stored-query scenarios store under.
const NAME: &str = "org.example::patient_compositions";

/// The qualified name of the directed definition.
const DIRECTED: &str = "org.example::node_a_compositions";

/// The version every stored-query scenario stores.
const VERSION: &str = "1.0.0";

/// The patient's compositions over `from`, the identifier bound through
/// `$patient` so the definition the registry holds carries none (§5.4.1).
fn parameterised(from: &str) -> String {
    format!(
        "SELECT c/uid/value AS uid FROM {from} CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        PATIENT.namespace()
    )
}

/// `PUT {base}/v1/definition/query/{name}/{VERSION}` with `aql`, naming
/// `target` in the endpoint header when given.
fn put(name: &str, aql: &str, target: Option<&str>) -> Result<Request<Body>, http::Error> {
    let mut request = Request::put(format!("/v1/definition/query/{name}/{VERSION}"))
        .header(header::CONTENT_TYPE, "text/plain");
    if let Some(target) = target {
        request = request.header(ENDPOINT, target);
    }
    request.body(Body::from(aql.to_owned()))
}

/// `POST {base}/v1/query/{name}` binding `$patient`.
fn invoke(name: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("/v1/query/{name}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"query_parameters":{{"patient":"{}"}}}}"#,
            PATIENT.value()
        )))
}

/// A gateway over `nodes` holding its stored queries in `dir`, with
/// `federation` among its keys.
fn registry_gateway(
    dir: &Path,
    nodes: &TwoNodes,
    federation: &str,
) -> Result<axum::Router, Box<dyn Error>> {
    let store = toml::Value::String(dir.join("definitions.redb").display().to_string());
    let options = Options {
        federation: federation.to_owned(),
        tables: format!("[stored_queries]\npath = {store}\n"),
        ..Options::default()
    };
    gateway_with(dir, nodes, &options)
}

// conformance: CP-23 CP-25 track-9
#[tokio::test]
async fn the_self_description_lists_the_members_and_demographic_answers_as_declared() -> TestResult
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = Box::pin(seeded()).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    let described = exchange(&app, Request::options("/").body(Body::empty())?).await?;
    assert_eq!(
        StatusCode::OK,
        described.status,
        "CP-23: {}",
        described.text
    );
    crate::facade::schema::validate_options(&described.text)?;
    let root: OptionsRoot = serde_json::from_str(&described.text)?;
    let members: Vec<(&str, Option<&str>)> = root
        .endpoints
        .iter()
        .map(|member| (member.id.as_str(), member.system_id.as_deref()))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", Some(containers::NODE_A_SYSTEM_ID)),
            ("node-b-pub", Some(containers::NODE_B_SYSTEM_ID)),
        ],
        members,
        "CP-23: the member endpoints behind the gateway, asked for no patient"
    );
    assert_eq!(
        "unsupported: 501",
        root.federation.its_rest.demographic.as_str(),
        "CP-25: the declaration the behaviour below is held to"
    );

    let read = Request::get("/v1/demographic/person/synthetic-party").body(Body::empty())?;
    let answered = exchange(&app, read).await?;
    assert_eq!(
        StatusCode::NOT_IMPLEMENTED,
        answered.status,
        "CP-25: the DEMOGRAPHIC API is not federated: {}",
        answered.text
    );
    nobody_asked(&nodes, "CP-23, CP-25: neither request asks a node");
    Ok(())
}

// conformance: CP-34 track-9
#[tokio::test]
async fn a_definition_request_routes_to_the_one_named_node() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = Box::pin(seeded()).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;
    let templates = "/v1/definition/template/adl1.4";

    let named = Request::get(templates)
        .header(ENDPOINT, "node-a-pub")
        .body(Body::empty())?;
    let listed = exchange(&app, named).await?;
    assert_eq!(StatusCode::OK, listed.status, "CP-34: {}", listed.text);
    assert_eq!(Some("node-a-pub"), listed.field(ENDPOINT), "N31");
    assert_eq!(
        vec![("GET".to_owned(), format!("{API_PATH}{templates}"))],
        asked(&nodes.a),
        "CP-34: the one explicitly chosen node answers"
    );
    assert!(asked(&nodes.b).is_empty(), "CP-34: no merged catalogue");

    for target in [None, Some("*")] {
        clear(&nodes);
        let mut request = Request::get(templates);
        if let Some(target) = target {
            request = request.header(ENDPOINT, target);
        }
        let refused = exchange(&app, request.body(Body::empty())?).await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused.status,
            "CP-34: {target:?} chooses no one node: {}",
            refused.text
        );
        nobody_asked(&nodes, "CP-34: a refused definition request asks no node");
    }
    Ok(())
}

// conformance: CP-40 track-9
#[tokio::test]
async fn a_stored_query_is_held_at_the_gateway_and_runs_by_name_over_both_members() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = registry_gateway(dir.path(), &nodes, "")?;

    let stored = exchange(&app, put(NAME, &parameterised("EHR e"), None)?).await?;
    assert_eq!(StatusCode::OK, stored.status, "CP-40: {}", stored.text);
    let ran = exchange(&app, invoke(NAME)?).await?;
    assert_eq!(StatusCode::OK, ran.status, "CP-40: {}", ran.text);
    let answer = ran.federated()?;
    assert_eq!(
        Some(NAME),
        answer.name.as_deref(),
        "CP-40: the ITS-REST name member names the gateway's query"
    );
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        answer.statuses(),
        "CP-40: invoked by name, it fans out"
    );
    assert_eq!(2, answer.rows.len(), "rows from both members");
    for node in [&nodes.a, &nodes.b] {
        assert!(
            asked(node)
                .iter()
                .all(|(verb, path)| verb == "POST" && path.ends_with("/v1/query/aql")),
            "the definition stays at the gateway; a node receives only its query"
        );
        assert!(!node.proxy.journal_contains(PATIENT.value().as_bytes()));
    }

    clear(&nodes);
    let again = exchange(&app, put(NAME, &parameterised("EHR e"), None)?).await?;
    assert_eq!(
        StatusCode::CONFLICT,
        again.status,
        "CP-40: a second PUT to the same name and version is refused: {}",
        again.text
    );
    nobody_asked(&nodes, "CP-40: the refused PUT reaches no node");

    let directed = parameterised(r#"ENDPOINT ["node-a-pub"] CONTAINS EHR e"#);
    let stored = exchange(&app, put(DIRECTED, &directed, None)?).await?;
    assert_eq!(
        StatusCode::OK,
        stored.status,
        "CP-40: storable: {}",
        stored.text
    );
    let ran = exchange(&app, invoke(DIRECTED)?).await?;
    assert_eq!(StatusCode::OK, ran.status, "CP-40: {}", ran.text);
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "excluded")],
        ran.federated()?.statuses(),
        "CP-40: a directed definition stays federated-executable"
    );
    Ok(())
}

/// The answer to a definition fan-out: the per-member record.
#[derive(Debug, Deserialize)]
struct Distributed {
    meta: DistributedMeta,
}

/// The `meta` of [`Distributed`].
#[derive(Debug, Deserialize)]
struct DistributedMeta {
    federation: FederationMeta,
}

// conformance: CP-40 track-9
#[tokio::test]
async fn a_definition_fan_out_reports_a_one_node_rejection_as_partial() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = Box::pin(seeded()).await?;
    let dir = tempfile::tempdir()?;
    let app = registry_gateway(dir.path(), &nodes, "fan_out_stored_queries = true")?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));

    let fanned = exchange(&app, put(NAME, &parameterised("EHR e"), Some("*"))?).await?;
    assert_eq!(
        StatusCode::MULTI_STATUS,
        fanned.status,
        "CP-40: a one-node rejection is a partial success, never overall success: {}",
        fanned.text
    );
    let reported: Distributed = serde_json::from_str(&fanned.text)?;
    assert!(
        !reported.meta.federation.complete,
        "CP-40: complete is false"
    );
    let per_node: Vec<(&str, &str)> = reported
        .meta
        .federation
        .endpoints
        .iter()
        .map(|record| (record.id.as_str(), record.status.as_str()))
        .collect();
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "node-error")],
        per_node,
        "CP-40: reported per node"
    );
    let puts = asked(&nodes.a)
        .into_iter()
        .filter(|(verb, _)| verb == "PUT")
        .count();
    assert_eq!(1, puts, "node A received the definition once");

    clear(&nodes);
    nodes.b.proxy.clear_fault();
    let directed = parameterised(r#"ENDPOINT ["node-a-pub"] CONTAINS EHR e"#);
    let refused = exchange(&app, put(DIRECTED, &directed, Some("*"))?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        refused.status,
        "CP-40: a FROM ENDPOINT definition is refused for fan-out with a reason: {}",
        refused.text
    );
    assert!(
        !refused.code()?.is_empty(),
        "the refusal carries its reason"
    );
    nobody_asked(&nodes, "CP-40: the refused fan-out reaches no node");
    Ok(())
}
