// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `config.ui = "off"`, the default, changes nothing: no revision route
//! exists on the admin listener, no store is opened or read, a reload reads
//! the file alone, and a key that would configure the store is refused.
//! `config.ui = "managed"` needs the admin listener its API is served on, and
//! a part a revision would hold may not carry a secret inline. No
//! specification governs this: our own design.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_server::admin::{self, Access};
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::revisions::{RevisionsFault, Ui, check_tree};
use ferrofed_server::revision::service;
use ferrofed_server::state::AppState;
use http::{Method, Request, StatusCode, header};

use super::{REMOTE, TestResult, operator};
use crate::support::send_as_is;

/// What `text` resolves to, with its merged tree.
fn resolved(text: &str) -> Result<Result<(), Error>, Box<dyn std::error::Error>> {
    let config = Config::from_sources(Some(text), &BTreeMap::new())?;
    let tree: toml::Table = toml::from_str(text)?;
    Ok(config
        .resolve()
        .and_then(|settings| check_tree(settings.revisions.as_ref(), &tree)))
}

#[test]
fn the_switch_is_off_by_default_and_opens_no_store() -> TestResult {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert_eq!(Ui::Off, config.revisions.ui);
    let settings = config.resolve()?;
    assert!(settings.revisions.is_none());
    let explicit = Config::from_sources(Some("[config]\nui = \"off\"\n"), &BTreeMap::new())?;
    assert_eq!(config, explicit);
    let tree = toml::Table::new();
    let (booted, revisions) = service::boot(settings, &tree)?;
    assert!(revisions.is_none(), "off opens no store");
    assert!(booted.revisions.is_none());
    Ok(())
}

#[test]
fn a_store_key_under_off_is_refused() -> TestResult {
    for key in [
        "backend = \"redb\"",
        "path = \"revisions.redb\"",
        "url_file = \"/run/secrets/url\"",
        "confirm_ms = 60000",
        "approval = [\"consent\"]",
        "secrets_dir = \"/run/ferrofed/secrets\"",
    ] {
        let refused = resolved(&format!("[config]\n{key}\n"))?;
        assert!(
            matches!(refused, Err(Error::Revisions(RevisionsFault::Off { .. }))),
            "{key}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn managed_needs_the_admin_listener_and_a_store_path() -> TestResult {
    let refused = resolved("[config]\nui = \"managed\"\npath = \"revisions.redb\"\n")?;
    assert!(
        matches!(
            refused,
            Err(Error::Revisions(RevisionsFault::NoAdminListener))
        ),
        "{refused:?}"
    );
    let refused = resolved("[metrics]\nlisten = \"127.0.0.1:9\"\n\n[config]\nui = \"managed\"\n")?;
    assert!(
        matches!(&refused, Err(Error::Missing { key }) if key == "config.path"),
        "{refused:?}"
    );
    let refused = resolved(
        "[metrics]\nlisten = \"127.0.0.1:9\"\n\n[config]\nui = \"managed\"\npath = \"r.redb\"\nurl_file = \"/x\"\n",
    )?;
    assert!(
        matches!(refused, Err(Error::Revisions(RevisionsFault::Key { .. }))),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn managed_refuses_a_secret_inline_outside_the_core() -> TestResult {
    let managed = "[metrics]\nlisten = \"127.0.0.1:9\"\nscrape_token = \"synthetic-core-token\"\n\n\
                   [config]\nui = \"managed\"\npath = \"r.redb\"\n";
    assert!(
        resolved(managed)?.is_ok(),
        "a secret of the core stays inline"
    );
    let refused = resolved(&format!(
        "{managed}\n[credentials.\"node-a-pub\"]\nbearer_token = \"synthetic-inline-token\"\n"
    ))?;
    let Err(Error::Revisions(RevisionsFault::InlineSecret { key })) = &refused else {
        return Err(format!("refused as inline: {refused:?}").into());
    };
    assert_eq!("credentials.node-a-pub.bearer_token", key);
    let message = refused
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(!message.contains("synthetic-inline-token"), "{message}");
    let off =
        resolved("[credentials.\"node-a-pub\"]\nbearer_token = \"synthetic-inline-token\"\n")?;
    assert!(off.is_ok(), "off keeps an inline secret as before: {off:?}");
    Ok(())
}

#[tokio::test]
async fn off_serves_no_revision_route() -> TestResult {
    let access = Access::new(&crate::support::auth(), Profile::Development, Vec::new());
    let app = admin::router(Arc::new(AppState::default()), access)
        .layer(MockConnectInfo(SocketAddr::from(REMOTE)));
    for (method, path) in [
        (Method::GET, "/admin/config"),
        (Method::GET, "/admin/config/revisions"),
        (Method::GET, "/admin/config/revisions/1"),
        (Method::GET, "/admin/config/revisions/1/diff/1"),
        (Method::POST, "/admin/config/revisions"),
        (Method::POST, "/admin/config/revisions/1/apply"),
        (Method::POST, "/admin/config/rollback"),
    ] {
        let request = Request::builder()
            .method(method.clone())
            .uri(path)
            .header(header::AUTHORIZATION, operator()?)
            .body(axum::body::Body::empty())?;
        let response = send_as_is(app.clone(), request).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{method} {path}");
        if path == "/admin/config" {
            let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
            let text = String::from_utf8(bytes.to_vec())?;
            assert_eq!(
                "configuration-unmanaged",
                crate::support::error_body(&text)?.code
            );
        }
    }
    Ok(())
}
