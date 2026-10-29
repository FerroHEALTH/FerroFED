// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! TLS on the gateway's own listeners (#632, #1397), served by
//! `ferrohealth-server` from the tables the gateway resolves: the
//! certificate and key from files, read again on `SIGHUP`; a client
//! certificate required where `client_auth = "required"` names a client CA;
//! TLS 1.3 alone by default and TLS 1.2 with the BCP 195 suites alone where
//! `min_version = "1.2"` (RFC 8446, RFC 9325 §3.1.1, §4.2); `ferrofed
//! healthcheck` over it; the admin listener the same; and `config check`
//! naming each refused key.
//! The certificates are the testkit's, generated at run time. No
//! specification governs the listener: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod admin;
mod clients;
mod config;
mod hangup;
mod reloaded;
mod swap;
mod versions;

use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use ferrofed_server::healthcheck::READINESS;
use ferrofed_testkit::listener::ListenerFiles;
use ferrohealth_server::section::{ClientAuth, ServerSection, TlsSection, TlsVersion};
use ferrohealth_server::tls::{ReloadableTls, TlsKeys};
use http::StatusCode;
use tokio::net::TcpListener;

type TestResult = Result<(), Box<dyn Error>>;

/// The `[server.tls]` table over `files`, its client CA required when
/// `client_ca` is set, speaking TLS 1.2 too when `tls12` is set.
fn tls_section(files: &ListenerFiles, client_ca: bool, tls12: bool) -> TlsSection {
    TlsSection {
        client_auth: if client_ca {
            ClientAuth::Required
        } else {
            ClientAuth::None
        },
        client_ca_file: client_ca.then(|| files.client_ca.clone()),
        min_version: if tls12 {
            TlsVersion::Tls12
        } else {
            TlsVersion::Tls13
        },
        ..TlsSection::new(files.certificate.clone(), files.key.clone())
    }
}

/// The TLS the server keeps for `section`, its faults named under `keys`.
fn kept(section: &TlsSection, keys: TlsKeys) -> Result<Arc<ReloadableTls>, Box<dyn Error>> {
    Ok(Arc::new(ReloadableTls::of(section, keys)?))
}

/// Serves `GET {READINESS}` with `200` over `tls` on a loopback port, as
/// `ferrofed serve` serves its listener, and returns the port's address.
async fn ready_over(tls: Arc<ReloadableTls>) -> Result<SocketAddr, Box<dyn Error>> {
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let address = tcp.local_addr()?;
    let app = Router::new().route(READINESS, get(|| async { (StatusCode::OK, "ready") }));
    tokio::spawn(async move {
        ferrohealth_server::serve::run_with_tls(
            tcp,
            app,
            &ServerSection::default(),
            Some(tls),
            ferrohealth_health::lifecycle::Lifecycle::default(),
            std::future::pending(),
        )
        .await
    });
    Ok(address)
}

/// A client that trusts `ca` alone, presents `identity` when set, opens a
/// connection of its own for each request, and records the certificate it
/// was shown.
fn client(ca: &str, identity: Option<&str>) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .tls_certs_only([reqwest::Certificate::from_pem(ca.as_bytes())?])
        .tls_info(true)
        .pool_max_idle_per_host(0)
        .no_proxy();
    if let Some(identity) = identity {
        builder = builder.identity(reqwest::Identity::from_pem(identity.as_bytes())?);
    }
    builder.build()
}

/// Asks `https://{address}{path}` with `client`, and returns the status and
/// the DER of the certificate the listener presented.
async fn presented(
    client: &reqwest::Client,
    address: SocketAddr,
    path: &str,
) -> Result<(StatusCode, Vec<u8>), Box<dyn Error>> {
    let response = client
        .get(format!("https://{address}{path}"))
        .send()
        .await?;
    let leaf = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(reqwest::tls::TlsInfo::peer_certificate)
        .ok_or("the listener presented a certificate")?
        .to_vec();
    Ok((response.status(), leaf))
}

/// Runs the real binary with `args` over the configuration file `config`.
fn ferrofed(args: &[&str], config: &Path) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_ferrofed"))
        .args(args)
        .arg("--config")
        .arg(config)
        .env_remove("FERROFED_CONFIG")
        .env("FERROFED__USAGE_REPORT__ENABLED", "false")
        .output()
}

/// Returns everything a run wrote, both streams.
fn written(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `path` as a TOML string.
fn quoted(path: &Path) -> toml::Value {
    toml::Value::String(path.display().to_string())
}
