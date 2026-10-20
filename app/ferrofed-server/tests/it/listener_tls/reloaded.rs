// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reload `SIGHUP` and `POST /admin/reload` run reads the listener's
//! certificate files with the configuration: a renewed certificate is
//! presented after it, and files that do not form a pair refuse the whole
//! reload, so neither the certificate nor a changed `[auth]` is applied.

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::auth::trust::Trust;
use ferrofed_server::config::Config;
use ferrofed_server::healthcheck::READINESS;
use ferrofed_server::listener::certificates::Certificates;
use ferrofed_server::reload::Reloader;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::listener::ListenerCertificates;
use http::{Request, StatusCode, header};

use super::{TestResult, client, presented, ready_over, tls_files};
use crate::support::{AUDIENCE, auth_toml, call, settings};

/// An issuer the reload would add.
const ADDED: &str = "https://issuer-added.example.test";

/// A gateway with no registry over `auth`, the listener's certificates and
/// a reloader of both, and its client face.
fn gateway(
    dir: &std::path::Path,
    certificates: &Arc<Certificates>,
    auth: &str,
) -> Result<(std::path::PathBuf, Reloader, Router), Box<dyn Error>> {
    let config = dir.join("ferrofed.toml");
    std::fs::write(&config, format!("profile = \"development\"\n{auth}"))?;
    let resolved = Config::load(Some(&config))?.resolve()?;
    let state = Arc::new(AppState::build(&resolved)?);
    let trust = Arc::new(Trust::new(&resolved.server.auth));
    let app = ferrofed_server::trusted_router(Arc::clone(&state), &settings(), Arc::clone(&trust));
    let reloader = Reloader::new(Some(config.clone()), resolved, state)
        .with_certificates(vec![Arc::clone(certificates)])
        .with_trust(trust);
    Ok((config, reloader, app))
}

/// The status `OPTIONS {base}/` answers as a caller of `issuer`: `501`
/// once admitted, since no registry is configured, and `401` when refused.
async fn options_as(app: &Router, issuer: &Issuer) -> Result<StatusCode, Box<dyn Error>> {
    let token = issuer.mint(&Claims::new(issuer.name(), AUDIENCE))?;
    let request = Request::options("/")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())?;
    Ok(call(app.clone(), request).await?.0)
}

/// `[auth]` trusting the suite's issuer and `added`.
fn with_added(added: &Issuer) -> Result<String, Box<dyn Error>> {
    Ok(format!(
        "{}{}",
        auth_toml(),
        crate::support::issuer_toml(added)?
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_renewed_certificate_is_presented_after_the_reload() -> TestResult {
    let dir = tempfile::tempdir()?;
    let first = ListenerCertificates::generate()?;
    let files = first.write(dir.path())?;
    let certificates = Arc::new(Certificates::load(tls_files(&files, false))?);
    let address = ready_over(Arc::clone(&certificates)).await?;
    let (_, reloader, _) = gateway(dir.path(), &certificates, &auth_toml())?;

    let second = ListenerCertificates::generate()?;
    second.write(dir.path())?;
    let applied = reloader.reload()?;
    assert_eq!(vec!["server.tls"], applied.certificates);

    let (status, leaf) = presented(&client(second.ca(), None)?, address, READINESS).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(second.leaf(), leaf.as_slice(), "the renewed certificate");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn files_that_do_not_form_a_pair_refuse_the_whole_reload() -> TestResult {
    let dir = tempfile::tempdir()?;
    let first = ListenerCertificates::generate()?;
    let files = first.write(dir.path())?;
    let certificates = Arc::new(Certificates::load(tls_files(&files, false))?);
    let address = ready_over(Arc::clone(&certificates)).await?;
    let (config, reloader, app) = gateway(dir.path(), &certificates, &auth_toml())?;
    let added = Issuer::new(ADDED)?;

    std::fs::write(
        &config,
        format!("profile = \"development\"\n{}", with_added(&added)?),
    )?;
    let other = ListenerCertificates::generate()?;
    std::fs::write(&files.certificate, other.chain())?;
    let refused = reloader
        .reload()
        .err()
        .ok_or("a certificate beside another certificate's key is refused")?;
    assert_eq!("listener-certificate", refused.class(), "{refused:?}");
    assert_eq!(Some("server.tls"), refused.table());

    assert_eq!(
        StatusCode::UNAUTHORIZED,
        options_as(&app, &added).await?,
        "the changed [auth] is not applied"
    );
    let (status, leaf) = presented(&client(first.ca(), None)?, address, READINESS).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(first.leaf(), leaf.as_slice(), "the running certificate");

    std::fs::write(&files.certificate, first.chain())?;
    reloader.reload()?;
    assert_eq!(
        StatusCode::NOT_IMPLEMENTED,
        options_as(&app, &added).await?,
        "once the files form a pair the reload applies"
    );
    Ok(())
}
