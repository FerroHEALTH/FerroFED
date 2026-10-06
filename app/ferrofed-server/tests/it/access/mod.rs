// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log (Regulation (EU) 2025/327 Annex II 3.2): every federated
//! query, stored-query execution, routed read and routed write that reaches
//! patient data at a node is recorded as a BALP `AuditEvent` at the harness
//! Audit Record Repository, naming the verified caller, the patient, the
//! origins and the categories the deployment's map classifies it under; an
//! access whose record cannot be stored answers `503 access-unrecorded`; and
//! no record content reaches a node, the log or a metric (§5.4, N33).
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the records are read as JSON values"
)]

mod accessor;
mod address;
mod gate;
mod limits;
mod origins;
mod professional;
mod query;
mod routed;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, crossref, registry, settings_with_room};
use crate::feed_audit::audit_tables;

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic lab report template the test map gives medical test results.
const LAB_REPORT: &str = "Example Lab Report.v1";

/// A synthetic discharge template the test map gives discharge reports.
const DISCHARGE: &str = "Example Discharge.v1";

/// A synthetic administrative template the test map declares `none`.
const ADMIN: &str = "Example Admin Note.v1";

/// A synthetic template the test map does not hold.
const UNMAPPED: &str = "Example Unmapped.v1";

/// The archetype of every synthetic composition.
const REPORT: &str = "openEHR-EHR-COMPOSITION.report.v1";

/// The archetype the test map gives medical test results.
const LAB_ARCHETYPE: &str = "openEHR-EHR-OBSERVATION.laboratory_test_result.v1";

/// The `[access_log]` tables of the test map.
fn map_toml() -> String {
    format!(
        "\n[access_log.templates]\n\"{LAB_REPORT}\" = [\"medical-test-result\"]\n\
         \"{DISCHARGE}\" = [\"discharge-report\"]\n\"{ADMIN}\" = \"none\"\n\n\
         [access_log.archetypes]\n\"{LAB_ARCHETYPE}\" = [\"medical-test-result\"]\n"
    )
}

/// A development gateway over node A at `a` and node B at `b`, the patient
/// known at both, recording to `repository` with the `[audit.repository]`
/// keys `extra`, classifying with the test map, holding stored queries in
/// `dir`.
fn gateway(
    dir: &Path,
    nodes: (&str, &str),
    repository: &FeedRepository,
    extra: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway_under(dir, nodes, repository, extra, &settings_with_room())
}

/// The gateway of [`gateway`], served under `server`.
fn gateway_under(
    dir: &Path,
    nodes: (&str, &str),
    repository: &FeedRepository,
    extra: &str,
    server: &ServerSettings,
) -> Result<Router, Box<dyn Error>> {
    gateway_with(dir, nodes, repository, (extra, ""), server)
}

/// The gateway of [`gateway_under`], the `[federation]` keys `federation`
/// added.
fn gateway_with(
    dir: &Path,
    nodes: (&str, &str),
    repository: &FeedRepository,
    keys: (&str, &str),
    server: &ServerSettings,
) -> Result<Router, Box<dyn Error>> {
    let (router, _) = gateway_and_state(dir, nodes, repository, keys, server)?;
    Ok(router)
}

/// The gateway of [`gateway_with`], with the state it serves from: the
/// metrics a request records go to that state's registry.
fn gateway_and_state(
    dir: &Path,
    (a, b): (&str, &str),
    repository: &FeedRepository,
    (extra, federation): (&str, &str),
    server: &ServerSettings,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let store = toml::Value::String(dir.join("definitions.redb").display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 4000\noverall_timeout_ms = 6000\n{federation}\n\
         [stored_queries]\npath = {store}\n{}{}{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        audit_tables(repository, extra),
        map_toml()
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok((ferrofed_server::router(Arc::clone(&state), server), state))
}

/// Fails unless `exposition`, rendered from the registry a federated query
/// recorded into, holds the node request series of the endpoint `endpoint`,
/// so a check that no value reaches a metric reads what the request wrote.
fn recorded_the_query(exposition: &str, endpoint: &str) -> Result<(), Box<dyn Error>> {
    let series = exposition
        .lines()
        .filter(|line| line.starts_with("ferrofed_node_requests"))
        .any(|line| line.contains(&format!("endpoint=\"{endpoint}\"")));
    if series {
        Ok(())
    } else {
        Err(format!("no node request series of {endpoint} in {exposition}").into())
    }
}

/// A canonical `COMPOSITION` of `template`, the version `uid`.
fn composition(template: &str, uid: &str) -> String {
    format!(
        r#"{{"_type":"COMPOSITION","name":{{"_type":"DV_TEXT","value":"Synthetic report"}},"uid":{{"_type":"OBJECT_VERSION_ID","value":"{uid}"}},"archetype_details":{{"_type":"ARCHETYPED","archetype_id":{{"_type":"ARCHETYPE_ID","value":"{REPORT}"}},"template_id":{{"_type":"TEMPLATE_ID","value":"{template}"}},"rm_version":"1.1.0"}},"archetype_node_id":"{REPORT}","language":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"ISO_639-1"}},"code_string":"en"}},"territory":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"ISO_3166-1"}},"code_string":"NL"}},"category":{{"_type":"DV_CODED_TEXT","value":"event","defining_code":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"openehr"}},"code_string":"433"}}}},"composer":{{"_type":"PARTY_SELF"}}}}"#
    )
}

/// A node answering the federated query with one row per cell of `cells`.
async fn node_with_rows(cells: &[String]) -> Server {
    let server = Server::start().await;
    let rows: Vec<String> = cells.iter().map(|cell| format!("[{cell}]")).collect();
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The access records among `records`, each read as JSON.
fn accesses(records: &[String]) -> Result<Vec<Value>, Box<dyn Error>> {
    records
        .iter()
        .filter(|record| record.contains("ehds-categories"))
        .map(|record| Ok(serde_json::from_str(record)?))
        .collect()
}

/// The entities of `record` named `name`.
fn named<'a>(record: &'a Value, name: &str) -> Vec<&'a Value> {
    record["entity"]
        .as_array()
        .map(|entities| {
            entities
                .iter()
                .filter(|entity| entity["name"] == name)
                .collect()
        })
        .unwrap_or_default()
}

/// The `detail` values of `kind` in the entity named `name` of `record`.
fn details(record: &Value, name: &str, kind: &str) -> Vec<String> {
    named(record, name)
        .iter()
        .flat_map(|entity| {
            entity["detail"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|detail| detail["type"] == kind)
                .filter_map(|detail| detail["valueString"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The `meta.profile` of `record`.
fn profile(record: &Value) -> Option<&str> {
    record.pointer("/meta/profile/0").and_then(Value::as_str)
}
