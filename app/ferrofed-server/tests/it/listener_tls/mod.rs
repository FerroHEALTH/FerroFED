// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! TLS on the gateway's own listeners (#632): the certificate and key from
//! files, read again on `SIGHUP`; a client certificate required where a
//! client CA is set; TLS 1.3 preferred and TLS 1.2 with the BCP 195 suites
//! alone (RFC 8446, RFC 9325 §3.1.1, §4.2); `ferrofed healthcheck` over it;
//! the admin listener the same; and `config check` naming each refused key.
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
use ferrofed_server::listener::TlsListener;
use ferrofed_server::listener::certificates::{Certificates, TlsFiles};
use ferrofed_testkit::listener::ListenerFiles;
use http::StatusCode;
use tokio::net::TcpListener;

type TestResult = Result<(), Box<dyn Error>>;

/// The files of `[server.tls]` over `files`, with the client CA and the
/// healthcheck identity when `client_ca` is set.
fn tls_files(files: &ListenerFiles, client_ca: bool) -> TlsFiles {
    TlsFiles {
        table: "server.tls",
        certificate: files.certificate.clone(),
        key: files.key.clone(),
        client_ca: client_ca.then(|| files.client_ca.clone()),
        healthcheck_identity: None,
    }
}

/// Serves `GET {READINESS}` with `200` over TLS from `certificates` on a
/// loopback port, and returns the port's address.
async fn ready_over(certificates: Arc<Certificates>) -> Result<SocketAddr, Box<dyn Error>> {
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let address = tcp.local_addr()?;
    let listener = TlsListener::new(tcp, certificates)?;
    let app = Router::new().route(READINESS, get(|| async { (StatusCode::OK, "ready") }));
    tokio::spawn(async move { axum::serve(listener, app).await });
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
