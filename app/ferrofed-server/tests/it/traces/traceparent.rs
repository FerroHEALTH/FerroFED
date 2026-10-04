// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `traceparent` a node receives (W3C Trace Context): with the export on,
//! each node request carries the trace id of the client's trace and the span
//! id of its own `node_request` span; with the export off, none does; a
//! client's own `traceparent` and `tracestate` never reach a node; and one
//! that would carry a withheld identifier is not sent while the request still
//! is (§5.4.1, N33). Every assertion reads what the mock node received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use opentelemetry::trace::TraceId;

use super::{Exported, NAMESPACE, TestResult, attribute, id, named, resolving, the};
use crate::facade::{
    EHR_A, EHR_B, body, dev_gateway, gateway, node_answering, patient_query, post, registry,
};
use crate::path_ehr_id::{ENDPOINT_A, answer, holder, over, stranger};
use crate::support::{Logs, send};

/// The trace id of the client's trace, which holds no letter outside
/// hexadecimal and no withheld identifier.
const CLIENT_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

/// The client's `traceparent`: its trace and the span it sent from.
const CLIENT_PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

/// The client's `tracestate`, free text no node may read.
const CLIENT_STATE: &str = "vendor=synthetic-state-0001";

/// Every `traceparent` value `server` received, one per request, as text.
async fn traceparents(server: &Server) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .map(|request| {
            request
                .headers
                .get_all("traceparent")
                .iter()
                .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
                .collect()
        })
        .collect())
}

/// Whether `server` received a `tracestate` on any request.
async fn received_state(server: &Server) -> Result<bool, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .any(|request| request.headers.contains_key("tracestate")))
}

/// The façade query of the facade fixtures, with the client's trace context.
fn traced_query() -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    let headers = request.headers_mut();
    headers.insert("traceparent", CLIENT_PARENT.parse()?);
    headers.insert("tracestate", CLIENT_STATE.parse()?);
    Ok(request)
}

#[tokio::test]
async fn each_node_receives_the_clients_trace_id_and_its_own_span_as_parent() -> TestResult {
    let exported = Exported::install()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &rows)?;
    let response = send(app, traced_query()?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let spans = exported.spans()?;
    let request = the(&spans, "POST /v1/query/aql")?;
    assert_eq!(
        TraceId::from_hex(CLIENT_TRACE)?,
        request.span_context.trace_id()
    );
    assert_eq!("00f067aa0ba902b7", request.parent_span_id.to_string());
    assert!(
        request.parent_span_is_remote,
        "the client's span is the parent"
    );
    for (server, endpoint) in [(&a, "node-a-pub"), (&b, "node-b-pub")] {
        let node = named(&spans, "node_request")
            .into_iter()
            .find(|node| attribute(node, "endpoint_id").as_deref() == Some(endpoint))
            .ok_or("a node_request span for each endpoint")?;
        let expected = format!("00-{CLIENT_TRACE}-{}-01", id(node));
        assert_eq!(
            vec![vec![expected]],
            traceparents(server).await?,
            "{endpoint}"
        );
        assert!(!received_state(server).await?, "{endpoint}: no tracestate");
    }
    Ok(())
}

#[tokio::test]
async fn a_routed_read_replaces_the_clients_traceparent_with_its_own() -> TestResult {
    let exported = Exported::install()?;
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let read = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .header("traceparent", CLIENT_PARENT)
        .header("tracestate", CLIENT_STATE)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let node = the(&exported.spans()?, "node_request")?.clone();
    let expected = format!("00-{CLIENT_TRACE}-{}-01", id(&node));
    assert_eq!(vec![vec![expected]], traceparents(&a).await?);
    assert!(!received_state(&a).await?, "no tracestate");
    Ok(())
}

#[tokio::test]
async fn with_the_export_off_no_node_receives_a_traceparent() -> TestResult {
    let console = subscriber(Rendering::Json, "info", false, Logs::default())?;
    let _default = tracing::subscriber::set_default(console);
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &rows)?;
    let response = send(app, traced_query()?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for server in [&a, &b] {
        assert_eq!(vec![Vec::<String>::new()], traceparents(server).await?);
        assert!(!received_state(server).await?, "no tracestate");
    }
    Ok(())
}

/// The synthetic patient identifier, all digits, under the example OID arc.
const DIGITS: &str = "12345";

// conformance: CP-26 track-10
#[tokio::test]
async fn a_traceparent_that_would_carry_the_identifier_is_withheld_and_the_query_still_sent()
-> TestResult {
    let _exported = Exported::install()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &resolving(DIGITS)?,
    )?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{DIGITS}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    // A client chooses the trace id it sends, so this one ends in the
    // identifier, and the gateway's traceparent would continue it.
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            "traceparent",
            format!("00-{DIGITS:0>32}-00f067aa0ba902b7-01"),
        )
        .body(Body::from(body(&aql)?))?;
    let response = send(app, request).await?;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "the query is still answered"
    );
    for server in [&a, &b] {
        assert_eq!(
            vec![Vec::<String>::new()],
            traceparents(server).await?,
            "the node was asked once, with no traceparent"
        );
        let requests = server.received_requests().await.ok_or("recording is on")?;
        for request in &requests {
            for (name, value) in &request.headers {
                assert!(
                    !String::from_utf8_lossy(value.as_bytes()).contains(DIGITS)
                        || name == "x-request-id",
                    "the {name} header carries the identifier"
                );
            }
        }
    }
    Ok(())
}
