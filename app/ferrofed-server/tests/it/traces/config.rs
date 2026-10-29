// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[telemetry.otlp] endpoint`: off by default, an `http://` or `https://`
//! collector, a user name or password in it held to the transport policy of
//! every other credential, and an export that builds inside the runtime and
//! flushes.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;

use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::transport::{self, CleartextError, Encryption, ProtectedSite};
use ferrofed_server::metrics::service;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_telemetry::telemetry::Telemetry;

use super::TestResult;

/// A synthetic secret, which no refusal may quote.
const SECRET: &str = "synthetic-trace-secret-Pw4";

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// The protected site of a trace collector with userinfo.
fn collector_site() -> ProtectedSite {
    ProtectedSite {
        url_key: "telemetry.otlp.endpoint".to_owned(),
        payload: "the userinfo of telemetry.otlp.endpoint".to_owned(),
        requires: Encryption::Https,
    }
}

#[test]
fn the_trace_export_is_off_by_default() -> TestResult {
    assert_eq!(None, resolved("")?.telemetry.otlp);
    Ok(())
}

#[test]
fn the_trace_export_takes_an_http_collector_and_refuses_another_scheme() -> TestResult {
    let settings = resolved("[telemetry.otlp]\nendpoint = \"http://127.0.0.1:4317\"\n")?;
    assert_eq!(
        Some("http://127.0.0.1:4317"),
        settings
            .telemetry
            .otlp
            .as_ref()
            .map(|otlp| SecretUrl::expose(&otlp.endpoint))
    );
    for refused in ["ftp://collector.example.org:4317", "not a url"] {
        let text = format!("[telemetry.otlp]\nendpoint = \"{refused}\"\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or("the collector is refused")?;
        assert!(
            error.to_string().contains("telemetry.otlp.endpoint"),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn a_trace_collector_carrying_userinfo_is_refused_outside_development_and_reported_under_it()
-> TestResult {
    let collector = format!("endpoint = \"http://gateway:{SECRET}@127.0.0.1:4317\"\n");
    let production = resolved(&format!("[telemetry.otlp]\n{collector}"))?;
    let refused = transport::check(&production, None)
        .err()
        .ok_or("a collector with userinfo is refused outside development")?;
    assert_eq!(
        CleartextError {
            site: collector_site()
        },
        refused
    );
    assert!(!refused.to_string().contains(SECRET), "{refused}");
    let development = resolved(&format!(
        "profile = \"development\"\n\n[telemetry.otlp]\n{collector}"
    ))?;
    assert_eq!(
        vec![collector_site()],
        transport::check(&development, None)?
    );
    Ok(())
}

#[tokio::test]
async fn the_export_builds_inside_the_runtime_and_flushes_on_shutdown() -> TestResult {
    let settings = resolved("[telemetry.otlp]\nendpoint = \"http://127.0.0.1:4317\"\n")?;
    let telemetry = Telemetry::new(&settings.telemetry, service())?;
    assert!(telemetry.tracer().is_some(), "the spans are exported");
    tokio::task::spawn_blocking(move || telemetry.shutdown()).await??;
    Ok(())
}
