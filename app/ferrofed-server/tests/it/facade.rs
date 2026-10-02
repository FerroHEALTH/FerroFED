// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST {base}/v1/query/aql` against two mock nodes, configured through the
//! real configuration path: one federated `RESULT_SET` with the rows of every
//! member that knows the patient, the subject column re-injected, and no node
//! request carrying the patient identifier (§5.4, §7, §9, §11.1; N1, N2, N5,
//! N7, N16, N17, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use http::{Request, StatusCode, header};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{call, settings};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient identifier: visibly synthetic, under no real scheme.
pub(crate) const PATIENT: &str = "SENTINEL-PATIENT-38a1";

/// The synthetic issuing namespace, under the example OID arc.
pub(crate) const NAMESPACE: &str = "urn:oid:2.999.1";

/// The patient's `ehr_id` at node A and at node B.
pub(crate) const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
pub(crate) const EHR_B: &str = "1111bbbb-1111-4111-8111-111111111111";

/// The façade query of §7.2: the patient identified the openEHR way, the
/// identifier selected back, and one composition column.
pub(crate) fn patient_query() -> String {
    format!(
        "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value \
         FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The ITS-REST `AdhocQueryExecute` body carrying `aql`.
pub(crate) fn body(aql: &str) -> Result<String, serde_json::Error> {
    #[derive(serde::Serialize)]
    struct Adhoc<'a> {
        q: &'a str,
    }
    serde_json::to_string(&Adhoc { q: aql })
}

/// A node answering `POST /v1/query/aql` with one row holding `uid`.
pub(crate) async fn node_answering(uid: &str) -> MockServer {
    let server = MockServer::start().await;
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["{uid}"]]}}"##
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

/// A node answering `POST /v1/query/aql` with `status` and an ITS-REST error.
pub(crate) async fn node_failing(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(status).set_body_raw(
            br#"{"message":"synthetic node failure"}"#.to_vec(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

/// The registry document of node A and node B at `a` and `b`, with
/// `extra` appended.
pub(crate) fn registry(a: &str, b: &str, extra: &str) -> String {
    format!(
        r#"
[[organisation]]
id = "org-a"

[[organisation]]
id = "org-b"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[node]]
id = "node-b"
organisation = "org-b"
system_id = "cdr-b.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "{a}"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "{b}"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
{extra}"#
    )
}

/// The `[dev]` rows mapping the patient to `rows`, each `(member, ehr_id)`.
pub(crate) fn crossref(rows: &[(&str, &str)]) -> String {
    rows.iter().fold(String::new(), |mut text, (member, ehr_id)| {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        );
        text
    })
}

/// The gateway configured by the top-level keys `top` and the tables
/// `tables`, with the registry document `registry` written into `dir`.
pub(crate) fn gateway(
    dir: &Path,
    registry: &str,
    top: &str,
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "{top}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\n\n{tables}"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    ))
}

/// The middleware settings, with a request timeout past the fan-out budget.
pub(crate) fn settings_with_room() -> ferrofed_server::config::settings::ServerSettings {
    let mut server = settings();
    server.request_timeout = std::time::Duration::from_secs(10);
    server.body_limit = 64 * 1024;
    server
}

/// A development gateway over node A and node B at `a` and `b`, resolving the
/// patient at the members `rows` name.
pub(crate) fn dev_gateway(
    dir: &Path,
    a: &str,
    b: &str,
    rows: &[(&str, &str)],
) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &crossref(rows),
    )
}

/// `POST /v1/query/aql` with `body`.
pub(crate) fn post(body: String) -> Result<Request<Body>, http::Error> {
    Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
}

/// The bodies of every request `server` received.
pub(crate) async fn received(server: &MockServer) -> Result<Vec<String>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut bodies = Vec::new();
    for request in requests {
        bodies.push(String::from_utf8(request.body.clone())?);
    }
    Ok(bodies)
}

/// Every byte `server` received, request line and headers included.
pub(crate) async fn wire(server: &MockServer) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut text = String::new();
    for request in requests {
        text.push_str(request.url.as_str());
        for (name, value) in &request.headers {
            text.push_str(name.as_str());
            text.push_str(value.to_str().unwrap_or_default());
        }
        text.push_str(&String::from_utf8_lossy(&request.body));
    }
    Ok(text)
}

/// The federated answer, read for the members the tests assert on.
#[derive(Debug, Deserialize)]
pub(crate) struct Answer {
    q: String,
    columns: Vec<Column>,
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) meta: Meta,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Column {
    name: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Meta {
    pub(crate) federation: FederationMeta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FederationMeta {
    pub(crate) complete: bool,
    pub(crate) endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Endpoint {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) row_count: Option<u64>,
}

/// The ITS-REST error body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItsError {
    message: String,
    #[serde(rename = "validationErrors")]
    validation_errors: Vec<String>,
}

/// Each endpoint's status, in the envelope's order.
pub(crate) fn statuses(answer: &Answer) -> Vec<(&str, &str)> {
    answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|endpoint| (endpoint.id.as_str(), endpoint.status.as_str()))
        .collect()
}

// conformance: CP-1 CP-2 CP-4 CP-7 CP-35
#[tokio::test]
async fn a_patient_query_is_one_result_set_over_both_nodes() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(patient_query(), answer.q, "q is the client's query (N17)");
    assert_eq!(
        vec![
            Column {
                name: "patient".to_owned(),
                path: Some("/ehr_status/subject/external_ref/id/value".to_owned()),
            },
            Column {
                name: "#1".to_owned(),
                path: Some("/uid/value".to_owned()),
            },
        ],
        answer.columns,
        "columns[] renders the client's query the ITS-REST way, with no endpoint column (N17, CP-35)"
    );
    assert_eq!(
        vec![
            vec![
                PATIENT.to_owned(),
                "uid-at-a::cdr-a.example.org::1".to_owned()
            ],
            vec![
                PATIENT.to_owned(),
                "uid-at-b::cdr-b.example.org::1".to_owned()
            ],
        ],
        answer.rows,
        "one positional row per node, the subject re-injected (N5, CP-7)"
    );
    assert!(answer.meta.federation.complete, "both members answered");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "meta.federation names both endpoints (N16)"
    );
    assert!(
        answer
            .meta
            .federation
            .endpoints
            .iter()
            .all(|endpoint| endpoint.row_count == Some(1)),
        "each endpoint reports its row count"
    );

    for (server, own, other) in [(&a, EHR_A, EHR_B), (&b, EHR_B, EHR_A)] {
        let bodies = received(server).await?;
        assert_eq!(1, bodies.len(), "each node is asked once: {bodies:?}");
        let sent = bodies.first().ok_or("one request")?;
        assert!(
            sent.contains(&format!("e/ehr_id/value='{own}'")),
            "the node query is keyed on the node's own ehr_id (N7, CP-4): {sent}"
        );
        assert!(
            !sent.contains(other),
            "a node never learns another node's ehr_id: {sent}"
        );
        assert!(
            !sent.contains("ehr_status/subject"),
            "no subject path reaches a node (N2, CP-2): {sent}"
        );
        let all = wire(server).await?;
        assert!(
            !all.contains(PATIENT),
            "the patient identifier reaches no request line, header or body (N33): {all}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_know_the_patient_is_not_resolved_and_not_asked() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "not-resolved fails nothing (N6): {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-resolved")],
        statuses(&answer),
        "the member that does not know the patient is reported (N16)"
    );
    assert!(
        !answer.meta.federation.complete,
        "a not-resolved member clears complete (§11.4)"
    );
    assert_eq!(1, answer.rows.len(), "only node A contributes");
    assert!(
        received(&b).await?.is_empty(),
        "a member that does not know the patient is never asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_query_that_names_no_patient_is_asked_of_every_member() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec!["uid-at-a".to_owned()], vec!["uid-at-b".to_owned()]],
        answer.rows,
        "every member is asked where no localizer is configured (N4)"
    );
    for server in [&a, &b] {
        let bodies = received(server).await?;
        assert_eq!(
            1,
            bodies.len(),
            "each member is asked once, as written: {bodies:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn without_a_resolver_a_patient_query_fails_closed() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "no cross-reference answers, so the query fails (decision A17): {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.rows.is_empty(), "a failing query returns no rows");
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        statuses(&answer),
        "every member is reported with the reason"
    );
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "no node is asked a query the gateway cannot scope"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_failing_node_fails_the_query_and_the_envelope_still_comes_back() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "a node-error fails the query (N37): {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer.rows.is_empty(),
        "a failing query returns none of the rows it did obtain (§11.4)"
    );
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "node-error")],
        statuses(&answer),
        "the failing answer carries the envelope"
    );
    Ok(())
}

#[tokio::test]
async fn a_suspended_endpoint_is_excluded_and_never_asked() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let spare = node_answering("uid-at-a-again").await;
    let suspended = node_answering("uid-suspended").await;
    let dir = tempfile::tempdir()?;
    let extra = format!(
        r#"
[[endpoint]]
id = "node-a-spare"
node = "node-a"
url = "{}"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-old"
node = "node-b"
url = "{}"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
status = "suspended"
"#,
        spare.uri(),
        suspended.uri()
    );
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), &extra),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-a-spare", "excluded"),
            ("node-b-old", "excluded"),
            ("node-b-pub", "active"),
        ],
        statuses(&answer),
        "each member is asked once, and every endpoint is reported (§11.1)"
    );
    assert!(
        answer.meta.federation.complete,
        "an excluded endpoint was never in scope (§11.4)"
    );
    assert_eq!(2, answer.rows.len(), "one row per member, none twice");
    for server in [&spare, &suspended] {
        assert!(
            received(server).await?.is_empty(),
            "an excluded endpoint is never contacted"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_bound_parameter_resolves_like_a_literal_and_never_reaches_a_node() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let request = format!(
        r#"{{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'","query_parameters":{{"patient":"{PATIENT}"}}}}"#
    );

    let (status, text) = call(app, post(request)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(2, answer.rows.len(), "both members answer");
    for server in [&a, &b] {
        let all = wire(server).await?;
        assert!(
            !all.contains(PATIENT),
            "a bound identifier reaches no node either (N33): {all}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_query_is_a_400_that_quotes_nothing_and_asks_nobody() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let refused = [
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
             OR c/name/value = 'x'"
        ),
        format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
             AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}' \
             AND c/name/value = CONCAT('SENTINEL-PATIENT', '-38a1')"
        ),
        format!("SELECT {PATIENT} FROM"),
    ];
    for aql in refused {
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let (status, text) = call(app, post(body(&aql)?)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{aql}: {text}");
        let error: ItsError = serde_json::from_str(&text)?;
        assert!(
            !error.message.contains(PATIENT) && !error.message.contains("38a1"),
            "the refusal quotes nothing (§5.4.3): {}",
            error.message
        );
        assert!(error.validation_errors.is_empty(), "no other detail");
    }
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "a refused query reaches no node"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_body_that_is_not_an_adhoc_query_is_a_400_with_a_fixed_message() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    for request in [
        format!(r#"{{"q":5,"note":"{PATIENT}"}}"#),
        String::from("not json"),
        format!(r#"{{"q":"SELECT 1 FROM EHR e","query_parameters":{{"p":["{PATIENT}"]}}}}"#),
    ] {
        let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
        let (status, text) = call(app, post(request.clone())?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{request}: {text}");
        assert!(
            !text.contains(PATIENT),
            "the refusal never quotes the body: {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn without_a_registry_the_query_route_is_unserved() -> TestResult {
    let (status, _) = call(crate::support::app(), post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::NOT_IMPLEMENTED,
        status,
        "a gateway with no registry federates nothing"
    );
    Ok(())
}

/// Schema validation of the answer bodies against the vendored specification.
pub(crate) mod schema {
    #![expect(
        clippy::disallowed_types,
        reason = "seam 4 of rust-style.md: schema validation reads JSON as values, in tests only"
    )]

    use std::error::Error;

    use serde_json::Value;

    /// The vendored result-envelope schema.
    const RESULT_SET_SCHEMA: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/federation-spec/modules/ROOT/attachments/federated-result-set.schema.json"
    );

    /// Validates the JSON `text` against the result-set schema, formats
    /// included.
    pub(crate) fn validate(text: &str) -> Result<(), Box<dyn Error>> {
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(RESULT_SET_SCHEMA)?)?;
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)?;
        let instance: Value = serde_json::from_str(text)?;
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("federated-result-set.schema.json: {}", errors.join("; ")).into())
        }
    }
}
