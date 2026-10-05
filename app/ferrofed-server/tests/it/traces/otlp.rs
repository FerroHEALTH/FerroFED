// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The trace export over the OTLP wire: a `serve` process pointed at the
//! testkit's in-process OTLP/gRPC collector answers a federated query, is
//! stopped with `SIGTERM`, and the collector holds that query's spans under
//! the gateway's resource, flushed on the drain, with no synthetic patient
//! identifier in any span, even one hex-encoded in the trace id of a
//! client's `traceparent`, which no span records (§5.4.1, §5.4.3, N33). No
//! specification governs the export: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::fmt::Write as _;
use std::io::Write as _;
use std::net::TcpListener;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ferrofed_testkit::otlp::Collector;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::trace::v1::Span;

use super::{NAMESPACE, TestResult, resolving};
use crate::facade::{body, node_answering, registry};
use crate::support::{MINTED_REQUEST_ID, auth_toml, bearer, is_minted_form, signed};

/// The synthetic patient identifier, under the example OID arc.
const PATIENT: &str = "12345";

/// [`PATIENT`] written as the hexadecimal of its ASCII bytes, the form a
/// client can hide it in inside a trace id.
const HEX_PATIENT: &str = "3132333435";

/// The client's trace id: [`HEX_PATIENT`], zero-padded to 32 digits.
static CLIENT_TRACE: LazyLock<String> = LazyLock::new(|| format!("{HEX_PATIENT:0>32}"));

/// The span the client sent from.
const CLIENT_SPAN: &str = "00f067aa0ba902b7";

/// How long the process may take to listen, and to drain and exit.
const PATIENCE: Duration = Duration::from_secs(20);

/// A `serve` process killed when the test ends, however it ends.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        // The process may already have exited; nothing is left to stop.
        let _killed: std::io::Result<()> = self.0.kill();
        let _reaped: std::io::Result<ExitStatus> = self.0.wait();
    }
}

impl Serving {
    /// Sends `SIGTERM` and waits for the process to exit.
    fn terminate(&mut self) -> Result<ExitStatus, Box<dyn Error>> {
        let sent = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status()?;
        assert!(sent.success(), "SIGTERM was sent");
        let started = Instant::now();
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() > PATIENCE {
                return Err("the process did not exit after SIGTERM".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The text of an attribute value, or its `Debug` form when it is no
/// string.
fn text(value: Option<&AnyValue>) -> String {
    match value.and_then(|value| value.value.as_ref()) {
        Some(Value::StringValue(text)) => text.clone(),
        other => format!("{other:?}"),
    }
}

/// The value of the attribute `key` of `attributes`, as text.
fn attribute(attributes: &[KeyValue], key: &str) -> Option<String> {
    attributes
        .iter()
        .find(|pair| pair.key == key)
        .map(|pair| text(pair.value.as_ref()))
}

/// Every piece of text `span` carries, its ids and times aside, with a
/// request id in the form the gateway mints read as [`MINTED_REQUEST_ID`].
fn carried(span: &Span) -> String {
    let mut carried = vec![span.name.clone(), format!("{:?}", span.status)];
    for pair in &span.attributes {
        let value = text(pair.value.as_ref());
        let minted = pair.key == "request_id" && is_minted_form(&value);
        carried.push(pair.key.clone());
        carried.push(if minted {
            MINTED_REQUEST_ID.to_owned()
        } else {
            value
        });
    }
    for event in &span.events {
        carried.push(event.name.clone());
        carried.extend(event.attributes.iter().map(|pair| format!("{pair:?}")));
    }
    for link in &span.links {
        carried.extend(link.attributes.iter().map(|pair| format!("{pair:?}")));
    }
    carried.join("\n")
}

/// The ids `span` carries, its own, its parent's and every link's, written
/// as a `traceparent` writes them.
fn ids(span: &Span) -> String {
    let links = span
        .links
        .iter()
        .flat_map(|link| [hex(&link.trace_id), hex(&link.span_id)]);
    [
        hex(&span.trace_id),
        hex(&span.span_id),
        hex(&span.parent_span_id),
    ]
    .into_iter()
    .chain(links)
    .collect::<Vec<_>>()
    .join(" ")
}

/// `bytes` written as lowercase hexadecimal, as a `traceparent` writes ids.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(text, "{byte:02x}");
        text
    })
}

/// A free loopback port.
fn free_port() -> Result<std::net::SocketAddr, std::io::Error> {
    TcpListener::bind("127.0.0.1:0")?.local_addr()
}

#[tokio::test]
async fn a_federated_querys_spans_reach_an_otlp_collector_and_are_flushed_on_the_drain()
-> TestResult {
    let collector = Collector::start()?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(&a.uri(), &b.uri(), ""))?;
    let listen = free_port()?;
    let config = signed(&format!(
        "profile = \"development\"\n\n[server]\nlisten = \"{listen}\"\n\n[telemetry]\nformat = \"json\"\notlp_endpoint = \"{}\"\n\n[registry]\ndocument = {}\n\n[federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n{}{}",
        collector.endpoint(),
        toml::Value::String(document.display().to_string()),
        resolving(PATIENT)?,
        auth_toml()?,
    ));
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(config.as_bytes())?;
    let mut serving = Serving(
        Command::new(env!("CARGO_BIN_EXE_ferrofed"))
            .args(["serve", "--config"])
            .arg(file.path())
            .env_remove("FERROFED_CONFIG")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let client = reqwest::Client::new();
    let started = Instant::now();
    while client
        .get(format!("http://{listen}/health"))
        .send()
        .await
        .is_err()
    {
        if started.elapsed() > PATIENCE {
            return Err("the gateway never listened".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let answer = client
        .post(format!("http://{listen}/v1/query/aql"))
        .header("content-type", "application/json")
        .header("authorization", bearer()?)
        .header(
            "traceparent",
            format!("00-{}-{CLIENT_SPAN}-01", *CLIENT_TRACE),
        )
        .header("tracestate", format!("vendor={HEX_PATIENT}"))
        .body(body(&aql)?)
        .send()
        .await?;
    assert_eq!(
        http::StatusCode::OK,
        answer.status(),
        "{}",
        answer.text().await?
    );
    let status = serving.terminate()?;
    assert!(
        status.success(),
        "the gateway drained and exited cleanly: {status}"
    );
    exported_clean(&collector);
    Ok(())
}

/// Asserts that `collector` received the federated query's spans under the
/// gateway's resource, and that no span records the synthetic identifier or
/// anything of the client's trace context.
fn exported_clean(collector: &Collector) {
    let received = collector.received();
    let resources: Vec<_> = received
        .iter()
        .flat_map(|request| &request.resource_spans)
        .filter_map(|spans| spans.resource.as_ref())
        .collect();
    assert!(!resources.is_empty(), "spans arrived over OTLP");
    for resource in resources {
        assert_eq!(
            Some("ferrofed".to_owned()),
            attribute(&resource.attributes, "service.name"),
            "the resource names the service"
        );
        assert_eq!(
            Some(ferrofed_server::body::VERSION.to_owned()),
            attribute(&resource.attributes, "service.version"),
            "the resource names the version"
        );
    }
    let spans = collector.spans();
    let names: Vec<&str> = spans.iter().map(|span| span.name.as_str()).collect();
    assert!(names.contains(&"POST /v1/query/aql"), "{names:?}");
    assert_eq!(
        2,
        names.iter().filter(|name| **name == "node_request").count(),
        "one node request span per member: {names:?}"
    );
    for span in &spans {
        let carried = carried(span);
        assert!(!carried.contains(PATIENT), "{}: {carried}", span.name);
        assert!(!carried.contains(HEX_PATIENT), "{}: {carried}", span.name);
        assert!(span.links.is_empty(), "{} carries a link", span.name);
        let ids = ids(span);
        for client in [CLIENT_TRACE.as_str(), HEX_PATIENT, CLIENT_SPAN] {
            assert!(
                !ids.contains(client),
                "{} carries the client's {client}: {ids}",
                span.name
            );
        }
    }
}
