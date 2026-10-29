// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OTLP export over TLS (#644): the TLS files of `[telemetry.otlp]` take
//! effect beside an `https://` collector alone, every file is read and
//! checked when the export is built, a credential in the collector URL passes
//! the transport rule once it is `https://`, and a span reaches a collector
//! that requires a client certificate only when the gateway presents one and
//! trusts the collector's CA. No specification governs the export: our own
//! design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::transport;
use ferrofed_server::metrics::service;
use ferrofed_testkit::listener::ListenerCertificates;
use ferrofed_testkit::otlp::Collector;
use ferrohealth_telemetry::telemetry::{InitError, Telemetry};
use opentelemetry::trace::Tracer as _;

use super::TestResult;

/// A synthetic secret, which no refusal may quote.
const SECRET: &str = "synthetic-collector-secret-Qv8";

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// The export the configuration `text` builds, or the refusal it is built
/// with.
fn built(text: &str) -> Result<Result<Telemetry, InitError>, Box<dyn std::error::Error>> {
    Ok(Telemetry::new(&resolved(text)?.telemetry, service()))
}

/// Writes the client identity and the trust roots of `certificates` into
/// `dir`, and returns the `[telemetry.otlp]` keys naming them.
fn tls_table(dir: &Path, certificates: &ListenerCertificates) -> Result<String, std::io::Error> {
    let identity = dir.join("collector-client.pem");
    let roots = dir.join("collector-ca.pem");
    std::fs::write(&identity, certificates.client_identity())?;
    std::fs::write(&roots, certificates.ca())?;
    Ok(format!(
        "client_identity_file = {}\ntrust_roots_file = {}\n",
        toml::Value::String(identity.display().to_string()),
        toml::Value::String(roots.display().to_string()),
    ))
}

#[tokio::test]
async fn an_https_collector_takes_a_client_identity_and_trust_roots_from_files() -> TestResult {
    let dir = tempfile::tempdir()?;
    let certificates = ListenerCertificates::generate()?;
    let table = tls_table(dir.path(), &certificates)?;
    let telemetry = built(&format!(
        "[telemetry.otlp]\nendpoint = \"https://collector.example.org:4317\"\n{table}"
    ))??;
    assert!(telemetry.tracer().is_some(), "the export is built");
    let plain = built("[telemetry.otlp]\nendpoint = \"https://collector.example.org:4317\"\n")??;
    assert!(plain.tracer().is_some(), "the platform's roots alone");
    Ok(())
}

#[tokio::test]
async fn tls_material_beside_an_http_collector_or_none_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = tls_table(dir.path(), &ListenerCertificates::generate()?)?;
    let error = built(&format!(
        "[telemetry.otlp]\nendpoint = \"http://127.0.0.1:4317\"\n{table}"
    ))?
    .err()
    .ok_or("TLS material beside an http:// collector is refused")?;
    assert!(
        matches!(error, InitError::TlsWithoutHttps { .. }),
        "{error}"
    );
    let error = built(&format!("[telemetry.otlp]\n{table}"))?
        .err()
        .ok_or("TLS material beside no collector is refused")?;
    assert!(matches!(error, InitError::MissingEndpoint), "{error}");
    Ok(())
}

#[tokio::test]
async fn tls_material_that_is_no_pem_of_its_kind_is_refused_without_quoting_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let certificates = ListenerCertificates::generate()?;
    let no_key = dir.path().join("no-key.pem");
    std::fs::write(&no_key, certificates.ca())?;
    let garbage = dir.path().join("garbage.pem");
    std::fs::write(&garbage, SECRET)?;
    let cases = [
        (
            "client_identity_file",
            &no_key,
            "telemetry.otlp.client_identity_file",
        ),
        (
            "client_identity_file",
            &garbage,
            "telemetry.otlp.client_identity_file",
        ),
        (
            "trust_roots_file",
            &garbage,
            "telemetry.otlp.trust_roots_file",
        ),
    ];
    for (key, path, named) in cases {
        let error = built(&format!(
            "[telemetry.otlp]\nendpoint = \"https://collector.example.org:4317\"\n{key} = {}\n",
            toml::Value::String(path.display().to_string())
        ))?
        .err()
        .ok_or("the material is refused")?
        .to_string();
        assert!(error.contains(named), "{error}");
        assert!(!error.contains(SECRET), "{error}");
    }
    Ok(())
}

#[test]
fn a_collector_credential_passes_the_transport_rule_over_https() -> TestResult {
    let production = resolved(&format!(
        "[telemetry.otlp]\nendpoint = \"https://gateway:{SECRET}@collector.example.org:4317\"\n"
    ))?;
    assert_eq!(
        Vec::<transport::ProtectedSite>::new(),
        transport::check(&production, None)?
    );
    Ok(())
}

/// Exports one span to `collector` with the TLS `table` names, and returns
/// how many spans the collector holds once the export has flushed.
async fn exported(collector: &Collector, table: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let telemetry = built(&format!(
        "[telemetry.otlp]\nendpoint = \"{}\"\n{table}",
        collector.endpoint()
    ))??;
    let tracer = telemetry.tracer().ok_or("the spans are exported")?;
    tracer.in_span("synthetic-probe", |_context| ());
    // NOTE: a refused handshake fails the flush, which is the outcome this
    // helper measures by the spans the collector holds.
    let _flushed = tokio::task::spawn_blocking(move || telemetry.shutdown()).await?;
    Ok(collector.spans().len())
}

#[tokio::test]
async fn a_span_reaches_a_mutual_tls_collector_only_with_the_identity_and_the_roots() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let certificates = ListenerCertificates::generate()?;
    let collector = Collector::start_tls(&certificates)?;
    assert!(collector.endpoint().starts_with("https://"));
    // NOTE: the refusal is the client's own, of a server certificate no root it holds signed, so
    // it is decided in the handshake; a TLS 1.3 server refuses a missing client certificate only
    // after the client has finished its side, which makes that refusal a timing race (#1292).
    let identity = dir.path().join("collector-client.pem");
    std::fs::write(&identity, certificates.client_identity())?;
    let identity_only = format!(
        "client_identity_file = {}\n",
        toml::Value::String(identity.display().to_string())
    );
    assert_eq!(
        0,
        exported(&collector, &identity_only).await?,
        "a collector no trust root names: refused"
    );
    // The collector admits only a client certificate its client CA signed, so a span that
    // arrives proves the gateway presented its identity.
    let table = tls_table(dir.path(), &certificates)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut received = exported(&collector, &table).await?;
    while received == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
        received = collector.spans().len();
    }
    assert_eq!(1, received, "the span arrives over mutual TLS");
    Ok(())
}
