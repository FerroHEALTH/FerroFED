// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener over `[metrics.tls]`: it serves `GET /metrics` over
//! TLS, refuses a client with no certificate where `client_auth =
//! "required"` names a client CA, and still records each connection's peer,
//! so under the development profile a loopback peer keeps the write actions
//! without a credential.

use std::sync::Arc;

use ferrofed_identity::profile::Profile;
use ferrofed_server::admin::{self, Access};
use ferrofed_server::state::AppState;
use ferrofed_testkit::listener::ListenerCertificates;
use http::StatusCode;
use tokio::net::TcpListener;

use super::{TestResult, client, kept, presented, tls_section};

/// The keys of the admin listener's table.
const METRICS_TLS: ferrohealth_server::tls::TlsKeys = ferrohealth_server::tls_keys!("metrics.tls");

#[tokio::test(flavor = "multi_thread")]
async fn the_admin_listener_serves_tls_and_knows_its_loopback_peer() -> TestResult {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let files = set.write(dir.path())?;
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let address = tcp.local_addr()?;
    let tls = kept(&tls_section(&files, false, false), METRICS_TLS)?;
    let access = Access::new(&crate::support::auth(), Profile::Development, Vec::new());
    let app = admin::router(Arc::new(AppState::default()), access);
    tokio::spawn(admin::serve(tcp, app, Some(tls)));

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
    assert_eq!(
        StatusCode::NOT_FOUND,
        write.status(),
        "the loopback peer was recorded over TLS, and reached the action"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_admin_listener_with_a_client_ca_refuses_a_client_with_no_certificate() -> TestResult {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let files = set.write(dir.path())?;
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let address = tcp.local_addr()?;
    let tls = kept(&tls_section(&files, true, false), METRICS_TLS)?;
    let access = Access::new(&crate::support::auth(), Profile::Development, Vec::new());
    let app = admin::router(Arc::new(AppState::default()), access);
    tokio::spawn(admin::serve(tcp, app, Some(tls)));

    let anonymous = presented(&client(set.ca(), None)?, address, "/metrics").await;
    assert!(
        anonymous.is_err(),
        "a client with no certificate is refused"
    );
    let (status, _) = presented(
        &client(set.ca(), Some(set.client_identity()))?,
        address,
        "/metrics",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "the CA's client is admitted");
    Ok(())
}
