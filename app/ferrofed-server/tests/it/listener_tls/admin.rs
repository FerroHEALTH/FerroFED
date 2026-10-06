// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener over `[metrics.tls]`: it serves `GET /metrics` over
//! TLS, and still records each connection's peer, so a loopback peer keeps
//! the write actions.

use std::sync::Arc;

use ferrofed_server::admin;
use ferrofed_server::listener::TlsListener;
use ferrofed_server::listener::certificates::Certificates;
use ferrofed_server::state::AppState;
use ferrofed_testkit::listener::ListenerCertificates;
use http::StatusCode;
use tokio::net::TcpListener;

use super::{TestResult, client, presented, tls_files};

#[tokio::test(flavor = "multi_thread")]
async fn the_admin_listener_serves_tls_and_knows_its_loopback_peer() -> TestResult {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let mut files = tls_files(&set.write(dir.path())?, false);
    files.table = "metrics.tls";
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let address = tcp.local_addr()?;
    let listener = TlsListener::new(tcp, Arc::new(Certificates::load(files)?))?;
    let app = admin::router(Arc::new(AppState::default()));
    tokio::spawn(admin::serve(listener, app));

    let trusting = client(set.ca(), None)?;
    let (status, leaf) = presented(&trusting, address, "/metrics").await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(set.leaf(), leaf.as_slice());

    let write = trusting
        .post(format!(
            "https://{address}/admin/stored-queries/example::query/1.0.0/distribute"
        ))
        .send()
        .await?;
    assert_ne!(
        StatusCode::FORBIDDEN,
        write.status(),
        "the loopback peer was recorded over TLS"
    );
    Ok(())
}
