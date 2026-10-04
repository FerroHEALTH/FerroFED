// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No patient identifier and no query text in any exported span (§5.4.1,
//! §5.4.3, N33): a federated query carrying a synthetic identifier in the
//! AQL, the body's parameters, a header, the client's request id and its
//! trace state leaves neither the identifier nor a query literal in any span
//! name, attribute, event, link or status.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use axum::body::Body;
use http::{Request, StatusCode, header};
use opentelemetry_sdk::trace::SpanData;

use super::{Exported, NAMESPACE, TestResult, named, resolving};
use crate::facade::{gateway, node_answering, registry};
use crate::support::{MINTED_REQUEST_ID, is_minted_form, send};

/// The synthetic patient identifier, under the example OID arc.
const PATIENT: &str = "12345";

/// A literal of the query that is no identifier, which no span may quote
/// either.
const LITERAL: &str = "synthetic-literal-q7";

/// The façade query, naming [`PATIENT`] through `external_ref` and again as
/// a bound parameter, beside [`LITERAL`].
fn query() -> String {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}' \
         AND c/name/value = '{LITERAL}'"
    );
    format!(r#"{{"q":"{aql}","query_parameters":{{"patient":"{PATIENT}"}}}}"#)
}

/// Every piece of text `span` carries, its ids aside: the name, each
/// attribute key and value, each event and link with their attributes, the
/// status, and the instrumentation scope. A request id in the form the
/// gateway mints reads as [`MINTED_REQUEST_ID`], since a random UUID can
/// hold a short all-digit value by chance.
fn text(span: &SpanData) -> String {
    let mut text = vec![span.name.to_string(), format!("{:?}", span.status)];
    for pair in &span.attributes {
        let value = pair.value.as_str().into_owned();
        let minted = pair.key.as_str() == "request_id" && is_minted_form(&value);
        text.push(pair.key.as_str().to_owned());
        text.push(if minted {
            MINTED_REQUEST_ID.to_owned()
        } else {
            value
        });
    }
    for event in span.events.iter() {
        text.push(event.name.to_string());
        text.extend(event.attributes.iter().map(|pair| format!("{pair:?}")));
    }
    for link in span.links.iter() {
        text.extend(link.attributes.iter().map(|pair| format!("{pair:?}")));
    }
    text.push(format!("{:?}", span.instrumentation_scope));
    text.join("\n")
}

#[tokio::test]
async fn no_span_carries_the_patient_identifier_or_a_query_literal() -> TestResult {
    let exported = Exported::install()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &resolving(PATIENT)?,
    )?;
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-patient", PATIENT)
        .header("x-request-id", PATIENT)
        .header("tracestate", format!("vendor={PATIENT}"))
        .body(Body::from(query()))?;
    let response = send(app, request).await?;
    assert_eq!(StatusCode::OK, response.status());
    let spans = exported.spans()?;
    assert_eq!(
        2,
        named(&spans, "node_request").len(),
        "both members were asked, so the scan is not vacuous: {spans:?}"
    );
    for span in &spans {
        let carried = text(span);
        assert!(
            !carried.contains(PATIENT),
            "{} carries the identifier: {carried}",
            span.name
        );
        assert!(
            !carried.contains(LITERAL),
            "{} quotes the query: {carried}",
            span.name
        );
        assert!(
            !carried.contains("COMPOSITION"),
            "{} quotes the query: {carried}",
            span.name
        );
        assert!(span.events.is_empty(), "no event is exported: {carried}");
    }
    Ok(())
}
