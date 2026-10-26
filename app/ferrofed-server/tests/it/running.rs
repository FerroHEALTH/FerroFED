// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration under `config.ui = "off"`: served on the admin
//! listener beside the reload, behind the operator scope, each setting
//! with the file or the default it comes from, nothing editable, and a
//! secret set inline in the file never in the document or a log. No
//! specification governs this: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::profile::Profile;
use ferrofed_registry::running::{RunningConfig, Source};
use ferrofed_server::admin::{self, Access};
use ferrofed_server::config::Config;
use ferrofed_server::reload::Reloader;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{Request, StatusCode, header};

use crate::support::{self, Logs, OPERATOR_SCOPE, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic secret the file sets inline.
const SECRET: &str = "synthetic-stored-secret-58";

/// The file of an unmanaged gateway, with a stored-query connection string
/// set inline.
fn file() -> String {
    format!(
        "[federation]\nper_node_timeout_ms = 2500\n\n[stored_queries]\nbackend = \"postgres\"\n\
         url = \"postgres://ferrofed:{SECRET}@db.example.org/stored\"\n"
    )
}

/// A reloader of an unmanaged gateway started over [`file`].
fn reloader() -> Result<Arc<Reloader>, Box<dyn Error>> {
    let settings = Config::default().resolve()?;
    let tree: toml::Table = toml::from_str(&file())?;
    Ok(Arc::new(
        Reloader::new(None, settings, Arc::new(AppState::default())).with_tree(tree),
    ))
}

/// The `Authorization` value of an operator.
fn operator() -> Result<String, Box<dyn Error>> {
    let mut claims = support::claims();
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}").trim().to_owned());
    Ok(format!("Bearer {}", support::issuer().mint(&claims)?))
}

/// `GET /admin/config/running` as `authorization`, its status and body.
async fn get(authorization: Option<&str>) -> Result<(StatusCode, String), Box<dyn Error>> {
    let access = Access::new(&support::auth(), Profile::Production, Vec::new());
    let app = admin::serving_router(
        Arc::new(AppState::default()),
        access,
        None,
        Some(reloader()?),
    )
    .layer(MockConnectInfo(SocketAddr::from(([192, 0, 2, 10], 40_000))));
    let mut request = Request::builder().uri("/admin/config/running");
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let response = send_as_is(app, request.body(Body::empty())?).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

#[tokio::test]
async fn the_running_configuration_names_the_file_and_never_a_secret() -> TestResult {
    let (status, body) = get(Some(&operator()?)).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(!body.contains(SECRET), "{body}");
    assert!(!body.contains("ferrofed:"), "{body}");
    let document: RunningConfig = serde_json::from_str(&body)?;
    assert_eq!("off", document.ui);
    let per_node = document
        .setting("federation.per_node_timeout_ms")
        .ok_or("listed")?;
    assert_eq!(Some(toml::Value::Integer(2500)), per_node.value);
    assert_eq!(Source::File, per_node.source);
    assert!(document.settings.iter().all(|setting| !setting.editable));
    let url = document.setting("stored_queries.url").ok_or("listed")?;
    assert_eq!(
        Some(true),
        url.secret.as_ref().map(|secret| secret.set),
        "an inline secret is named as set"
    );
    assert_eq!(None, url.value);
    Ok(())
}

#[tokio::test]
async fn the_running_configuration_needs_the_operator_scope() -> TestResult {
    let (status, _) = get(None).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    let (status, _) = get(Some(&format!(
        "Bearer {}",
        support::issuer().mint(&support::claims())?
    )))
    .await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    Ok(())
}

#[test]
fn reading_the_running_configuration_logs_no_secret() -> TestResult {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "trace", false, logs.clone())?;
    let reloader = reloader()?;
    let written = tracing::subscriber::with_default(capture, || {
        ferrofed_server::running::document(&reloader)
            .map_err(|error| error.to_string())
            .and_then(|document| serde_json::to_string(&document).map_err(|e| e.to_string()))
    })?;
    assert!(!written.contains(SECRET), "{written}");
    let text = logs.text();
    assert!(!text.contains(SECRET), "{text}");
    Ok(())
}

#[test]
fn a_reload_of_the_file_changes_a_live_setting_and_names_the_next_start_of_a_restart_one()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ferrofed.toml");
    let boot =
        "[federation]\nper_node_timeout_ms = 2500\n\n[federation.consent]\ndisclose = true\n";
    std::fs::write(&path, boot)?;
    let (config, tree) = Config::load_with_tree(Some(&path))?;
    let reloader = Reloader::new(
        Some(path.clone()),
        config.resolve()?,
        Arc::new(AppState::default()),
    )
    .with_tree(tree);
    std::fs::write(
        &path,
        "[federation]\nper_node_timeout_ms = 3000\n\n[federation.consent]\ndisclose = false\n",
    )?;
    reloader.reload()?;
    let document = ferrofed_server::running::document(&reloader)?;
    let per_node = document
        .setting("federation.per_node_timeout_ms")
        .ok_or("listed")?;
    assert_eq!(Some(toml::Value::Integer(2500)), per_node.value);
    assert_eq!(Some(toml::Value::Integer(3000)), per_node.next);
    let disclose = document
        .setting("federation.consent.disclose")
        .ok_or("listed")?;
    assert_eq!(Some(toml::Value::Boolean(false)), disclose.value);
    assert_eq!(Source::File, disclose.source);
    Ok(())
}
