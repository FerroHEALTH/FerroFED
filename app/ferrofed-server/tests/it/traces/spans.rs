// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The spans a request leaves: one trace per client request, its `request`
//! span the root, one `node_request` child per node asked, and every
//! attribute an allow-listed routing id, template, status or count.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;

use axum::body::Body;
use http::{Request, StatusCode};

use super::{Exported, TestResult, attribute, id, named, the, unlisted};
use crate::facade::{EHR_A, EHR_B, body, dev_gateway, node_answering, patient_query, post};
use crate::path_ehr_id::{ENDPOINT_A, answer, holder, over, stranger};
use crate::support::send;

#[tokio::test]
async fn a_federated_query_is_one_trace_with_a_node_request_span_per_node() -> TestResult {
    let exported = Exported::install()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let response = send(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let spans = exported.spans()?;

    let request = the(&spans, "POST /v1/query/aql")?;
    assert_eq!(
        Some("/v1/query/aql".to_owned()),
        attribute(request, "http.route")
    );
    assert_eq!(
        Some("POST".to_owned()),
        attribute(request, "http.request.method")
    );
    assert_eq!(
        Some("200".to_owned()),
        attribute(request, "http.response.status_code")
    );
    let resolve = the(&spans, "resolve")?;
    assert_eq!(Some("2".to_owned()), attribute(resolve, "members"));
    assert_eq!(Some("2".to_owned()), attribute(resolve, "resolved"));
    let fan_out = the(&spans, "fan_out")?;
    assert_eq!(Some("2".to_owned()), attribute(fan_out, "endpoints"));
    let merge = the(&spans, "merge")?;
    assert_eq!(Some("2".to_owned()), attribute(merge, "rows"));
    for parent_is_request in [resolve, fan_out] {
        assert_eq!(id(request), parent_is_request.parent_span_id);
    }
    assert_eq!(id(fan_out), merge.parent_span_id);

    let nodes = named(&spans, "node_request");
    let endpoints: BTreeSet<String> = nodes
        .iter()
        .filter_map(|node| attribute(node, "endpoint_id"))
        .collect();
    assert_eq!(
        BTreeSet::from(["node-a-pub".to_owned(), "node-b-pub".to_owned()]),
        endpoints,
        "one node_request span per node asked"
    );
    for node in &nodes {
        assert_eq!(
            id(fan_out),
            node.parent_span_id,
            "each node call is a child of the fan-out"
        );
        assert_eq!(
            Some("query_execute_adhoc_query_body".to_owned()),
            attribute(node, "operation")
        );
        assert_eq!(Some("active".to_owned()), attribute(node, "outcome"));
        assert_eq!(Some("answered".to_owned()), attribute(node, "contact"));
        assert_eq!(
            Some("200".to_owned()),
            attribute(node, "http.response.status_code")
        );
    }
    let traces: BTreeSet<String> = spans
        .iter()
        .map(|span| span.span_context.trace_id().to_string())
        .collect();
    assert_eq!(1, traces.len(), "the request is one trace: {spans:?}");
    assert!(unlisted(&spans).is_empty(), "{:?}", unlisted(&spans));
    Ok(())
}

#[tokio::test]
async fn a_routed_read_resolved_by_the_probe_leaves_a_probe_span_over_each_member() -> TestResult {
    let exported = Exported::install()?;
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let read = Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?;
    let (status, acting, text) = answer(over(dir.path(), &a, &b)?, read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(ENDPOINT_A), acting.as_deref());
    let spans = exported.spans()?;

    let request = the(&spans, "GET /v1/ehr/{ehr_id}")?;
    assert_eq!(
        Some("/v1/ehr/{ehr_id}".to_owned()),
        attribute(request, "http.route")
    );
    let probe = the(&spans, "probe")?;
    assert_eq!(id(request), probe.parent_span_id);
    assert_eq!(Some("2".to_owned()), attribute(probe, "endpoints"));
    assert_eq!(Some("1".to_owned()), attribute(probe, "holding"));
    let nodes = named(&spans, "node_request");
    assert_eq!(2, nodes.len(), "one probe of each member: {spans:?}");
    let statuses: BTreeSet<String> = nodes
        .iter()
        .filter_map(|node| attribute(node, "http.response.status_code"))
        .collect();
    assert_eq!(
        BTreeSet::from(["200".to_owned(), "404".to_owned()]),
        statuses
    );
    for node in nodes {
        assert_eq!(id(probe), node.parent_span_id);
        assert_eq!(
            Some("ehr_get_by_id".to_owned()),
            attribute(node, "operation")
        );
    }
    assert!(unlisted(&spans).is_empty(), "{:?}", unlisted(&spans));
    Ok(())
}

#[tokio::test]
async fn a_read_routed_to_a_named_node_leaves_one_node_request_under_the_request() -> TestResult {
    let exported = Exported::install()?;
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let read = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let spans = exported.spans()?;

    let request = the(&spans, "GET /v1/ehr/{ehr_id}")?;
    let node = the(&spans, "node_request")?;
    assert_eq!(id(request), node.parent_span_id);
    assert_eq!(Some(ENDPOINT_A.to_owned()), attribute(node, "endpoint_id"));
    assert_eq!(Some("answered".to_owned()), attribute(node, "contact"));
    assert!(
        named(&spans, "probe").is_empty(),
        "the client named the node"
    );
    assert!(unlisted(&spans).is_empty(), "{:?}", unlisted(&spans));
    Ok(())
}

#[tokio::test]
async fn an_unknown_method_is_recorded_as_other_and_never_as_the_client_sent_it() -> TestResult {
    let exported = Exported::install()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    let unusual = Request::builder()
        .method(http::Method::from_bytes(b"SYNTHETIC-METHOD-1")?)
        .uri("/v1/query/aql")
        .body(Body::empty())?;
    send(app, unusual).await?;
    let spans = exported.spans()?;

    let requests: Vec<_> = spans
        .iter()
        .filter(|span| attribute(span, "http.request.method").is_some())
        .collect();
    let [request] = requests.as_slice() else {
        return Err(format!("one request span: {requests:?}").into());
    };
    assert_eq!(
        Some("_OTHER".to_owned()),
        attribute(request, "http.request.method"),
        "OpenTelemetry HTTP conventions: an unknown method is _OTHER"
    );
    assert!(
        request.name.starts_with("HTTP "),
        "an unknown method names the span HTTP: {}",
        request.name
    );
    for span in &spans {
        assert!(!span.name.contains("SYNTHETIC"), "{}", span.name);
        for pair in &span.attributes {
            assert!(
                !pair.value.as_str().contains("SYNTHETIC"),
                "{} carries the client's method",
                pair.key
            );
        }
    }
    Ok(())
}
