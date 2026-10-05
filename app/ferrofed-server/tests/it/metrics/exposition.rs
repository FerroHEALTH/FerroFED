// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener: off by default, never the gateway's own listener,
//! held to loopback unless the operator allows a remote address, and serving
//! a text exposition a Prometheus scraper can read.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write as _;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_server::EXIT_CONFIG;
use ferrofed_server::config::Config;
use ferrofed_server::metrics::{self, CONTENT_TYPE, Metrics};
use http::{Request, StatusCode, header};

use crate::metrics::{count, parse};
use crate::run::binary;
use crate::support::{app, call, send};

type TestResult = Result<(), Box<dyn Error>>;

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<ferrofed_server::config::settings::Settings, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

#[test]
fn the_listener_and_the_push_are_off_by_default() -> TestResult {
    let settings = resolved("")?;
    assert_eq!(None, settings.metrics.listen, "no admin listener");
    assert_eq!(None, settings.metrics.otlp_endpoint, "no OTLP push");
    Ok(())
}

#[tokio::test]
async fn the_gateway_listener_never_serves_metrics() -> TestResult {
    let (status, _) = call(app(), Request::get(metrics::PATH).body(Body::empty())?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "metrics live on their own listener"
    );
    Ok(())
}

#[tokio::test]
async fn the_admin_listener_serves_a_parseable_text_exposition() -> TestResult {
    let surface = metrics::router(Arc::new(Metrics::default()));
    let response = send(
        surface.clone(),
        Request::get(metrics::PATH).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some(CONTENT_TYPE),
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    let samples = parse(&text)?;
    assert!(
        text.contains("# TYPE ferrofed_integrity_incidents_total counter"),
        "{text}"
    );
    assert!(
        text.contains("# TYPE ferrofed_registry_reloads_total counter"),
        "{text}"
    );
    for result in ["applied", "refused"] {
        assert_eq!(
            Some("0".to_owned()),
            count(
                &samples,
                "ferrofed_registry_reloads_total",
                &[("result", result)]
            ),
            "{result} is exposed at zero from boot: {text}"
        );
    }
    let (status, _) = call(surface, Request::get("/health").body(Body::empty())?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "the listener serves metrics alone"
    );
    Ok(())
}

#[tokio::test]
async fn an_otlp_push_shares_the_provider_and_leaves_the_exposition_whole() -> TestResult {
    let pushed =
        Metrics::new(&resolved("[metrics]\notlp_endpoint = \"http://127.0.0.1:4317\"\n")?.metrics)?;
    let alone = Metrics::default();
    let families = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| line.starts_with("# TYPE "))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(families(&alone.render()?), families(&pushed.render()?));
    Ok(())
}

#[test]
fn the_exposition_reader_refuses_what_the_format_does_not_admit() {
    let admitted = "# HELP a_total A.\n# TYPE a_total counter\na_total{k=\"v\\\"w\"} 3\n\
                    # TYPE h histogram\nh_bucket{le=\"+Inf\"} 1\nh_sum 0.5\nh_count 1\n";
    assert!(parse(admitted).is_ok(), "{admitted}");
    for refused in [
        "a_total 1\n",
        "# TYPE a_total counter\na_total{k=v} 1\n",
        "# TYPE a_total counter\na_total{k=\"v\"} one\n",
        "# TYPE a_total counter\n9a_total 1\n",
        "# TYPE a_total widget\na_total 1\n",
        "# TYPE g gauge\ng_bucket 1\n",
        "#TYPE a_total counter\n",
    ] {
        assert!(parse(refused).is_err(), "{refused}");
    }
}

#[test]
fn a_remote_listener_is_refused_unless_allowed() -> TestResult {
    for remote in ["0.0.0.0:9464", "192.0.2.10:9464", "[::]:9464"] {
        let text = format!("[metrics]\nlisten = \"{remote}\"\n");
        let refused = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or("a remote listener is refused")?;
        let shown = refused.to_string();
        assert!(
            shown.contains("metrics.listen") && shown.contains("metrics.allow_remote"),
            "{shown}"
        );
        let allowed = resolved(&format!("{text}allow_remote = true\n"))?;
        assert_eq!(Some(remote.parse()?), allowed.metrics.listen);
    }
    for loopback in ["127.0.0.1:9464", "[::1]:9464"] {
        let settings = resolved(&format!("[metrics]\nlisten = \"{loopback}\"\n"))?;
        assert_eq!(Some(loopback.parse()?), settings.metrics.listen);
    }
    Ok(())
}

#[test]
fn the_listener_never_shares_the_gateway_address() -> TestResult {
    let text = "[server]\nlisten = \"127.0.0.1:8080\"\n\n[metrics]\nlisten = \"127.0.0.1:8080\"\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("a shared address is refused")?;
    assert!(refused.to_string().contains("server.listen"), "{refused}");
    Ok(())
}

#[test]
fn the_otlp_push_takes_an_http_collector_and_refuses_any_other() -> TestResult {
    let settings = resolved("[metrics]\notlp_endpoint = \"http://127.0.0.1:4317\"\n")?;
    assert_eq!(
        Some("http://127.0.0.1:4317/"),
        settings
            .metrics
            .otlp_endpoint
            .as_ref()
            .map(SecretUrl::expose)
    );
    for refused in ["https://collector.example.org:4317", "not a url"] {
        let text = format!("[metrics]\notlp_endpoint = \"{refused}\"\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or("the collector is refused")?;
        assert!(
            error.to_string().contains("metrics.otlp_endpoint"),
            "{error}"
        );
    }
    Ok(())
}

#[test]
fn config_check_refuses_a_remote_listener_without_the_flag() -> TestResult {
    let remote = "[server]\nlisten = \"127.0.0.1:1\"\n\n[metrics]\nlisten = \"0.0.0.0:9464\"\n";
    let output = binary(&["config", "check"], remote)?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("metrics.listen") && stderr.contains("metrics.allow_remote"),
        "{stderr}"
    );
    let allowed = binary(
        &["config", "check"],
        &format!("{remote}allow_remote = true\n"),
    )?;
    assert_eq!(
        Some(0),
        allowed.status.code(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    Ok(())
}

#[test]
fn serve_binds_the_admin_listener_and_fails_loud_when_it_cannot() -> TestResult {
    let taken = TcpListener::bind("127.0.0.1:0")?;
    let address = taken.local_addr()?;
    let config = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\n\n[telemetry]\nformat = \"json\"\n\n[metrics]\nlisten = \"{address}\"\n"
    );
    let output = binary(&["serve"], &config)?;
    drop(taken);
    assert_eq!(
        Some(1),
        output.status.code(),
        "the metrics address is taken"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("binding metrics.listen"), "{stdout}");
    Ok(())
}

/// A `serve` process killed when the test ends, however it ends.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        // NOTE: the process may already have exited; nothing is left to stop.
        let _stopped: std::io::Result<()> = self.0.kill();
        let _reaped: std::io::Result<std::process::ExitStatus> = self.0.wait();
    }
}

#[tokio::test]
async fn serve_answers_the_exposition_on_the_admin_listener() -> TestResult {
    let free = TcpListener::bind("127.0.0.1:0")?;
    let address = free.local_addr()?;
    drop(free);
    let mut file = tempfile::NamedTempFile::new()?;
    write!(
        file,
        "[server]\nlisten = \"127.0.0.1:0\"\n\n[telemetry]\nformat = \"json\"\n\n[metrics]\nlisten = \"{address}\"\n"
    )?;
    let _serving = Serving(
        Command::new(env!("CARGO_BIN_EXE_ferrofed"))
            .args(["serve", "--config"])
            .arg(file.path())
            .env_remove("FERROFED_CONFIG")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let client = reqwest::Client::new();
    let url = format!("http://{address}{}", metrics::PATH);
    let started = Instant::now();
    let text = loop {
        match client.get(&url).send().await {
            Ok(response) if response.status() == StatusCode::OK => break response.text().await?,
            _ if started.elapsed() > Duration::from_secs(20) => {
                return Err("the admin listener never answered".into());
            }
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    };
    let samples = parse(&text)?;
    assert_eq!(
        Some("0".to_owned()),
        count(
            &samples,
            "ferrofed_integrity_incidents_total",
            &[("kind", "EhrIdCollision")]
        ),
        "{text}"
    );
    Ok(())
}
