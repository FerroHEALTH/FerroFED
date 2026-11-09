// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `DPoP` proofs of every replica in FerroFED's own store (RFC 9449
//! §11.1): a proof one replica accepted is refused by a second over the same
//! database, one proof sent to both at once is accepted once, the table
//! holds the hash of each `jti` alone and the sweep deletes a row past its
//! window, and with the database stopped a `DPoP` request is refused, never
//! checked in memory instead. No specification governs the store: our own
//! design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::containers::{self, database};
use ferrofed_testkit::dpop::Holder;
use http::{Method, Request, StatusCode};

use super::store::{executed, store_toml};
use crate::auth::dpop::{bound, proving};
use crate::e2e::TestResult;
use crate::facade::{registry, settings_with_room};
use crate::support::call;

/// A replica over two members that never answer, its store the one `store`
/// names, checking `DPoP` proofs.
fn replica(dir: &Path, store: &str) -> Result<(Arc<AppState>, Router), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout = '2s'\noverall_timeout_ms = 3000\n{store}"
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(tokio::task::block_in_place(|| AppState::build(&settings))?);
    state.lifecycle().booted();
    let mut server = settings_with_room();
    server.auth = proving()?;
    let router = ferrofed_server::router(Arc::clone(&state), &server);
    Ok((state, router))
}

/// The `jti` of a proof.
#[derive(serde::Deserialize)]
struct Jti {
    jti: String,
}

/// `OPTIONS {base}/`, which any verified caller reaches.
fn options() -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())
}

/// The rows of the proof table at `url`, as `proof` and whether its window
/// is still open.
async fn proofs(url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let driven = tokio::spawn(connection);
    let rows = client
        .query("SELECT proof FROM ferrofed.dpop_proof ORDER BY proof", &[])
        .await?;
    drop(client);
    driven.await??;
    Ok(rows.iter().map(|row| row.get::<usize, String>(0)).collect())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proof_one_replica_accepted_is_refused_by_another() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("dpop_replicas", &[]).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "dpop_replicas", dir.path())?;
    let (_first_state, first) = replica(dir.path(), &store)?;
    let (_second_state, second) = replica(dir.path(), &store)?;
    let holder = Holder::generate()?;
    let (request, proof) = bound(&holder, options()?, None)?;
    let (status, text) = call(first, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (replayed, _) = bound(&holder, options()?, Some(&proof))?;
    let (status, text) = call(second.clone(), replayed).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status,
        "RFC 9449 §11.1: a replay to another replica: {text}"
    );
    let (fresh, _) = bound(&holder, options()?, None)?;
    let (status, text) = call(second, fresh).await?;
    assert_eq!(StatusCode::OK, status, "a fresh proof: {text}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_proof_sent_to_two_replicas_at_once_is_accepted_once() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("dpop_race", &[]).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "dpop_race", dir.path())?;
    let (_a, first) = replica(dir.path(), &store)?;
    let (_b, second) = replica(dir.path(), &store)?;
    let holder = Holder::generate()?;
    for _ in 0..5 {
        let (one, proof) = bound(&holder, options()?, None)?;
        let (two, _) = bound(&holder, options()?, Some(&proof))?;
        let (left, right) = tokio::join!(call(first.clone(), one), call(second.clone(), two));
        let statuses = [left?.0, right?.0];
        assert_eq!(
            1,
            statuses
                .iter()
                .filter(|status| **status == StatusCode::OK)
                .count(),
            "one replica admits the proof and the other refuses it: {statuses:?}"
        );
        assert!(statuses.contains(&StatusCode::UNAUTHORIZED), "{statuses:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_table_holds_the_hash_of_each_jti_alone_and_is_swept() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("dpop_table", &[]).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "dpop_table", dir.path())?;
    let (state, router) = replica(dir.path(), &store)?;
    let holder = Holder::generate()?;
    let (request, proof) = bound(&holder, options()?, None)?;
    let (status, text) = call(router, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let url = server.plain_url("dpop_table");
    let held = proofs(&url).await?;
    let [key] = held.as_slice() else {
        return Err(format!("one proof recorded, got {held:?}").into());
    };
    let jti = jsonwebtoken::dangerous::insecure_decode_claims::<Jti>(&proof)?.jti;
    assert!(
        !key.contains(&jti),
        "RFC 9449 §11.1: the value is never held"
    );
    assert_eq!(43, key.len(), "a base64url SHA-256: {key}");
    executed(
        &url,
        "UPDATE ferrofed.dpop_proof SET expires_at = now() - interval '1 second'",
    )
    .await?;
    let swept = state.store().ok_or("the store is open")?.sweep().await?;
    assert!(
        swept.contains(&(String::from("dpop_proof"), 1)),
        "the row past its window is deleted: {swept:?}"
    );
    assert_eq!(Vec::<String>::new(), proofs(&url).await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_database_stopped_a_dpop_request_is_refused() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("dpop_down", &[]).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "dpop_down", dir.path())?;
    let (_state, router) = replica(dir.path(), &store)?;
    server.container().stop().await?;
    let holder = Holder::generate()?;
    let (request, _) = bound(&holder, options()?, None)?;
    let (status, text) = call(router, request).await?;
    assert_eq!(
        StatusCode::SERVICE_UNAVAILABLE,
        status,
        "fail closed, never checked in memory instead: {text}"
    );
    Ok(())
}
