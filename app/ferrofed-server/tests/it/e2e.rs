// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The first federated query end to end, behind the `FERROFED_E2E` gate: one
//! `RESULT_SET` over FerroEHR and EHRbase, each behind its capturing proxy,
//! and no node request carrying the patient identifier (§7, §9, §11; N1, N5,
//! N7, N16, N17, N33).
//!
//! The gateway resolves the patient through the development cross-reference,
//! which the gateway alone holds: the nodes are seeded with no subject at all,
//! so a row can only reach the answer through the `ehr_id` the gateway sent.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::containers::{self, ProxiedNode};
use ferrofed_testkit::seed::{self, CompositionSeed, DemoComposition, EhrSeed, SeedPlan};
use http::{Request, StatusCode, header};
use serde::Deserialize;
use uuid::Uuid;

use crate::support::{call, settings};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient the gateway's cross-reference knows.
const PATIENT: &str = "SENTINEL-PATIENT-e2e-38";

/// The synthetic issuing namespace, under the example OID arc.
const NAMESPACE: &str = "urn:oid:2.999.1";

/// The patient's `ehr_id` on node A (FerroEHR) and on node B (EHRbase).
const EHR_A: Uuid = Uuid::from_u128(0x3333_3333_3333_4333_8333_3333_3333_3333);
const EHR_B: Uuid = Uuid::from_u128(0x4444_4444_4444_4444_8444_4444_4444_4444);

/// A seed of one EHR with no subject and one composition in it.
fn plan(ehr_id: Uuid, composition: DemoComposition) -> SeedPlan {
    SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id,
            subject: None,
        }],
        template: true,
        compositions: vec![CompositionSeed {
            ehr_id,
            composition,
        }],
    }
}

/// The gateway over node A and node B, resolving the patient at both through
/// the development cross-reference.
fn gateway(
    dir: &std::path::Path,
    a: &ProxiedNode,
    b: &ProxiedNode,
) -> Result<axum::Router, Box<dyn Error>> {
    let resolver = format!(
        r#"profile = "development"

[[dev.crossref]]
namespace = "{NAMESPACE}"
value = "{PATIENT}"
member = "node-a"
ehr_id = "{EHR_A}"

[[dev.crossref]]
namespace = "{NAMESPACE}"
value = "{PATIENT}"
member = "node-b"
ehr_id = "{EHR_B}"
"#
    );
    gateway_resolving(dir, a, b, &resolver)
}

/// The gateway over node A and node B, with the resolver `resolver`
/// configures.
fn gateway_resolving(
    dir: &std::path::Path,
    a: &ProxiedNode,
    b: &ProxiedNode,
    resolver: &str,
) -> Result<axum::Router, Box<dyn Error>> {
    let registry = format!(
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
url = "{}"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "{}"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
"#,
        a.api_root(),
        b.api_root()
    );
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let document = toml::Value::String(document.display().to_string());
    let config = format!(
        "{resolver}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\n"
    );
    let settings_ = Config::from_sources(Some(&config), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings_)?.ok_or("a registry is configured")?;
    let mut server = settings();
    server.request_timeout = Duration::from_secs(30);
    server.body_limit = 64 * 1024;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &server,
    ))
}

/// `POST /v1/query/aql` with `aql`.
fn query(aql: &str) -> Result<Request<Body>, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Adhoc<'a> {
        q: &'a str,
    }
    Ok(Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&Adhoc { q: aql })?))?)
}

/// The members of the answer the test reads.
#[derive(Debug, Deserialize)]
struct Answer {
    rows: Vec<Vec<String>>,
    meta: Meta,
}

#[derive(Debug, Deserialize)]
struct Meta {
    federation: FederationMeta,
}

#[derive(Debug, Deserialize)]
struct FederationMeta {
    complete: bool,
    endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize)]
struct Endpoint {
    id: String,
    status: String,
    row_count: Option<u64>,
}

// conformance: CP-1 CP-2 CP-4 CP-35
#[tokio::test]
async fn one_result_set_over_two_cdr_products_and_no_identifier_on_the_wire() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;

    let patient = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, text) = call(gateway(dir.path(), &nodes.a, &nodes.b)?, query(&patient)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.meta.federation.complete, "both products answered");
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "active", Some(1)),
        ],
        reported,
        "one composition from each product (N16)"
    );
    assert_eq!(2, answer.rows.len(), "rows from both nodes: {text}");

    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let journal = node.proxy.journal();
        let api = node.node.product().api_path();
        let steps: Vec<(&str, &str)> = journal
            .iter()
            .map(|capture| (capture.method.as_str(), capture.path.as_str()))
            .collect();
        assert_eq!(
            vec![("POST", format!("{api}/v1/query/aql").as_str())],
            steps,
            "{:?} received one ITS-REST query and nothing else",
            node.node.product()
        );
        let sent = journal.first().ok_or("one capture")?;
        let body = String::from_utf8_lossy(&sent.body);
        assert!(
            body.contains(&own.to_string()),
            "the node query is keyed on the node's own ehr_id (N7): {body}"
        );
        assert!(
            !body.contains(&other.to_string()),
            "a node never learns another node's ehr_id: {body}"
        );
        assert!(
            !node.proxy.journal_contains(PATIENT.as_bytes()),
            "{:?} saw the patient identifier in some carrier (N33)",
            node.node.product()
        );
    }

    // The README quickstart query: no patient, every member asked (N4).
    let (status, text) = call(
        gateway(dir.path(), &nodes.a, &nodes.b)?,
        query("SELECT e/ehr_id/value FROM EHR e")?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    let mut ehr_ids: Vec<String> = answer.rows.into_iter().flatten().collect();
    ehr_ids.sort();
    let mut expected = vec![EHR_A.to_string(), EHR_B.to_string()];
    expected.sort();
    assert_eq!(
        expected, ehr_ids,
        "the quickstart query returns the EHR of each member"
    );
    Ok(())
}

/// The rows of `aql` through the gateway, sorted, after asserting a complete
/// `200` whose envelope validates.
async fn sorted_rows(router: axum::Router, aql: &str) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let (status, text) = call(router, query(aql)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.meta.federation.complete, "both products answered");
    let mut rows = answer.rows;
    rows.sort();
    Ok(rows)
}

// conformance: CP-38
#[tokio::test]
async fn both_patient_carriers_resolve_to_the_same_rows_over_two_cdr_products() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;

    // §5.4.3, CP-38: the same patient query once per carrier.
    let from = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
    let via_external_ref = format!(
        "SELECT c/uid/value {from} \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let via_entry = format!(
        "SELECT c/uid/value {from} \
         WHERE o/subject/identifiers/id = '{PATIENT}' \
         AND o/subject/identifiers/issuer = '{NAMESPACE}'"
    );
    let external_ref_rows =
        sorted_rows(gateway(dir.path(), &nodes.a, &nodes.b)?, &via_external_ref).await?;
    let entry_rows = sorted_rows(gateway(dir.path(), &nodes.a, &nodes.b)?, &via_entry).await?;
    assert!(
        !entry_rows.is_empty(),
        "the demo compositions hold observations, so the comparison is not vacuous"
    );
    assert_eq!(
        external_ref_rows, entry_rows,
        "both carriers return the same rows (CP-38)"
    );

    for node in [&nodes.a, &nodes.b] {
        let journal = node.proxy.journal();
        let bodies: Vec<String> = journal
            .iter()
            .map(|capture| String::from_utf8_lossy(&capture.body).into_owned())
            .collect();
        assert_eq!(
            2,
            bodies.len(),
            "{:?} received one query per carrier",
            node.node.product()
        );
        assert_eq!(
            bodies.first(),
            bodies.get(1),
            "{:?} received the same node query from both carriers (§7.1)",
            node.node.product()
        );
        assert!(
            !node.proxy.journal_contains(PATIENT.as_bytes()),
            "{:?} saw the patient identifier in some carrier (N33)",
            node.node.product()
        );
        assert!(
            !node.proxy.journal_contains(b"subject/identifiers"),
            "{:?} received the ENTRY carrier (N33)",
            node.node.product()
        );
    }
    Ok(())
}

/// The `ehr_id` domains of node A and node B at the PIX Manager.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// A PIX Manager that knows the patient at node A's `ehr_id` domain only.
async fn pix_manager_knowing_node_a() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    let answer = format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}}]}}"#
    );
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/fhir/Patient/$ihe-pix"))
        .and(wiremock::matchers::query_param(
            "sourceIdentifier",
            format!("{NAMESPACE}|{PATIENT}"),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(answer.into_bytes(), "application/fhir+json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    server
}

// conformance: CP-3 CP-36
#[tokio::test]
async fn a_pix_resolved_query_asks_only_the_member_that_knows_the_patient() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let pix = pix_manager_knowing_node_a().await;
    let resolver = format!(
        "[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n",
        pix.uri()
    );
    let dir = tempfile::tempdir()?;

    let patient = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        !answer.meta.federation.complete,
        "a not-resolved member leaves the answer incomplete (§11.1)"
    );
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "not-resolved", None),
        ],
        reported,
        "{text}"
    );
    assert_eq!(1, answer.rows.len(), "the rows of node A alone: {text}");

    let journal = nodes.a.proxy.journal();
    let sent = journal.first().ok_or("node A was asked")?;
    assert!(
        String::from_utf8_lossy(&sent.body).contains(&EHR_A.to_string()),
        "node A is asked by its own ehr_id (N7)"
    );
    assert!(
        nodes.b.proxy.journal().is_empty(),
        "node B, where the patient is not known, is never asked (N8)"
    );
    for node in [&nodes.a, &nodes.b] {
        for carried in [PATIENT, NAMESPACE] {
            assert!(
                !node.proxy.journal_contains(carried.as_bytes()),
                "{:?} saw the patient identifier or its namespace (N33)",
                node.node.product()
            );
        }
    }
    let asked = pix.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, asked.len(), "one ITI-83 call for both members");
    Ok(())
}
