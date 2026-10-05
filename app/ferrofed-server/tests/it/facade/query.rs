// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query against two mock nodes: one `RESULT_SET` over every
//! member that knows the patient, the members a query asks and never asks,
//! a failing or suspended member, and the refusals that ask nobody (§5.4,
//! §7, §9, §11.1; N1, N2, N5, N7, N16, N17, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_engine::hygiene::mask::MASK;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_federation::outcome::ErrorDetail;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    Answer, Column, EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, dev_gateway,
    gateway, node_answering, node_failing, patient_query, post, received, registry, schema,
    statuses, wire,
};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

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
        "no cross-reference answers, so the query fails (§11.1): {text}"
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

// conformance: CP-30
#[tokio::test]
async fn a_node_error_carries_the_nodes_message_with_the_subject_masked() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = Server::start().await;
    let said = format!(
        r#"{{"message":"no EHR at this node for {PATIENT}\r\n\u0007 in namespace {NAMESPACE}"}}"#
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(400).set_body_raw(said.into_bytes(), "application/json"),
        )
        .mount(&b)
        .await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "N37: {text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    let failed = answer
        .meta
        .federation
        .endpoints
        .iter()
        .find(|endpoint| endpoint.id == "node-b-pub")
        .ok_or("node B is reported")?;
    assert_eq!(
        Some(ErrorDetail::Text(format!(
            "the node answered 400 Bad Request: no EHR at this node for {MASK} in namespace {NAMESPACE}"
        ))),
        failed.error,
        "§9.5, §11.1: the node's status and message; §5.4.1, N33: never the subject"
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
             AND c/name/value = CONCAT('SENTINEL-PATIENT', '-{PATIENT_TAIL}')"
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
        let error = error_body(&text)?;
        assert!(
            !error.message.contains(PATIENT) && !error.message.contains(PATIENT_TAIL),
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
