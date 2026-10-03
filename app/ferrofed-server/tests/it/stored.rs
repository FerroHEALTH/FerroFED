// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated stored-query registry against two mock nodes (§12.7, N44,
//! N33; CP-40, CP-28): definitions held at the gateway on ITS-REST's semver
//! segment, a held version immutable across a restart, a literal patient
//! refused at storage, and a definition invoked by name as an ordinary
//! fan-out whose answer names the gateway's definition.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::state::AppState;
use http::{Request, StatusCode, header};
use openehr_federation::options::{DefinitionBehaviour, OptionsRoot};
use openehr_its::rest::generated::definition::StoredQuery;
use serde::Deserialize;
use wiremock::MockServer;

use crate::directive::{EHR_C, Nodes};
use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, crossref, node_answering, received,
    registry, schema, settings_with_room, statuses, wire,
};
use crate::support::{call, error_body, send};

type TestResult = Result<(), Box<dyn Error>>;

/// The qualified name every fixture stores under.
const NAME: &str = "org.example::patient_compositions";

/// A definition naming the patient through `$patient`, in the fixture
/// namespace, with one more parameter, `$name`.
fn parameterised() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}' \
         AND c/name/value = $name"
    )
}

/// The `query_parameters` binding the patient and the composition name.
fn bound() -> String {
    format!(r#"{{"query_parameters":{{"patient":"{PATIENT}","name":"Visit"}}}}"#)
}

/// The settings text of a gateway over `document`, holding its definitions
/// in `store`, resolving the patient at the members `rows` name.
fn settings_text(document: &Path, store: &Path, rows: &[(&str, &str)]) -> String {
    let document = toml::Value::String(document.display().to_string());
    let store = toml::Value::String(store.display().to_string());
    format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\n\
         [stored_queries]\npath = {store}\n{}",
        crossref(rows)
    )
}

/// The store file of a gateway whose state lives in `dir`.
fn store_file(dir: &Path) -> PathBuf {
    dir.join("definitions.redb")
}

/// A gateway offering the registry, its registry document `registry` and its
/// store in `dir`, resolving the patient at the members `rows` name.
pub(crate) fn gateway(
    dir: &Path,
    registry: &str,
    rows: &[(&str, &str)],
) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let text = settings_text(&document, &store_file(dir), rows);
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&settings)?;
    Ok(ferrofed_server::router(
        Arc::new(state),
        &settings_with_room(),
    ))
}

/// A registry gateway over node A and node B, the patient known at both.
fn two_members(dir: &Path, a: &MockServer, b: &MockServer) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )
}

/// `PUT {base}/v1/definition/query/{name}/{version}` with the AQL `aql`.
fn put(name: &str, version: &str, aql: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/definition/query/{name}/{version}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql.to_owned()))
}

/// `GET {base}/v1/definition/query/{path}`.
fn get(path: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/definition/query/{path}")).body(Body::empty())
}

/// `POST {base}/v1/query/{path}` with the `Query` body `body` and the header
/// lines `fields`.
fn invoke(path: &str, body: &str, fields: &[(&str, &str)]) -> Result<Request<Body>, http::Error> {
    let mut request =
        Request::post(format!("/v1/query/{path}")).header(header::CONTENT_TYPE, "application/json");
    for (name, value) in fields {
        request = request.header(*name, *value);
    }
    request.body(Body::from(body.to_owned()))
}

/// The ITS-REST `name` member of a result set.
#[derive(Debug, Deserialize)]
struct Named {
    name: Option<String>,
}

/// Stores `aql` at `version` of [`NAME`] and checks it was stored.
async fn stored(app: &Router, version: &str, aql: &str) -> TestResult {
    let (status, text) = call(app.clone(), put(NAME, version, aql)?).await?;
    assert_eq!(StatusCode::OK, status, "§12.7: stored: {text}");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_stored_query_invoked_by_name_fans_out_and_names_the_gateways_definition() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;

    let (status, text) = call(app, invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let named: Named = serde_json::from_str(&text)?;
    assert_eq!(
        Some(NAME),
        named.name.as_deref(),
        "§12.7, N44: the name of the gateway's definition"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "CP-40: meta.federation.endpoints[] covers both members"
    );
    let mut rows = answer.rows.clone();
    rows.sort();
    assert_eq!(
        vec![
            vec!["uid-at-a::cdr-a.example.org::1".to_owned()],
            vec!["uid-at-b::cdr-b.example.org::1".to_owned()],
        ],
        rows,
        "CP-40: rows from more than one member"
    );
    for node in [&a, &b] {
        let captured = wire(node).await?;
        assert!(!captured.is_empty(), "each member was asked");
        assert!(
            !captured.contains(PATIENT_TAIL),
            "N33: no node receives the identifier: {captured}"
        );
    }
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn query_parameters_bind_into_the_stored_query_as_if_submitted_inline() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    let inline = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let other = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let elsewhere = tempfile::tempdir()?;
    let adhoc = two_members(elsewhere.path(), &inline, &other)?;
    let body = format!(
        r#"{{"q":{},"query_parameters":{{"patient":"{PATIENT}","name":"Visit"}}}}"#,
        serde_json::to_string(&parameterised())?
    );
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))?;
    let (status, text) = call(adhoc, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        received(&inline).await?,
        received(&a).await?,
        "§12.7: exactly as if the client had submitted the text inline"
    );
    let dispatched = received(&a).await?.concat();
    assert!(dispatched.contains("'Visit'"), "{dispatched}");
    assert!(
        dispatched.contains(EHR_A),
        "§7.1: scoped to the node's ehr_id"
    );

    let (status, text) = call(app, invoke(NAME, "{}", &[])?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "parameters",
        error_body(&text)?.code,
        "an unbound parameter is refused by name"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_second_put_of_a_held_name_and_version_is_refused_and_the_text_stands() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let first = parameterised().replace("$name", "'first'");
    stored(&app, "1.0.0", &first).await?;

    let second = parameterised().replace("$name", "'second'");
    let (status, text) = call(app.clone(), put(NAME, "1.0.0", &second)?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "§12.7, N44: {text}");
    assert_eq!("stored-query-held", error_body(&text)?.code);

    let (status, text) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert!(
        held.q.contains("'first'"),
        "the held text stands: {}",
        held.q
    );
    assert!(!held.q.contains("'second'"), "never applied: {}", held.q);
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_definition_naming_the_patient_by_a_literal_is_refused_at_put() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let literal = parameterised().replace("$patient", &format!("'{PATIENT}'"));

    let (status, text) = call(app.clone(), put(NAME, "1.0.0", &literal)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "N33: {text}");
    assert_eq!("subject-literal", error_body(&text)?.code);
    assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never quoted: {text}");

    let (status, _) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    let bytes = std::fs::read(store_file(dir.path()))?;
    assert!(
        !bytes
            .windows(PATIENT_TAIL.len())
            .any(|window| window == PATIENT_TAIL.as_bytes()),
        "N33: the identifier never reaches the store"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn the_store_holds_no_identifier_after_a_definition_is_stored_and_invoked() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    drop(app);

    let bytes = std::fs::read(store_file(dir.path()))?;
    let holds = |needle: &str| {
        bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    assert!(holds(NAME), "the store file holds the definition");
    assert!(holds("$patient"), "the patient is held as its parameter");
    assert!(!holds(PATIENT_TAIL), "N33: no identifier at rest");
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_stored_version_survives_a_restart_and_stays_immutable() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    {
        let app = two_members(dir.path(), &a, &b)?;
        stored(&app, "1.0.0", &parameterised()).await?;
    }
    let restarted = two_members(dir.path(), &a, &b)?;
    let (status, text) = call(restarted.clone(), get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the version outlives the process: {text}"
    );
    let (status, text) = call(restarted.clone(), invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "invocable after the restart: {text}"
    );
    let (status, text) = call(restarted, put(NAME, "1.0.0", &parameterised())?).await?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "§12.7, N44: the refusal holds after a restart: {text}"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn no_version_runs_the_latest_and_a_prefix_runs_the_highest_it_matches() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    for (version, marker) in [
        ("1.2.0", "one-two"),
        ("1.10.0", "one-ten"),
        ("2.0.0", "two"),
    ] {
        stored(
            &app,
            version,
            &parameterised().replace("$name", &format!("'{marker}'")),
        )
        .await?;
    }
    let body = format!(r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#);
    for (path, marker) in [
        (NAME.to_owned(), "two"),
        (format!("{NAME}/1"), "one-ten"),
        (format!("{NAME}/1.2"), "one-two"),
        (format!("{NAME}/1.10.0"), "one-ten"),
    ] {
        let (status, text) = call(app.clone(), invoke(&path, &body, &[])?).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {text}");
        let answer: Answer = serde_json::from_str(&text)?;
        assert!(
            answer.q.contains(marker),
            "ITS-REST: {path} runs the version with {marker}: {}",
            answer.q
        );
        let named: Named = serde_json::from_str(&text)?;
        assert_eq!(Some(NAME), named.name.as_deref(), "{path}");
    }
    let (status, text) = call(app, invoke(&format!("{NAME}/3"), &body, &[])?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("stored-query-unknown", error_body(&text)?.code);
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn a_stored_query_is_targeted_by_the_header_as_by_the_directive() -> TestResult {
    let everywhere = [("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)];
    let plain = parameterised();
    let directed = plain.replace(
        "FROM EHR e",
        r#"FROM ENDPOINT p ["node-a-pub", "node-b-pub"] CONTAINS EHR e"#,
    );
    let body = bound();

    let by_header = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = by_header.gateway_with_store(dir.path(), &everywhere)?;
    stored(&app, "1.0.0", &plain).await?;
    let header = [("openEHR-federation-endpoint", "node-a-pub, node-b-pub")];
    let (status, text) = call(app, invoke(NAME, &body, &header)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let header: Answer = serde_json::from_str(&text)?;

    let by_directive = Nodes::start().await;
    let other = tempfile::tempdir()?;
    let app = by_directive.gateway_with_store(other.path(), &everywhere)?;
    stored(&app, "1.0.0", &directed).await?;
    let (status, text) = call(app.clone(), invoke(NAME, &body, &[])?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let directive: Answer = serde_json::from_str(&text)?;

    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&header),
        "§8.4: the header selects the node set of a stored query"
    );
    assert_eq!(
        statuses(&directive),
        statuses(&header),
        "CP-28: the same stored query, by header and by directive"
    );
    assert_eq!(directive.rows.len(), header.rows.len());
    assert_eq!([1, 1, 0], by_header.asked().await?);
    assert_eq!([1, 1, 0], by_directive.asked().await?);

    let conflict = [("openEHR-federation-endpoint", "node-c-pub")];
    let (status, text) = call(app, invoke(NAME, &body, &conflict)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1, N35: {text}");
    assert_eq!("targeting-conflict", error_body(&text)?.code);
    assert_eq!(
        [1, 1, 0],
        by_directive.asked().await?,
        "nothing more is dispatched"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_stored_definition_reads_back_as_an_its_rest_stored_query_and_lists() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let response = send(app.clone(), put(NAME, "1.0.0", &parameterised())?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("1.0.0"),
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        "ITS-REST: Location names the stored query, relative to the request"
    );
    stored(&app, "1.1.0", &parameterised()).await?;

    let (status, text) = call(app.clone(), get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(NAME, held.name);
    assert_eq!("AQL", held.r#type);
    assert_eq!("1.0.0", held.version);
    assert!(
        held.saved.parse::<jiff::Timestamp>().is_ok(),
        "{}",
        held.saved
    );
    assert!(held.q.contains("$patient"), "{}", held.q);

    let (status, text) = call(app.clone(), get(&format!("{NAME}/1"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let latest: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!("1.1.0", latest.version, "ITS-REST: the highest 1.x");

    let (status, text) = call(app, get("org.example")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    let versions: Vec<&str> = listed.iter().map(|query| query.version.as_str()).collect();
    assert_eq!(
        vec!["1.0.0", "1.1.0"],
        versions,
        "ITS-REST definition_query_list"
    );
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_definition_the_rewrite_refuses_whatever_is_bound_is_refused_at_put() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    for (aql, code) in [
        ("SELECT FROM WHERE", "not-aql"),
        (
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_status/subject/external_ref/id/value = $patient OR c/name/value = 'x'",
            "unreducible",
        ),
    ] {
        let (status, text) = call(app.clone(), put(NAME, "1.0.0", aql)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{aql}: {text}");
        assert_eq!(code, error_body(&text)?.code, "{aql}");
    }
    let (status, _) = call(app, get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "nothing was stored");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_name_a_version_or_a_query_type_outside_its_rest_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let aql = parameterised();
    let unversioned = Request::put(format!("/v1/definition/query/{NAME}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql.clone()))?;
    let typed = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?query_type=SQL"))
        .body(Body::from(aql.clone()))?;
    let undeclared = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?endpoint=x"))
        .body(Body::from(aql.clone()))?;
    for (request, status, code) in [
        (
            put("ns::aql", "1.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-name-invalid",
        ),
        (
            put("a%20b", "1.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-name-invalid",
        ),
        (
            put(NAME, "1.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            put(NAME, "01.0.0", &aql)?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            unversioned,
            StatusCode::BAD_REQUEST,
            "query-version-required",
        ),
        (typed, StatusCode::BAD_REQUEST, "query-type-unsupported"),
        (
            undeclared,
            StatusCode::BAD_REQUEST,
            "query-parameter-refused",
        ),
        (
            get(&format!("{NAME}/x"))?,
            StatusCode::BAD_REQUEST,
            "query-version-invalid",
        ),
        (
            invoke("unknown", "{}", &[])?,
            StatusCode::NOT_FOUND,
            "stored-query-unknown",
        ),
    ] {
        let (answered, text) = call(app.clone(), request).await?;
        assert_eq!(status, answered, "{text}");
        assert_eq!(code, error_body(&text)?.code, "{text}");
    }
    let typed = Request::put(format!("/v1/definition/query/{NAME}/1.0.0?query_type=aql"))
        .body(Body::from(aql))?;
    let (status, text) = call(app, typed).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the ITS-REST default, in any case: {text}"
    );
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-40
#[tokio::test]
async fn a_failing_member_fails_the_stored_query_and_the_envelope_still_names_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = crate::facade::node_failing(500).await;
    let app = two_members(dir.path(), &a, &b)?;
    stored(&app, "1.0.0", &parameterised()).await?;
    let (status, text) = call(app, invoke(NAME, &bound(), &[])?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.4: {text}");
    schema::validate(&text)?;
    let named: Named = serde_json::from_str(&text)?;
    assert_eq!(Some(NAME), named.name.as_deref(), "§12.7: the name stands");
    Ok(())
}

// conformance: CP-23 CP-40
#[tokio::test]
async fn options_declares_the_registry_and_the_methods_it_serves() -> TestResult {
    let dir = tempfile::tempdir()?;
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let app = two_members(dir.path(), &a, &b)?;
    let (status, text) = call(app.clone(), Request::options("/").body(Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    let body: OptionsRoot = serde_json::from_str(&text)?;
    assert_eq!(
        DefinitionBehaviour::new(false)
            .with_stored_query_registry(true)?
            .with_stored_query_fan_out(false)?,
        body.federation.definition,
        "§7a.2, N44: the registry is declared; no definition fan-out"
    );
    for (uri, expected) in [
        (format!("/v1/query/{NAME}"), "POST, OPTIONS"),
        (format!("/v1/query/{NAME}/1.0.0"), "POST, OPTIONS"),
        (format!("/v1/definition/query/{NAME}"), "GET, PUT, OPTIONS"),
        (
            format!("/v1/definition/query/{NAME}/1.0.0"),
            "GET, PUT, OPTIONS",
        ),
    ] {
        let response = send(app.clone(), Request::options(&uri).body(Body::empty())?).await?;
        assert_eq!(StatusCode::NO_CONTENT, response.status(), "{uri}");
        let allow = response
            .headers()
            .get(header::ALLOW)
            .ok_or("an Allow field")?
            .to_str()?;
        assert_eq!(expected, allow, "§7a.2: {uri}");
    }
    Ok(())
}

#[tokio::test]
async fn without_the_registry_the_definition_area_stays_unserved() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = crate::facade::dev_gateway(dir.path(), &a.uri(), &b.uri(), &[])?;
    for request in [
        put(NAME, "1.0.0", &parameterised())?,
        invoke(NAME, &bound(), &[])?,
    ] {
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "§12.6: {text}");
    }
    Ok(())
}

#[test]
fn a_store_without_a_registry_document_refuses_to_resolve() -> TestResult {
    let text = "[stored_queries]\npath = \"/var/lib/ferrofed/definitions.redb\"\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Missing { key } if key == "registry.document"),
        "{refused:?}"
    );
    Ok(())
}
