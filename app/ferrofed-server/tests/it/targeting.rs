// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `openEHR-federation-endpoint` and `openEHR-federation-organisation`
//! headers through `POST {base}/v1/query/aql`, against three mock nodes
//! (§8.1, §8.4, §8.4.1, §11.1; N11, N19, N20, N35; CP-6, CP-28).
//!
//! The headers select the node set exactly as the AQL directive does: a
//! listed member where the patient is not known is `not-resolved`, a member
//! the selection left out is `excluded`, an identifier the registry does not
//! know is a `400`, and a selection of no endpoint is a `404`. A directive and
//! a header that select the same set proceed, and two that differ are a `400`
//! naming both. No query parameter targets anything, and no node receives a
//! targeting header.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;

use crate::directive::{EHR_C, Nodes, directed, patient};
use crate::facade::{Answer, EHR_A, EHR_B, PATIENT_TAIL, body, post, received, statuses, wire};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// The organisation header.
const ORGANISATION: &str = "openEHR-federation-organisation";

/// The patient at every member.
const EVERYWHERE: [(&str, &str); 3] = [("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)];

/// The undirected query for the patient's compositions.
fn undirected() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient()
    )
}

/// `POST uri` with the ad hoc query `aql` and the header lines `fields`.
fn posted(uri: &str, aql: &str, fields: &[(&str, &str)]) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/json");
    for (name, value) in fields {
        request = request.header(*name, *value);
    }
    Ok(request.body(Body::from(body(aql)?))?)
}

/// `POST /v1/query/aql` with `aql` and the header lines `fields`.
fn with(aql: &str, fields: &[(&str, &str)]) -> Result<Request<Body>, Box<dyn Error>> {
    posted("/v1/query/aql", aql, fields)
}

/// The status and the parsed answer of `request` at a gateway over `nodes`
/// resolving the patient at `rows`.
async fn answered(
    nodes: &Nodes,
    rows: &[(&str, &str)],
    request: Request<Body>,
) -> Result<(StatusCode, Answer), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let (status, text) = call(nodes.gateway(dir.path(), rows)?, request).await?;
    let answer: Answer = serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))?;
    Ok((status, answer))
}

/// The status and the error code of `request`, refused with no node asked.
async fn refused(request: Request<Body>) -> Result<(StatusCode, String, String), Box<dyn Error>> {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(nodes.gateway(dir.path(), &EVERYWHERE)?, request).await?;
    assert_eq!(
        [0, 0, 0],
        nodes.asked().await?,
        "nothing is dispatched: {text}"
    );
    let error = error_body(&text)?;
    Ok((status, error.code, error.message))
}

// conformance: CP-28 CP-6
#[tokio::test]
async fn the_endpoint_header_selects_the_node_set_as_the_directive_does() -> TestResult {
    let rows = [("node-a", EHR_A), ("node-c", EHR_C)];
    let by_header = Nodes::start().await;
    let (status, header) = answered(
        &by_header,
        &rows,
        with(&undirected(), &[(ENDPOINT, "node-a-pub, node-b-pub")])?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "not-resolved is reported, never errored (§8.1)"
    );
    let by_directive = Nodes::start().await;
    let (status, directive) = answered(
        &by_directive,
        &rows,
        post(body(&directed(
            r#"ENDPOINT p ["node-a-pub", "node-b-pub"]"#,
        ))?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status);

    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "not-resolved"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&header),
        "§8.1, §11.1: the header decided about node C"
    );
    assert_eq!(
        statuses(&directive),
        statuses(&header),
        "§8.4: the two mechanisms are equivalent"
    );
    assert_eq!(
        directive.rows, header.rows,
        "CP-28: the same answer either way"
    );
    assert_eq!([1, 0, 0], by_header.asked().await?);
    assert_eq!(
        received(&by_directive.a).await?,
        received(&by_header.a).await?,
        "N7: the targeting changes the node set, never the node query"
    );
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn the_organisation_header_asks_every_endpoint_the_organisation_manages() -> TestResult {
    let nodes = Nodes::start().await;
    let (status, answer) = answered(
        &nodes,
        &EVERYWHERE,
        with(&undirected(), &[(ORGANISATION, "org-a")])?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "excluded"),
            ("node-c-pub", "active"),
        ],
        statuses(&answer),
        "N20: org-a manages the endpoints of node A and node C"
    );
    assert_eq!([1, 0, 1], nodes.asked().await?);
    Ok(())
}

// conformance: CP-28 CP-6
#[tokio::test]
async fn a_header_and_a_directive_that_select_the_same_set_proceed() -> TestResult {
    for (directive, fields) in [
        (
            r#"ENDPOINT p ["node-a-pub", "node-c-pub"]"#,
            vec![(ENDPOINT, "node-c-pub,node-a-pub")],
        ),
        (
            r#"ORGANISATION ["org-a"]"#,
            vec![(ENDPOINT, "node-a-pub, node-c-pub")],
        ),
        (
            r#"ENDPOINT p ["node-a-pub", "node-c-pub"]"#,
            vec![(ORGANISATION, "org-a")],
        ),
        (
            r#"ORGANISATION ["org-a"]"#,
            vec![
                (ENDPOINT, "node-a-pub"),
                (ENDPOINT, "node-c-pub"),
                (ORGANISATION, "org-a"),
            ],
        ),
    ] {
        let nodes = Nodes::start().await;
        let (status, answer) =
            answered(&nodes, &EVERYWHERE, with(&directed(directive), &fields)?).await?;
        assert_eq!(
            StatusCode::OK,
            status,
            "§8.4.1: {directive} with {fields:?}"
        );
        assert_eq!(
            vec![
                ("node-a-pub", "active"),
                ("node-b-pub", "excluded"),
                ("node-c-pub", "active"),
            ],
            statuses(&answer),
            "{directive} with {fields:?}"
        );
        assert_eq!([1, 0, 1], nodes.asked().await?);
    }
    Ok(())
}

// conformance: CP-28 CP-6
#[tokio::test]
async fn a_header_and_a_directive_that_differ_are_refused_400_naming_both_sets() -> TestResult {
    let (status, code, message) = refused(with(
        &directed(r#"ENDPOINT p ["node-a-pub", "node-b-pub"]"#),
        &[(ENDPOINT, "node-a-pub")],
    )?)
    .await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "§8.4.1, N35: never merged, never preferred"
    );
    assert_eq!("targeting-conflict", code);
    assert!(
        message.contains("the endpoint directive selects [node-a-pub, node-b-pub]")
            && message.contains("the openEHR-federation-endpoint header selects [node-a-pub]"),
        "§8.4.1: the error names both sets: {message}"
    );
    assert!(!message.contains(PATIENT_TAIL), "§5.4.3: {message}");

    let (status, code, message) = refused(with(
        &directed(r#"ORGANISATION ["org-a"]"#),
        &[(ORGANISATION, "org-b")],
    )?)
    .await?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "targeting-conflict"),
        (status, code.as_str())
    );
    assert!(
        message.contains("[node-a-pub, node-c-pub]") && message.contains("[node-b-pub]"),
        "the sets are named by the endpoints they select: {message}"
    );
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn the_two_headers_that_differ_are_refused_400() -> TestResult {
    let (status, code, message) = refused(with(
        &undirected(),
        &[(ENDPOINT, "node-a-pub"), (ORGANISATION, "org-a")],
    )?)
    .await?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "targeting-conflict"),
        (status, code.as_str())
    );
    assert!(
        message.contains("the openEHR-federation-endpoint header selects [node-a-pub]")
            && message.contains(
                "the openEHR-federation-organisation header selects [node-a-pub, node-c-pub]"
            ),
        "{message}"
    );
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn an_identifier_a_header_names_that_the_registry_does_not_know_is_refused_400() -> TestResult
{
    for (fields, code) in [
        (
            vec![(ENDPOINT, "node-a-pub, node-z-pub")],
            "endpoint-unknown",
        ),
        (
            vec![
                (ENDPOINT, "node-a-pub"),
                (ENDPOINT, "https://cdr-a.example.org/openehr"),
            ],
            "endpoint-unknown",
        ),
        (vec![(ORGANISATION, "org-a, org-z")], "organisation-unknown"),
        (
            vec![
                (ORGANISATION, "org-a"),
                (ENDPOINT, "node-a-pub, node-z-pub"),
            ],
            "endpoint-unknown",
        ),
    ] {
        let (status, answered, message) = refused(with(&undirected(), &fields)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1: {fields:?}");
        assert_eq!(code, answered, "{fields:?}");
        assert!(
            message.contains("identifier 2 of the openEHR-federation-"),
            "the message locates the identifier: {message}"
        );
        for quoted in ["node-z-pub", "org-z", "https://"] {
            assert!(!message.contains(quoted), "§5.4.3: {message}");
        }
    }
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn an_unknown_header_identifier_is_refused_even_beside_a_valid_directive() -> TestResult {
    let (status, code, _) = refused(with(
        &directed(r#"ENDPOINT p ["node-a-pub"]"#),
        &[(ENDPOINT, "node-z-pub")],
    )?)
    .await?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "endpoint-unknown"),
        (status, code.as_str()),
        "§8.4.1: unknown under either mechanism"
    );
    Ok(())
}

#[tokio::test]
async fn a_header_that_names_no_identifier_is_refused_400() -> TestResult {
    for (fields, code) in [
        ([(ENDPOINT, "")], "endpoint-unknown"),
        ([(ENDPOINT, " , ,")], "endpoint-unknown"),
        ([(ORGANISATION, "")], "organisation-unknown"),
    ] {
        let (status, answered, _) = refused(with(&undirected(), &fields)?).await?;
        assert_eq!(
            (StatusCode::BAD_REQUEST, code),
            (status, answered.as_str()),
            "{fields:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_organisation_header_that_selects_no_endpoint_leaves_no_destination() -> TestResult {
    let (status, code, _) = refused(with(&undirected(), &[(ORGANISATION, "org-c")])?).await?;
    assert_eq!(
        (StatusCode::NOT_FOUND, "no-destination"),
        (status, code.as_str()),
        "§11.2: org-c manages no endpoint"
    );
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn a_query_parameter_targets_nothing() -> TestResult {
    let open = Nodes::start().await;
    let (status, answer) = answered(
        &open,
        &EVERYWHERE,
        posted(
            "/v1/query/aql?endpoint=node-b-pub&organisation=org-b",
            &undirected(),
            &[],
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status);
    assert!(
        statuses(&answer)
            .iter()
            .all(|(_, status)| *status == "active"),
        "§8.4: ?endpoint= is no targeting mechanism: {:?}",
        statuses(&answer)
    );
    assert_eq!([1, 1, 1], open.asked().await?);

    let pinned = Nodes::start().await;
    let (status, answer) = answered(
        &pinned,
        &EVERYWHERE,
        posted(
            "/v1/query/aql?endpoint=node-b-pub",
            &undirected(),
            &[(ENDPOINT, "node-a-pub")],
        )?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the parameter neither targets nor conflicts with the header"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "excluded"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&answer)
    );
    assert_eq!([1, 0, 0], pinned.asked().await?);
    for node in [&open.a, &open.b, &open.c, &pinned.a] {
        let captured = wire(node).await?;
        for absent in ["endpoint=", "organisation="] {
            assert!(
                !captured.contains(absent),
                "a node received {absent:?}: {captured}"
            );
        }
    }
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn no_node_receives_a_targeting_header_or_its_value() -> TestResult {
    let nodes = Nodes::start().await;
    let (status, _) = answered(
        &nodes,
        &EVERYWHERE,
        with(
            &directed(r#"ORGANISATION ["org-a"]"#),
            &[
                (ENDPOINT, "node-a-pub, node-c-pub"),
                (ORGANISATION, "org-a"),
            ],
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!([1, 0, 1], nodes.asked().await?);
    for node in [&nodes.a, &nodes.c] {
        let captured = wire(node).await?;
        for absent in [
            "openEHR-federation-endpoint",
            "openEHR-federation-organisation",
            "node-a-pub",
            "node-c-pub",
            "org-a",
        ] {
            assert!(
                !captured.contains_ignoring_ascii_case(absent),
                "§8.4: a node received {absent:?}: {captured}"
            );
        }
        assert!(!captured.contains(PATIENT_TAIL), "§5.4.1, N33: {captured}");
    }
    Ok(())
}

// conformance: CP-28
#[tokio::test]
async fn an_aggregate_the_header_directs_at_one_endpoint_reaches_it_unchanged() -> TestResult {
    let nodes = Nodes::start().await;
    let query = format!(
        "SELECT COUNT(c/uid/value) AS n FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient()
    );
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        nodes.gateway(dir.path(), &EVERYWHERE)?,
        with(&query, &[(ENDPOINT, "node-a-pub")])?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "N14, §8.4: a single-node aggregate directed by the header: {text}"
    );
    let sent = received(&nodes.a).await?;
    let [sent] = sent.as_slice() else {
        panic!("node A is asked once: {sent:?}");
    };
    assert!(
        sent.contains("COUNT(c/uid/value)") && sent.contains(EHR_A),
        "{sent}"
    );
    assert_eq!([1, 0, 0], nodes.asked().await?);
    Ok(())
}
