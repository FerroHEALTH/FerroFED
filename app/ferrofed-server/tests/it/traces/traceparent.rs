// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `traceparent` a node receives (W3C Trace Context): the gateway starts
//! a trace of its own for every client request, with the export on each node
//! request carries that trace's id and the span id of its own `node_request`
//! span. A client's `traceparent` and `tracestate` are recorded nowhere: no
//! node receives them and no exported span carries them, as parent, link,
//! attribute, event or status, since a trace id can encode any identifier
//! and no check on it could stop one (§5.4.1, N33). With the export off, no
//! node receives a `traceparent`.
//! Every assertion on a node reads what the mock node received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use opentelemetry::trace::{SpanId, TraceId};
use opentelemetry_sdk::trace::SpanData;

use super::{Exported, NAMESPACE, TestResult, attribute, id, named, resolving, the};
use crate::facade::{
    EHR_A, EHR_B, body, dev_gateway, gateway, node_answering, patient_query, post, registry,
};
use crate::path_ehr_id::{ENDPOINT_A, answer, holder, over, stranger};
use crate::support::{Logs, send};

/// The trace id of the client's trace.
const CLIENT_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

/// The span the client sent from.
const CLIENT_SPAN: &str = "00f067aa0ba902b7";

/// The client's `traceparent`: its trace and the span it sent from.
const CLIENT_PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

/// The client's `tracestate`, free text no node may read.
const CLIENT_STATE: &str = "vendor=synthetic-state-0001";

/// Every `traceparent` value `server` received, one list per request.
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

/// Every header value `server` received, each as text with its name.
async fn header_values(server: &Server) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .flat_map(|request| request.headers.iter())
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect())
}

/// Asserts that no header `server` received carries any of `client`, the
/// values of the client's trace context.
async fn none_of_the_clients(server: &Server, client: &[&str]) -> TestResult {
    for (name, value) in header_values(server).await? {
        assert!(name != "tracestate", "a node received a tracestate");
        for fragment in client {
            assert!(
                !value.contains(fragment),
                "the {name} header carries {fragment}"
            );
        }
    }
    Ok(())
}

/// Asserts that `request` is the root of a trace of the gateway's own, not
/// the client's trace `trace`, and returns the gateway's trace id.
fn own_root(request: &SpanData, trace: &str) -> Result<TraceId, Box<dyn Error>> {
    assert_eq!(
        SpanId::INVALID,
        request.parent_span_id,
        "the request span has no parent"
    );
    let gateway = request.span_context.trace_id();
    assert_ne!(
        TraceId::from_hex(trace)?,
        gateway,
        "the gateway starts its own trace"
    );
    Ok(gateway)
}

/// Asserts that no span of `spans` records any of `client`, the values of
/// the client's trace context: not in its ids, its name, an attribute, an
/// event, a link or its status, and that no span has a link at all.
fn recorded_nowhere(spans: &[SpanData], client: &[&str]) {
    for span in spans {
        assert!(
            span.links.links.is_empty(),
            "{} carries a link: {:?}",
            span.name,
            span.links
        );
        let carried = format!(
            "{} {} {} {:?} {:?} {:?}",
            span.span_context.trace_id(),
            span.span_context.span_id(),
            span.name,
            span.attributes,
            span.events,
            span.status
        );
        for fragment in client {
            assert!(
                !carried.contains(fragment),
                "{} records {fragment}: {carried}",
                span.name
            );
        }
    }
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
async fn each_node_joins_the_gateways_own_trace_under_its_own_span() -> TestResult {
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
    let trace = own_root(request, CLIENT_TRACE)?;
    recorded_nowhere(&spans, &[CLIENT_TRACE, CLIENT_SPAN, CLIENT_STATE]);
    for (server, endpoint) in [(&a, "node-a-pub"), (&b, "node-b-pub")] {
        let node = named(&spans, "node_request")
            .into_iter()
            .find(|node| attribute(node, "endpoint_id").as_deref() == Some(endpoint))
            .ok_or("a node_request span for each endpoint")?;
        let expected = format!("00-{trace}-{}-01", id(node));
        assert_eq!(
            vec![vec![expected]],
            traceparents(server).await?,
            "{endpoint}"
        );
        none_of_the_clients(server, &[CLIENT_TRACE, CLIENT_SPAN]).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_routed_read_sends_the_gateways_trace_never_the_clients() -> TestResult {
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
    let spans = exported.spans()?;
    let trace = own_root(the(&spans, "GET /v1/ehr/{ehr_id}")?, CLIENT_TRACE)?;
    recorded_nowhere(&spans, &[CLIENT_TRACE, CLIENT_SPAN, CLIENT_STATE]);
    let node = the(&spans, "node_request")?;
    let expected = format!("00-{trace}-{}-01", id(node));
    assert_eq!(vec![vec![expected]], traceparents(&a).await?);
    none_of_the_clients(&a, &[CLIENT_TRACE, CLIENT_SPAN]).await
}

#[tokio::test]
async fn a_traceparent_that_does_not_parse_is_recorded_nowhere() -> TestResult {
    let exported = Exported::install()?;
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let read = Request::get(format!("/v1/ehr/{EHR_A}"))
        .header("openEHR-federation-endpoint", ENDPOINT_A)
        .header("traceparent", "00-synthetic-not-a-trace-01")
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let spans = exported.spans()?;
    recorded_nowhere(&spans, &["synthetic-not-a-trace"]);
    none_of_the_clients(&a, &["synthetic-not-a-trace"]).await
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
        none_of_the_clients(server, &[CLIENT_TRACE, CLIENT_SPAN]).await?;
    }
    Ok(())
}

/// The synthetic patient identifier, all digits, under the example OID arc.
const DIGITS: &str = "12345";

/// [`DIGITS`] written as the hexadecimal of its ASCII bytes, the form a
/// client can hide it in inside a trace id.
const HEX_DIGITS: &str = "3132333435";

// conformance: CP-26 track-10
#[tokio::test]
async fn an_identifier_hidden_in_the_clients_trace_id_reaches_no_node_and_no_span() -> TestResult {
    let exported = Exported::install()?;
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
    let trace = format!("{HEX_DIGITS:0>32}");
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header("traceparent", format!("00-{trace}-{CLIENT_SPAN}-01"))
        .body(Body::from(body(&aql)?))?;
    let response = send(app, request).await?;
    assert_eq!(StatusCode::OK, response.status());
    let spans = exported.spans()?;
    let gateway_trace = own_root(the(&spans, "POST /v1/query/aql")?, &trace)?;
    for span in &spans {
        assert_eq!(
            gateway_trace,
            span.span_context.trace_id(),
            "one gateway trace"
        );
    }
    recorded_nowhere(&spans, &[trace.as_str(), HEX_DIGITS, CLIENT_SPAN]);
    for server in [&a, &b] {
        assert_eq!(
            1,
            traceparents(server).await?.len(),
            "the node was asked once"
        );
        none_of_the_clients(server, &[trace.as_str(), HEX_DIGITS, CLIENT_SPAN]).await?;
    }
    Ok(())
}
