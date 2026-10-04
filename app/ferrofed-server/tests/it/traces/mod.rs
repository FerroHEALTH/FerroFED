// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The trace export: the spans a request leaves, read from an in-memory span
//! exporter behind the gateway's own export layer ([`spans`]), what no span
//! may carry ([`hygiene`]), and the `traceparent` a node receives
//! ([`traceparent`]). No specification governs tracing: our own design,
//! held to §5.4.1, §5.4.3 and N33 for what a span and a node request carry.

mod config;
mod hygiene;
mod spans;
mod traceparent;

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;

use ferrofed_server::telemetry::{Rendering, traced};
use opentelemetry::trace::{SpanId, TracerProvider as _};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use tracing::subscriber::DefaultGuard;

use crate::facade::{EHR_A, EHR_B};
use crate::support::Logs;

/// The result of a test that returns its setup errors.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// Every attribute key an exported span may carry: the fields the gateway's
/// spans declare, and the three the export layer adds (the span's `target`
/// and its busy and idle time).
pub(crate) const ALLOWED_ATTRIBUTES: [&str; 16] = [
    "target",
    "busy_ns",
    "idle_ns",
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "request_id",
    "endpoint_id",
    "operation",
    "contact",
    "outcome",
    "endpoints",
    "rows",
    "members",
    "resolved",
    "holding",
];

/// An in-memory span exporter behind the gateway's export layer, installed
/// as this thread's subscriber while the value lives.
pub(crate) struct Exported {
    exporter: InMemorySpanExporter,
    provider: SdkTracerProvider,
    _default: DefaultGuard,
}

impl Exported {
    /// Installs the export layer over a fresh in-memory exporter as this
    /// thread's default subscriber, beside the console at `info`.
    pub(crate) fn install() -> Result<Self, Box<dyn Error>> {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = traced(
            Rendering::Json,
            "info",
            false,
            Logs::default(),
            Some(provider.tracer("ferrofed")),
        )?;
        Ok(Self {
            exporter,
            provider,
            _default: tracing::subscriber::set_default(subscriber),
        })
    }

    /// Every span exported so far, in the order each ended.
    pub(crate) fn spans(&self) -> Result<Vec<SpanData>, Box<dyn Error>> {
        self.provider.force_flush()?;
        Ok(self.exporter.get_finished_spans()?)
    }
}

/// The value of the attribute `key` of `span`, as text.
pub(crate) fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == key)
        .map(|pair| pair.value.as_str().into_owned())
}

/// The spans of `spans` named `name`.
pub(crate) fn named<'a>(spans: &'a [SpanData], name: &str) -> Vec<&'a SpanData> {
    spans.iter().filter(|span| span.name == name).collect()
}

/// The one span of `spans` named `name`.
pub(crate) fn the<'a>(spans: &'a [SpanData], name: &str) -> Result<&'a SpanData, Box<dyn Error>> {
    match named(spans, name).as_slice() {
        [only] => Ok(only),
        found => Err(format!("expected one {name} span, found {}", found.len()).into()),
    }
}

/// The span id of `span`.
pub(crate) fn id(span: &SpanData) -> SpanId {
    span.span_context.span_id()
}

/// The namespace of every synthetic identifier these tests resolve, under
/// the example OID arc.
pub(crate) const NAMESPACE: &str = "urn:oid:2.999.1";

/// The `[dev]` rows that resolve the synthetic identifier `value` in
/// [`NAMESPACE`] at node A and node B.
pub(crate) fn resolving(value: &str) -> Result<String, std::fmt::Error> {
    let mut text = String::new();
    for (member, ehr_id) in [("node-a", EHR_A), ("node-b", EHR_B)] {
        write!(
            text,
            "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        )?;
    }
    Ok(text)
}

/// Every attribute key of `spans` outside [`ALLOWED_ATTRIBUTES`].
pub(crate) fn unlisted(spans: &[SpanData]) -> BTreeSet<String> {
    spans
        .iter()
        .flat_map(|span| span.attributes.iter())
        .map(|pair| pair.key.as_str().to_owned())
        .filter(|key| !ALLOWED_ATTRIBUTES.contains(&key.as_str()))
        .collect()
}
