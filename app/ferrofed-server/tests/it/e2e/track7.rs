// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 7, auth conveyance and consent-deny, over two FerroEHR nodes: the
//! caller authenticates to the gateway, each node is reached with the token
//! its token endpoint issued for an RFC 7523 client assertion verified
//! against the JWK Set the gateway publishes, and is told the caller in a
//! token the gateway signs; consent is exercised in the three configurations
//! of §16.3 and on a directed query, and the node stays the gate (§13.1,
//! §13.2.1, §14.3, §16.3 track 7; N8, N24, N25, N27, N27a; CP-16, CP-17,
//! CP-30, CP-36).
//!
//! A FerroEHR node of the harness runs with its own authentication off and
//! holds no consent policy, so a node's consent refusal is the ITS-REST
//! `Error` node B's capturing proxy answers in its place. CP-18 and CP-19
//! are Node obligations, scored against the member CDRs (§16.2). The
//! refusal of an unauthenticated caller is the conformance run's own check
//! ([`ferrofed_server::conformance::scenarios::track7`]).

use ferrofed_engine::conveyance;
use ferrofed_server::conformance::scenarios::track7;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::oauth::{TokenEndpoint, Verdict};
use ferrofed_testkit::proxy::Fault;
use ferrofed_testkit::seed::PatientId;
use http::{Request, StatusCode, header};

use crate::conveyance::{CALLER, published, verified_from};
use crate::e2e::scenario::{
    Options, Reply, asked, captured_field, clear, dev_rows, development, exchange, fixture,
    gateway_with, in_process, nobody_asked, patient_compositions, patient_predicate, post_aql,
    queries, seed_both,
};
use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult};
use crate::support::CLIENT_TOKEN;

/// The client the nodes' authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope the gateway requests onward.
const SCOPE: &str = "system/aql-*.s";

/// The consent refusal code node B's registry entry lists.
const REFUSAL_CODE: &str = "consent-refused";

/// Node B's consent refusal, an ITS-REST `Error` carrying [`REFUSAL_CODE`].
const REFUSAL: &str =
    r#"{"message":"synthetic consent refusal","validationErrors":[],"code":"consent-refused"}"#;

/// The registry line that lists [`REFUSAL_CODE`] for node B's endpoint.
fn refusal_codes() -> String {
    format!("consent_refusal_codes = [\"{REFUSAL_CODE}\"]\n")
}

/// The `[credentials]` tables of both endpoints' OAuth 2.0 grant at
/// `token_url`.
fn oauth2(token_url: &str) -> String {
    ["node-a-pub", "node-b-pub"]
        .iter()
        .map(|endpoint| {
            format!(
                "[credentials.\"{endpoint}\".oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"{SCOPE}\"\n\n"
            )
        })
        .collect::<Vec<_>>()
        .concat()
}

/// Asserts that every request `node` received carries a token `issuer`
/// issued and never the caller's own, and the caller signed for `endpoint`.
fn reached_as_the_gateway_for_the_caller(
    nodes: &TwoNodes,
    issuer: &TokenEndpoint,
    keys: &jsonwebtoken::jwk::JwkSet,
) -> TestResult {
    for (node, endpoint) in [(&nodes.a, "node-a-pub"), (&nodes.b, "node-b-pub")] {
        let journal = node.proxy.journal();
        assert!(!journal.is_empty(), "{endpoint} was asked");
        for capture in &journal {
            let bearer = captured_field(capture, header::AUTHORIZATION.as_str())
                .and_then(|value| value.strip_prefix("Bearer "))
                .ok_or("CP-17: the node receives a bearer credential")?;
            assert!(
                issuer.accepts(bearer),
                "CP-17: {endpoint} receives the token its token endpoint issued"
            );
            assert!(
                !capture.contains(CLIENT_TOKEN.as_bytes()),
                "CP-17: the caller's token never reaches a node"
            );
            let conveyed = captured_field(capture, conveyance::HEADER)
                .ok_or("CP-16: the caller's identity is conveyed")?;
            let claims = verified_from(conveyed, keys, endpoint, CLIENT_ID)?;
            assert_eq!(
                CALLER, claims.sub,
                "CP-16: the node is told the verified caller"
            );
        }
    }
    Ok(())
}

// conformance: CP-16 CP-17 track-7
#[tokio::test]
async fn each_node_is_reached_with_its_onward_token_and_told_the_caller() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let issuer = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    issuer.expect_scope(SCOPE);
    let dir = tempfile::tempdir()?;
    let options = Options {
        tables: oauth2(&issuer.token_url()),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;
    let keys = published(&app).await?;
    issuer.trust(keys.clone());

    let mut request = post_aql(&patient_compositions(), &[])?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {}", *CLIENT_TOKEN).parse()?,
    );
    let reply = exchange(&app, request).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    assert_eq!(2, reply.federated()?.rows.len(), "both nodes answered");
    assert!(
        issuer
            .verdicts()
            .iter()
            .all(|verdict| *verdict == Verdict::Issued),
        "CP-17: every client assertion verified against the published JWK Set"
    );
    reached_as_the_gateway_for_the_caller(&nodes, &issuer, &keys)?;

    clear(&nodes);
    let follow_up = Request::get(format!("/v1/ehr/{EHR_A}")).body(axum::body::Body::empty())?;
    let read = exchange(&app, follow_up).await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.text);
    let routed = nodes.a.proxy.journal();
    let capture = routed.first().ok_or("the follow-up reached node A")?;
    let conveyed = captured_field(capture, conveyance::HEADER)
        .ok_or("CP-16: the follow-up conveys the caller")?;
    assert_eq!(
        CALLER,
        verified_from(conveyed, &keys, "node-a-pub", CLIENT_ID)?.sub
    );
    Ok(())
}

// conformance: CP-17 track-7
#[tokio::test]
async fn a_caller_that_does_not_authenticate_reaches_no_node() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    track7::unauthenticated(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    nobody_asked(&nodes, "CP-17: an unauthenticated request reaches no node");
    Ok(())
}

/// Asserts that `reply` is a `200` with node A's rows and node B reported
/// `consent-denied`, incomplete (§11.3, N27).
fn denied_at_b(reply: &Reply) -> TestResult {
    assert_eq!(
        StatusCode::OK,
        reply.status,
        "CP-30: a consent refusal fails nothing: {}",
        reply.text
    );
    let answer = reply.federated()?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        answer.statuses(),
        "CP-36: the endpoints reflect node B's decision"
    );
    assert!(!answer.meta.federation.complete, "CP-30: complete is false");
    assert_eq!(1, answer.rows.len(), "node B adds no row");
    Ok(())
}

// conformance: CP-30 CP-36 track-7
#[tokio::test]
async fn with_no_consent_service_a_node_refusal_is_reported_and_the_query_succeeds() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        b_endpoint: refusal_codes(),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Reply(StatusCode::FORBIDDEN, REFUSAL));

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    denied_at_b(&reply)?;
    assert_eq!(
        1,
        queries(&nodes.b).len(),
        "track 7 (a): node B was asked and decided"
    );
    Ok(())
}

// conformance: CP-36 track-7
#[tokio::test]
async fn a_member_the_consent_service_denies_is_reported_and_never_dispatched_to() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        resolver: development(&[dev_rows(
            PATIENT,
            &[("node-a", EHR_A), ("node-b", EHR_B)],
            &["node-b"],
        )]),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    denied_at_b(&reply)?;
    assert!(
        asked(&nodes.b).is_empty(),
        "track 7 (b): a member dropped at Step 1 is never dispatched to"
    );
    Ok(())
}

// conformance: CP-30 CP-36 track-7
#[tokio::test]
async fn a_member_the_consent_service_admits_still_refuses_and_the_query_succeeds() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let someone_else = PatientId::new(1, 78);
    let options = Options {
        resolver: development(&[
            dev_rows(PATIENT, &[("node-a", EHR_A), ("node-b", EHR_B)], &[]),
            dev_rows(someone_else, &[], &["node-b"]),
        ]),
        b_endpoint: refusal_codes(),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Reply(StatusCode::FORBIDDEN, REFUSAL));

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    denied_at_b(&reply)?;
    assert_eq!(
        1,
        queries(&nodes.b).len(),
        "track 7 (c): dispatching is no evidence that consent permits release"
    );
    Ok(())
}

// conformance: CP-36 track-7
#[tokio::test]
async fn a_directed_query_is_consent_checked_at_the_node() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        b_endpoint: refusal_codes(),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;
    nodes
        .b
        .proxy
        .set_fault(Fault::Reply(StatusCode::FORBIDDEN, REFUSAL));
    let directed = format!(
        "SELECT c/uid/value FROM ENDPOINT [\"node-b-pub\"] CONTAINS EHR e \
         CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    );

    let reply = exchange(&app, post_aql(&directed, &[])?).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    assert_eq!(
        vec![("node-a-pub", "excluded"), ("node-b-pub", "consent-denied")],
        reply.federated()?.statuses(),
        "CP-36: no localization ran, and the node still applied consent"
    );
    assert_eq!(
        1,
        queries(&nodes.b).len(),
        "N27: the directed node is asked"
    );
    assert!(asked(&nodes.a).is_empty(), "node A was not named");
    Ok(())
}
