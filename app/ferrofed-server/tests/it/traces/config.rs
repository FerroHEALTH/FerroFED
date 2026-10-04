// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[telemetry] otlp_endpoint`: off by default, an `http://` collector only,
//! a user name or password in it held to the transport policy of every other
//! credential, and an export that builds inside the runtime and flushes.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;

use ferrofed_registry::secret::SecretUrl;
use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::transport::{self, CleartextError, Encryption, ProtectedSite};
use ferrofed_server::telemetry::Traces;

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
        url_key: "telemetry.otlp_endpoint".to_owned(),
        payload: "the userinfo of telemetry.otlp_endpoint".to_owned(),
        requires: Encryption::Https,
    }
}

#[test]
fn the_trace_export_is_off_by_default() -> TestResult {
    assert_eq!(None, resolved("")?.telemetry.otlp_endpoint);
    Ok(())
}

#[test]
fn the_trace_export_takes_an_http_collector_and_refuses_any_other() -> TestResult {
    let settings = resolved("[telemetry]\notlp_endpoint = \"http://127.0.0.1:4317\"\n")?;
    assert_eq!(
        Some("http://127.0.0.1:4317/"),
        settings
            .telemetry
            .otlp_endpoint
            .as_ref()
            .map(SecretUrl::expose)
    );
    for refused in ["https://collector.example.org:4317", "not a url"] {
        let text = format!("[telemetry]\notlp_endpoint = \"{refused}\"\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or("the collector is refused")?;
        assert!(
            error.to_string().contains("telemetry.otlp_endpoint"),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn a_trace_collector_carrying_userinfo_is_refused_outside_development_and_reported_under_it()
-> TestResult {
    let collector = format!("otlp_endpoint = \"http://gateway:{SECRET}@127.0.0.1:4317\"\n");
    let production = resolved(&format!("[telemetry]\n{collector}"))?;
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
        "profile = \"development\"\n\n[telemetry]\n{collector}"
    ))?;
    assert_eq!(
        vec![collector_site()],
        transport::check(&development, None)?
    );
    Ok(())
}

#[tokio::test]
async fn the_export_builds_inside_the_runtime_and_flushes_on_shutdown() -> TestResult {
    let settings = resolved("[telemetry]\notlp_endpoint = \"http://127.0.0.1:4317\"\n")?;
    let endpoint = settings
        .telemetry
        .otlp_endpoint
        .as_ref()
        .ok_or("a collector is configured")?;
    let traces = Traces::new(endpoint, settings.telemetry.trace_sample_ratio)?;
    let _tracer = traces.tracer();
    tokio::task::spawn_blocking(move || traces.shutdown()).await??;
    Ok(())
}
