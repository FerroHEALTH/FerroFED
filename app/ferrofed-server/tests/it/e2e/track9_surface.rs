// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 9's self-description, DEMOGRAPHIC, definition and stored-query
//! clauses over two FerroEHR nodes: `OPTIONS {base}/` lists the members
//! behind the gateway, DEMOGRAPHIC answers as declared, a definition request
//! routes to the one node the client names, and a stored query held at the
//! gateway runs by name over both members (§7a, §12.6, §12.7, §16.3 track 9;
//! N30, N32, N43, N44; CP-23, CP-25, CP-34, CP-40).
//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track9`]); this suite adds
//! what each node was asked, and the node failure a definition fan-out
//! reports. The read and the write of a plain client at a prefixed base are
//! in `e2e::track9`.

use std::error::Error;
use std::path::Path;

use axum::body::Body;
use ferrofed_server::conformance::client::FederationMeta;
use ferrofed_server::conformance::scenarios::track9;
use ferrofed_testkit::containers::{self, API_PATH, TwoNodes};
use ferrofed_testkit::proxy::Fault;
use http::{Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use serde::Deserialize;

use crate::e2e::scenario::{
    Options, asked, clear, exchange, fixture, gateway_with, in_process, nobody_asked, seed_both,
};
use crate::e2e::track9::seeded;
use crate::e2e::{PATIENT, TestResult};

/// The qualified name the stored-query scenarios store under.
const NAME: &str = "org.example::patient_compositions";

/// The qualified name of the directed definition.
const DIRECTED: &str = "org.example::node_a_compositions";

/// The version every stored-query scenario stores.
const VERSION: &str = "1.0.0";

/// `PUT {base}/v1/definition/query/{name}/{VERSION}` with `aql`, naming
/// `target` in the endpoint header.
fn put_naming(name: &str, aql: &str, target: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/definition/query/{name}/{VERSION}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .header(ENDPOINT, target)
        .body(Body::from(aql.to_owned()))
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

    let root = track9::self_description(&in_process(&app), &fixture(Some(1), Some(0))?).await?;
    assert_eq!(
        track9::DEMOGRAPHIC_UNSUPPORTED,
        root.federation.its_rest.demographic.as_str(),
        "CP-25: the declaration the behaviour is held to"
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
    let gateway = in_process(&app);

    track9::definition_named(&gateway, "node-a-pub").await?;
    assert_eq!(
        vec![("GET".to_owned(), format!("{API_PATH}{}", track9::TEMPLATES))],
        asked(&nodes.a),
        "CP-34: the one explicitly chosen node answers"
    );
    assert!(asked(&nodes.b).is_empty(), "CP-34: no merged catalogue");

    for target in [None, Some("*")] {
        clear(&nodes);
        track9::definition_unnamed(&gateway, target).await?;
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
    let (gateway, fixture) = (in_process(&app), fixture(Some(1), Some(1))?);

    track9::store_and_run(&gateway, &fixture, (NAME, VERSION)).await?;
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
    track9::second_put_refused(&gateway, &fixture, (NAME, VERSION)).await?;
    nobody_asked(&nodes, "CP-40: the refused PUT reaches no node");

    track9::directed_store_and_run(&gateway, &fixture, (DIRECTED, VERSION), "node-a-pub").await?;
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
    let fixture = fixture(Some(1), Some(0))?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));

    let undirected = track9::parameterised(&fixture, "EHR e");
    let fanned = exchange(&app, put_naming(NAME, &undirected, "*")?).await?;
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
    let directed = track9::parameterised(&fixture, r#"ENDPOINT ["node-a-pub"] CONTAINS EHR e"#);
    let refused = exchange(&app, put_naming(DIRECTED, &directed, "*")?).await?;
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
