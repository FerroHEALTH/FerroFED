// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OTLP export over TLS (#644): `[telemetry.otlp_tls]` and
//! `[metrics.otlp_tls]` take effect beside an `https://` collector alone,
//! every file is read and checked at load, a credential in the collector URL
//! passes the transport rule once it is `https://`, and a span reaches a
//! collector that requires a client certificate only when the gateway
//! presents one and trusts the collector's CA. No specification governs the
//! export: our own design.
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
use ferrofed_server::telemetry::Traces;
use ferrofed_testkit::listener::ListenerCertificates;
use ferrofed_testkit::otlp::Collector;
use opentelemetry::trace::Tracer as _;

use super::TestResult;

/// A synthetic secret, which no refusal may quote.
const SECRET: &str = "synthetic-collector-secret-Qv8";

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// The refusal `text` resolves to.
fn refused(text: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("the configuration is refused")?
        .to_string())
}

/// Writes the client identity and the trust roots of `certificates` into
/// `dir`, and returns the `otlp_tls` table naming them.
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

#[test]
fn an_https_collector_takes_a_client_identity_and_trust_roots_from_files() -> TestResult {
    let dir = tempfile::tempdir()?;
    let certificates = ListenerCertificates::generate()?;
    let table = tls_table(dir.path(), &certificates)?;
    for section in ["telemetry", "metrics"] {
        let settings = resolved(&format!(
            "[{section}]\notlp_endpoint = \"https://collector.example.org:4317\"\n\n[{section}.otlp_tls]\n{table}"
        ))?;
        let tls = if section == "telemetry" {
            settings.telemetry.otlp_tls
        } else {
            settings.metrics.otlp_tls
        };
        assert_eq!(Some(certificates.ca()), tls.trust_roots.as_deref());
        assert_eq!(
            Some(certificates.client_identity().trim()),
            tls.client_identity
                .as_ref()
                .map(ferrofed_registry::secret::Secret::expose)
        );
    }
    let plain = resolved("[telemetry]\notlp_endpoint = \"https://collector.example.org:4317\"\n")?;
    assert_eq!(
        None, plain.telemetry.otlp_tls.trust_roots,
        "the platform's roots alone"
    );
    Ok(())
}

#[test]
fn tls_material_beside_an_http_collector_or_none_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = tls_table(dir.path(), &ListenerCertificates::generate()?)?;
    for section in ["telemetry", "metrics"] {
        for endpoint in ["otlp_endpoint = \"http://127.0.0.1:4317\"\n", ""] {
            let error = refused(&format!(
                "[{section}]\n{endpoint}\n[{section}.otlp_tls]\n{table}"
            ))?;
            assert!(
                error.contains(&format!("[{section}.otlp_tls]"))
                    && error.contains(&format!("https:// {section}.otlp_endpoint")),
                "{error}"
            );
        }
    }
    Ok(())
}

#[test]
fn tls_material_that_is_no_pem_of_its_kind_is_refused_without_quoting_it() -> TestResult {
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
            "telemetry.otlp_tls.client_identity",
        ),
        (
            "client_identity_file",
            &garbage,
            "telemetry.otlp_tls.client_identity",
        ),
        (
            "trust_roots_file",
            &garbage,
            "telemetry.otlp_tls.trust_roots_file",
        ),
    ];
    for (key, path, named) in cases {
        let error = refused(&format!(
            "[telemetry]\notlp_endpoint = \"https://collector.example.org:4317\"\n\n[telemetry.otlp_tls]\n{key} = {}\n",
            toml::Value::String(path.display().to_string())
        ))?;
        assert!(error.contains(named), "{error}");
        assert!(!error.contains(SECRET), "{error}");
    }
    Ok(())
}

#[test]
fn a_collector_credential_passes_the_transport_rule_over_https() -> TestResult {
    let production = resolved(&format!(
        "[telemetry]\notlp_endpoint = \"https://gateway:{SECRET}@collector.example.org:4317\"\n"
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
    let settings = resolved(&format!(
        "[telemetry]\notlp_endpoint = \"{}\"\n\n[telemetry.otlp_tls]\n{table}",
        collector.endpoint()
    ))?;
    let endpoint = settings
        .telemetry
        .otlp_endpoint
        .as_ref()
        .ok_or("a collector is configured")?;
    let traces = Traces::new(
        endpoint,
        &settings.telemetry.otlp_tls,
        settings.telemetry.trace_sample_ratio,
    )?;
    traces.tracer().in_span("synthetic-probe", |_context| ());
    // NOTE: a refused handshake fails the flush, which is the outcome this
    // helper measures by the spans the collector holds.
    let _flushed = tokio::task::spawn_blocking(move || traces.shutdown()).await?;
    Ok(collector.spans().len())
}

#[tokio::test]
async fn a_span_reaches_a_mutual_tls_collector_only_with_the_identity_and_the_roots() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let certificates = ListenerCertificates::generate()?;
    let collector = Collector::start_tls(&certificates)?;
    assert!(collector.endpoint().starts_with("https://"));
    let roots = dir.path().join("collector-ca.pem");
    std::fs::write(&roots, certificates.ca())?;
    let roots_only = format!(
        "trust_roots_file = {}\n",
        toml::Value::String(roots.display().to_string())
    );
    assert_eq!(
        0,
        exported(&collector, &roots_only).await?,
        "no client identity: refused"
    );
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
