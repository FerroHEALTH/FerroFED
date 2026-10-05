// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The next signing key of `[signing]`: published in the JWK Set at
//! `{base}/.well-known/jwks.json` ahead of a rotation and never signing, so
//! every replica can publish the key before any replica signs with it, and
//! refused by `config check` when it is on another curve than the algorithm
//! it is meant for (§13.1, N25; RFC 7517 §5, RFC 7515 §4.1.4, RFC 7518
//! §3.4).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use ferrofed_engine::onward::keys::{KeyError, SigningKey};
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::oauth;
use http::{Request, StatusCode};
use jsonwebtoken::jwk::JwkSet;
use secrecy::SecretString;

use crate::facade::{crossref, registry, settings_with_room};

type TestResult = Result<(), Box<dyn Error>>;

/// The JWK Set location the gateway declares.
const JWKS_URI: &str = "https://gw.example.org/.well-known/jwks.json";

/// A key file holding `pem` in `dir` under `name`, as a TOML string, and
/// the key's `kid`.
fn key_file(dir: &Path, name: &str, pem: &str) -> Result<(toml::Value, String), Box<dyn Error>> {
    let file = dir.join(name);
    std::fs::write(&file, pem)?;
    let kid = SigningKey::from_ec_pem(&SecretString::from(pem.to_owned()))?
        .kid()
        .to_owned();
    Ok((toml::Value::String(file.display().to_string()), kid))
}

/// The configuration text of a development gateway over two nodes, with
/// `signing` as its `[signing]` table.
fn text(dir: &Path, signing: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://127.0.0.1:9/a", "http://127.0.0.1:9/b", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{}\n[signing]\njwks_uri = \"{JWKS_URI}\"\n{signing}",
        crossref(&[("node-a", "2222aaaa-2222-4222-8222-222222222222")])
    ))
}

/// The error the configuration `text` is refused with.
fn refused(text: &str) -> Result<ConfigError, Box<dyn Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new())?.resolve() {
        Ok(_) => Err("the configuration was accepted".into()),
        Err(error) => Ok(error),
    }
}

/// The JWK Set the gateway `text` configures serves, and the `kid` of the
/// key it signs with.
async fn served(text: &str) -> Result<(JwkSet, String), Box<dyn Error>> {
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let signing_kid = settings
        .signing
        .as_ref()
        .ok_or("[signing] is set")?
        .keys
        .current()
        .kid()
        .to_owned();
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let app = ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    );
    let request = Request::get("/.well-known/jwks.json").body(Body::empty())?;
    let response = tower::ServiceExt::oneshot(app, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "the JWK Set is served");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((serde_json::from_slice(&bytes)?, signing_kid))
}

/// The `kid`s `set` publishes, in its order.
fn kids(set: &JwkSet) -> Vec<Option<String>> {
    set.keys
        .iter()
        .map(|jwk| jwk.common.key_id.clone())
        .collect()
}

/// The next key is published after the current one and never signs: the
/// gateway signs with the current key (RFC 7517 §5).
// conformance: CP-17
#[tokio::test]
async fn the_next_key_is_published_and_never_signs() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, current_kid) = key_file(dir.path(), "current.pem", &oauth::es384_pem()?)?;
    let (next, next_kid) = key_file(dir.path(), "next.pem", &oauth::es384_pem()?)?;
    let (set, signing_kid) = served(&text(
        dir.path(),
        &format!("key_file = {current}\nnext_key_file = {next}\n"),
    )?)
    .await?;
    assert_eq!(
        vec![Some(current_kid.clone()), Some(next_kid)],
        kids(&set),
        "the current key, then the next one"
    );
    assert_eq!(current_kid, signing_kid, "the next key never signs");
    Ok(())
}

/// The three keys of a rotation in flight are each published: the current
/// one, the previous one through its overlap window, and the next one.
#[tokio::test]
async fn a_previous_and_a_next_key_are_published_together() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, current_kid) = key_file(dir.path(), "current.pem", &oauth::es384_pem()?)?;
    let (previous, previous_kid) = key_file(dir.path(), "previous.pem", &oauth::es384_pem()?)?;
    let (next, next_kid) = key_file(dir.path(), "next.pem", &oauth::es384_pem()?)?;
    let (set, _) = served(&text(
        dir.path(),
        &format!("key_file = {current}\nprevious_key_file = {previous}\nnext_key_file = {next}\n"),
    )?)
    .await?;
    assert_eq!(
        vec![Some(current_kid), Some(previous_kid), Some(next_kid)],
        kids(&set)
    );
    Ok(())
}

/// A next key on another curve than the current key's is refused, naming
/// `signing.next_key_file`, unless `next_key_algorithm` names its
/// algorithm (RFC 7518 §3.4).
#[tokio::test]
async fn a_next_key_on_another_curve_is_refused_unless_intended() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, _) = key_file(dir.path(), "current.pem", &oauth::es384_pem()?)?;
    let (p256, p256_kid) = key_file(dir.path(), "p256.pem", &oauth::p256_pem()?)?;
    let error = refused(&text(
        dir.path(),
        &format!("key_file = {current}\nnext_key_file = {p256}\n"),
    )?)?;
    assert!(
        matches!(
            &error,
            ConfigError::NextKeyAlgorithm { key, found: "ES256", intended: "ES384" }
                if key == "signing.next_key_file"
        ),
        "{error:?}"
    );
    let (current_p384, _) = key_file(dir.path(), "current2.pem", &oauth::es384_pem()?)?;
    let error = refused(&text(
        dir.path(),
        &format!(
            "key_file = {current_p384}\nnext_key_file = {current}\nnext_key_algorithm = \"ES256\"\n"
        ),
    )?)?;
    assert!(
        matches!(
            &error,
            ConfigError::NextKeyAlgorithm {
                found: "ES384",
                intended: "ES256",
                ..
            }
        ),
        "{error:?}"
    );
    let (set, _) = served(&text(
        dir.path(),
        &format!("key_file = {current}\nnext_key_file = {p256}\nnext_key_algorithm = \"ES256\"\n"),
    )?)
    .await?;
    assert!(
        kids(&set).contains(&Some(p256_kid)),
        "an intended change of curve is published"
    );
    Ok(())
}

/// A next key that is already the current key, or a next algorithm with no
/// next key, is refused, each naming its key.
#[test]
fn a_reused_or_missing_next_key_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, _) = key_file(dir.path(), "current.pem", &oauth::es384_pem()?)?;
    let error = refused(&text(
        dir.path(),
        &format!("key_file = {current}\nnext_key_file = {current}\n"),
    )?)?;
    assert!(
        matches!(
            &error,
            ConfigError::SigningKey { key, source: KeyError::NextReused { .. } }
                if key == "signing.next_key_file"
        ),
        "{error:?}"
    );
    let error = refused(&text(
        dir.path(),
        &format!("key_file = {current}\nnext_key_algorithm = \"ES384\"\n"),
    )?)?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "signing.next_key_file"),
        "{error:?}"
    );
    Ok(())
}

/// The binary's `config check` refuses a next key on another curve than the
/// operator intends, naming the key and quoting no part of the file.
#[test]
fn config_check_refuses_a_next_key_on_another_curve() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, _) = key_file(dir.path(), "current.pem", &oauth::es384_pem()?)?;
    let (p256, _) = key_file(dir.path(), "p256.pem", &oauth::p256_pem()?)?;
    let toml = format!(
        "[signing]\nkey_file = {current}\nnext_key_file = {p256}\njwks_uri = \"{JWKS_URI}\"\n"
    );
    let output = crate::run::binary(&["config", "check"], &toml)?;
    assert_eq!(
        Some(i32::from(ferrofed_server::EXIT_CONFIG)),
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("signing.next_key_file"), "{stderr}");
    assert!(stderr.contains("ES256"), "{stderr}");
    assert!(!stderr.contains("PRIVATE KEY"), "{stderr}");
    Ok(())
}
