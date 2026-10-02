// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `FROM ENDPOINT` and `ORGANISATION` directive through
//! `POST {base}/v1/query/aql`, against three mock nodes (§8.1, §8.4.1, §11.1;
//! N7, N11, N19, N20, N33; CP-6, CP-26).
//!
//! The directive selects the node set and is orthogonal to resolution: a
//! listed member where the patient is not known is `not-resolved`, a member
//! the directive did not name is `excluded`, an identifier the registry does
//! not know is a `400`, and no node receives the directive.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;

use axum::Router;
use http::StatusCode;
use wiremock::MockServer;

use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, gateway,
    node_answering, post, received, registry, statuses, wire,
};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// The patient's `ehr_id` at node C.
pub(crate) const EHR_C: &str = "3333cccc-3333-4333-8333-333333333333";

/// The patient predicate of every fixture, with its namespace.
pub(crate) fn patient() -> String {
    format!(
        "e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// A query for the patient's compositions, `FROM` the directive `directive`.
pub(crate) fn directed(directive: &str) -> String {
    format!(
        "SELECT c/uid/value FROM {directive} CONTAINS EHR e CONTAINS COMPOSITION c WHERE {}",
        patient()
    )
}

/// Three members: node A of `org-a`, node B of `org-b`, and node C, operated
/// by `org-a`, whose endpoint `org-a` manages; and `org-c`, which manages
/// none.
fn members(a: &str, b: &str, c: &str) -> String {
    registry(
        a,
        b,
        &format!(
            r#"
[[organisation]]
id = "org-c"

[[node]]
id = "node-c"
organisation = "org-a"
system_id = "cdr-c.example.org"

[[endpoint]]
id = "node-c-pub"
node = "node-c"
url = "{c}"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#
        ),
    )
}

/// The three mock nodes.
pub(crate) struct Nodes {
    pub(crate) a: MockServer,
    pub(crate) b: MockServer,
    pub(crate) c: MockServer,
}

impl Nodes {
    pub(crate) async fn start() -> Self {
        Self {
            a: node_answering("uid-at-a::cdr-a.example.org::1").await,
            b: node_answering("uid-at-b::cdr-b.example.org::1").await,
            c: node_answering("uid-at-c::cdr-c.example.org::1").await,
        }
    }

    /// A development gateway over the three nodes, resolving the patient at
    /// the members `rows` name.
    pub(crate) fn gateway(
        &self,
        dir: &Path,
        rows: &[(&str, &str)],
    ) -> Result<Router, Box<dyn Error>> {
        gateway(
            dir,
            &members(&self.a.uri(), &self.b.uri(), &self.c.uri()),
            "profile = \"development\"",
            &crossref(rows),
        )
    }

    /// How many requests each node received, A, B and C.
    pub(crate) async fn asked(&self) -> Result<[usize; 3], Box<dyn Error>> {
        Ok([
            received(&self.a).await?.len(),
            received(&self.b).await?.len(),
            received(&self.c).await?.len(),
        ])
    }
}

// conformance: CP-6
#[tokio::test]
async fn a_directed_subject_query_reports_unlisted_members_excluded_and_unknown_ones_not_resolved()
-> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A), ("node-c", EHR_C)])?;
    let query = directed(r#"ENDPOINT p ["node-a-pub", "node-b-pub"]"#);

    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "not-resolved is reported, never errored (§8.1): {text}"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "not-resolved"),
            ("node-c-pub", "excluded"),
        ],
        statuses(&answer),
        "§11.1: the directive decided about node C, which knows the patient"
    );
    assert_eq!(
        vec![vec!["uid-at-a::cdr-a.example.org::1".to_owned()]],
        answer.rows,
        "N11: only the listed members are asked"
    );
    assert_eq!(
        [1, 0, 0],
        nodes.asked().await?,
        "node B does not know the patient, and node C was not named"
    );
    Ok(())
}

// conformance: CP-6 CP-26
#[tokio::test]
async fn no_node_receives_the_directive_and_each_receives_the_undirected_node_query() -> TestResult
{
    let pinned = Nodes::start().await;
    let open = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)];
    let query = directed(r#"ENDPOINT p ["node-a-pub", "node-b-pub", "node-c-pub"]"#);
    let (status, text) = call(pinned.gateway(dir.path(), &rows)?, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let other = tempfile::tempdir()?;
    let undirected = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient()
    );
    let (status, text) = call(
        open.gateway(other.path(), &rows)?,
        post(body(&undirected)?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    for (directed, undirected) in [
        (&pinned.a, &open.a),
        (&pinned.b, &open.b),
        (&pinned.c, &open.c),
    ] {
        assert_eq!(
            received(undirected).await?,
            received(directed).await?,
            "N7: the directive changes the node set, never the node query"
        );
        let captured = wire(directed).await?;
        assert!(!captured.is_empty(), "every listed member was asked");
        for absent in ["ENDPOINT", "node-a-pub", "node-b-pub", "node-c-pub", "p/"] {
            assert!(
                !captured.contains_ignoring_ascii_case(absent),
                "§8.1: a node received {absent:?}: {captured}"
            );
        }
        assert!(
            !captured.contains(PATIENT_TAIL),
            "§5.4.1, N33: a node received the patient identifier: {captured}"
        );
    }
    Ok(())
}

// conformance: CP-6 CP-26
#[tokio::test]
async fn the_organisation_selector_asks_every_endpoint_the_organisation_manages() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B), ("node-c", EHR_C)],
    )?;
    let query = directed(r#"organisation ["org-a"]"#);

    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
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
    for node in [&nodes.a, &nodes.c] {
        let captured = wire(node).await?;
        for absent in ["ORGANISATION", "org-a"] {
            assert!(
                !captured.contains_ignoring_ascii_case(absent),
                "§8.1: a node received {absent:?}: {captured}"
            );
        }
    }
    Ok(())
}

// conformance: CP-6
#[tokio::test]
async fn an_endpoint_the_registry_does_not_know_is_refused_400_before_any_node_is_asked()
-> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    for unknown in ["node-z-pub", "https://cdr-a.example.org/openehr", "node a"] {
        let app = nodes.gateway(dir.path(), &rows)?;
        let query = directed(&format!(r#"ENDPOINT p ["node-a-pub", "{unknown}"]"#));
        let (status, text) = call(app, post(body(&query)?)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1: {unknown}");
        let error = error_body(&text)?;
        assert_eq!("endpoint-unknown", error.code);
        assert!(
            error.message.contains("identifier 2"),
            "the message locates the identifier: {}",
            error.message
        );
        assert!(
            !text.contains(unknown) && !text.contains(PATIENT_TAIL),
            "§5.4.3: the answer quotes the query: {text}"
        );
    }
    assert_eq!([0, 0, 0], nodes.asked().await?, "nothing is dispatched");
    Ok(())
}

#[tokio::test]
async fn an_organisation_the_registry_does_not_know_is_refused_400() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A)])?;
    let query = directed(r#"ORGANISATION ["org-z"]"#);
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4.1");
    let error = error_body(&text)?;
    assert_eq!("organisation-unknown", error.code);
    assert!(!text.contains("org-z"), "§5.4.3: {text}");
    assert_eq!([0, 0, 0], nodes.asked().await?);
    Ok(())
}

#[tokio::test]
async fn an_organisation_that_manages_no_endpoint_leaves_no_destination() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A)])?;
    let query = directed(r#"ORGANISATION ["org-c"]"#);
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2: {text}");
    assert_eq!("no-destination", error_body(&text)?.code);
    assert_eq!([0, 0, 0], nodes.asked().await?);
    Ok(())
}

// conformance: CP-6 CP-37
#[tokio::test]
async fn a_selected_endpoint_attribute_is_added_to_the_rows_and_asked_of_no_node() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A)])?;
    let query = format!(
        r#"SELECT p/id AS endpoint_id, c/uid/value FROM ENDPOINT p ["node-a-pub"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {}"#,
        patient()
    );
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![vec![
            "node-a-pub".to_owned(),
            "uid-at-a::cdr-a.example.org::1".to_owned()
        ]],
        answer.rows,
        "§9.3, N12: the endpoint id comes from the registry"
    );
    assert_eq!([1, 0, 0], nodes.asked().await?);
    let captured = wire(&nodes.a).await?;
    assert!(
        !captured.contains("p/id") && !captured.contains("node-a-pub"),
        "§8.1: the node is asked no ENDPOINT attribute: {captured}"
    );
    Ok(())
}

#[tokio::test]
async fn the_directive_variable_in_where_is_refused_400() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A)])?;
    let query = format!(
        r#"SELECT c/uid/value FROM ENDPOINT p ["node-a-pub"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {} AND p/id = 'node-a-pub'"#,
        patient()
    );
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("endpoint-variable", error_body(&text)?.code);
    assert_eq!([0, 0, 0], nodes.asked().await?);
    Ok(())
}

// conformance: CP-6
#[tokio::test]
async fn a_directed_aggregate_to_one_endpoint_reaches_it_unchanged() -> TestResult {
    let nodes = Nodes::start().await;
    let dir = tempfile::tempdir()?;
    let app = nodes.gateway(dir.path(), &[("node-a", EHR_A), ("node-b", EHR_B)])?;
    let query = format!(
        r#"SELECT COUNT(c/uid/value) AS n FROM ENDPOINT ["node-a-pub"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE {}"#,
        patient()
    );
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "N14: a directed single-node aggregate: {text}"
    );
    let sent = received(&nodes.a).await?;
    let [sent] = sent.as_slice() else {
        panic!("node A is asked once: {sent:?}");
    };
    assert!(
        sent.contains("COUNT(c/uid/value)") && sent.contains(EHR_A),
        "§11.6.3: dispatched unchanged, scoped to the ehr_id: {sent}"
    );
    assert_eq!([1, 0, 0], nodes.asked().await?);
    Ok(())
}
