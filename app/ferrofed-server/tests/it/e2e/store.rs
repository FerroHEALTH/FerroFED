// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroFED's own store against a real PostgreSQL 18 serving TLS, one
//! database per use on one server: it is reached over TLS under the CA
//! `ca_file` names and refused under roots that do not hold it; two
//! replicas booting together over one empty database apply each migration
//! once; a stored-query database of the earlier release boots with every
//! definition held and every N44 refusal kept (§12.7); with the database
//! stopped the readiness indicator is down and a call that needs the store
//! is refused; and the sweep deletes the rows past their window. No
//! specification governs the store: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Barrier};

use axum::Router;
use axum::body::Body;
use ferrofed_registry::definition::store::StoreError;
use ferrofed_server::config::Config;
use ferrofed_server::config::store::StoreSettings;
use ferrofed_server::state::AppState;
use ferrofed_server::store::migrations::{KNOWN, LEDGER, MigrationError};
use ferrofed_server::store::{INDICATOR, Store};
use ferrofed_server::stored::postgres::TABLE;
use ferrofed_testkit::containers::database::PostgresTls;
use ferrofed_testkit::containers::{self, database};
use http::{Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;
use serde::Deserialize;

use crate::e2e::TestResult;
use crate::facade::{EHR_A, EHR_B, crossref, registry, settings_with_room};
use crate::support::{call, chain, error_body};

/// The qualified name the upgrade's definitions are held under.
const NAME: &str = "org.example::upgraded";

/// The `[store]` table of the database `name` on `server`, its connection
/// string and its CA written to files under `dir`.
pub(crate) fn store_toml(
    server: &PostgresTls,
    name: &str,
    dir: &Path,
) -> Result<String, Box<dyn Error>> {
    let ca = dir.join(format!("{name}-ca.pem"));
    std::fs::write(&ca, server.ca())?;
    let url = dir.join(format!("{name}-url"));
    std::fs::write(&url, format!("{}\n", server.url(name)))?;
    Ok(format!(
        "\n[store]\nurl_file = {}\nca_file = {}\nmax_connections = 2\ntimeout_ms = 5000\n",
        toml::Value::String(url.display().to_string()),
        toml::Value::String(ca.display().to_string()),
    ))
}

/// The store settings of the database `name` on `server`, read through the
/// configuration as a gateway reads them.
pub(crate) fn store_settings(
    server: &PostgresTls,
    name: &str,
    dir: &Path,
) -> Result<StoreSettings, Box<dyn Error>> {
    let text = store_toml(server, name, dir)?;
    Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()?
        .store
        .ok_or_else(|| "the store resolves".into())
}

/// Runs `statement` against the database at `url`, past the gateway.
pub(crate) async fn executed(url: &str, statement: &str) -> TestResult {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let driven = tokio::spawn(connection);
    client.batch_execute(statement).await?;
    drop(client);
    driven.await??;
    Ok(())
}

/// The first column of every row `query` answers at `url`, as text.
async fn column(url: &str, query: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let driven = tokio::spawn(connection);
    let rows = client.query(query, &[]).await?;
    drop(client);
    driven.await??;
    Ok(rows.iter().map(|row| row.get::<usize, String>(0)).collect())
}

/// The migrations the ledger at `url` records, in order.
async fn ledger(url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    column(
        url,
        &format!("SELECT version::text FROM {LEDGER} ORDER BY version"),
    )
    .await
}

/// Every migration this binary has, as the ledger names them.
fn every_migration() -> Vec<String> {
    (1..=KNOWN).map(|version| version.to_string()).collect()
}

/// A gateway over two members that never answer, its stored-query registry
/// on the `postgres` backend over the store `store` names.
fn gateway(dir: &Path, store: &str) -> Result<(Arc<AppState>, Router), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let rows = crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]);
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\n\
         [stored_queries]\nbackend = \"postgres\"\n{store}{rows}"
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(tokio::task::block_in_place(|| AppState::build(&settings))?);
    state.lifecycle().booted();
    let router = ferrofed_server::router(Arc::clone(&state), &settings_with_room());
    Ok((state, router))
}

/// `GET {base}/v1/definition/query/{path}`.
fn get(path: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/definition/query/{path}")).body(Body::empty())
}

/// `PUT {base}/v1/definition/query/{NAME}/{version}` with the AQL `aql`.
fn put(version: &str, aql: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("/v1/definition/query/{NAME}/{version}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(aql.to_owned()))
}

/// A definition naming no patient, marked by `mark`.
fn marked(mark: &str) -> String {
    format!("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = '{mark}'")
}

/// `text` as the body of an SQL string literal, each quote doubled.
fn literal(text: &str) -> String {
    text.replace('\'', "''")
}

/// The readiness document, reduced to what the test reads.
#[derive(Debug, Deserialize)]
struct Readiness {
    state: String,
    indicators: Vec<Indicator>,
}

/// One indicator of the readiness document.
#[derive(Debug, Deserialize)]
struct Indicator {
    name: String,
    state: String,
}

/// The readiness of `router`: its status and the state of the store's
/// indicator.
async fn readiness(router: Router) -> Result<(StatusCode, String), Box<dyn Error>> {
    let (status, body) = call(
        router,
        Request::get("/health/readiness").body(Body::empty())?,
    )
    .await?;
    let report: Readiness = serde_json::from_str(&body)?;
    let store = report
        .indicators
        .iter()
        .find(|indicator| indicator.name == INDICATOR)
        .ok_or("the store has an indicator")?;
    assert_eq!(status == StatusCode::OK, report.state == "up", "{body}");
    Ok((status, store.state.clone()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_store_is_reached_over_tls_under_its_ca_and_refused_under_other_roots() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_tls", &[]).await?;
    let dir = tempfile::tempdir()?;
    let settings = store_settings(&server, "store_tls", dir.path())?;
    let store = tokio::task::block_in_place(|| Store::open(&settings))?;
    store.ping().await?;
    assert_eq!(
        every_migration(),
        ledger(&server.plain_url("store_tls")).await?
    );
    let encrypted = column(
        &server.plain_url("store_tls"),
        "SELECT count(*)::text FROM pg_stat_ssl WHERE ssl",
    )
    .await?;
    assert_ne!(
        vec![String::from("0")],
        encrypted,
        "a connection runs over TLS"
    );

    let untrusted = StoreSettings {
        ca: None,
        ..settings
    };
    let refused = tokio::task::block_in_place(|| Store::open(&untrusted))
        .err()
        .ok_or("a database certificate the platform roots do not hold refuses the open")?;
    assert!(matches!(refused, StoreError::Backend(_)), "{refused:?}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_replicas_booting_together_over_an_empty_database_apply_each_migration_once()
-> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_race", &[]).await?;
    let dir = tempfile::tempdir()?;
    let settings = store_settings(&server, "store_race", dir.path())?;
    let start = Arc::new(Barrier::new(2));
    let replicas: Vec<_> = (0..2)
        .map(|_| {
            let (settings, start) = (settings.clone(), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                Store::open(&settings)
            })
        })
        .collect();
    let mut opened = Vec::new();
    for replica in replicas {
        let store = replica.join().map_err(|_panicked| "a replica panicked")??;
        opened.push(store);
    }
    for store in &opened {
        store.ping().await?;
    }
    let url = server.plain_url("store_race");
    assert_eq!(
        every_migration(),
        ledger(&url).await?,
        "each migration is recorded once, whichever replica applied it"
    );
    let tables = column(
        &url,
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'ferrofed' ORDER BY 1",
    )
    .await?;
    assert_eq!(
        vec![
            String::from("emergency_access"),
            String::from("store_migration"),
            String::from("stored_query_definition")
        ],
        tables
    );
    Ok(())
}

/// The statements the earlier release ran on an empty database, as its
/// stored-query store ran them: the schema, its table of schema versions,
/// the definitions' table at layout version 1.
const EARLIER_RELEASE: &str = "CREATE SCHEMA ferrofed;
CREATE TABLE ferrofed.schema_version (layout text PRIMARY KEY, version integer NOT NULL);
CREATE TABLE ferrofed.stored_query_definition (
  name text NOT NULL,
  version text NOT NULL,
  saved text NOT NULL,
  aql text NOT NULL,
  PRIMARY KEY (name, version)
);
INSERT INTO ferrofed.schema_version (layout, version) VALUES ('stored_query_definition', 1);";

// conformance: CP-40
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stored_query_database_of_the_earlier_release_boots_with_every_definition_and_refusal_kept()
-> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_upgrade", &[]).await?;
    let url = server.plain_url("store_upgrade");
    executed(&url, EARLIER_RELEASE).await?;
    executed(
        &url,
        &format!(
            "INSERT INTO {TABLE} (name, version, saved, aql) VALUES \
             ('{NAME}', '1.0.0', '2026-10-05T00:00:00Z', '{}'), \
             ('{NAME}', '1.1.0', '2026-10-06T00:00:00Z', '{}')",
            literal(&marked("first")),
            literal(&marked("second"))
        ),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "store_upgrade", dir.path())?;
    let (_state, router) = gateway(dir.path(), &store)?;

    for (version, mark) in [("1.0.0", "first"), ("1.1.0", "second")] {
        let (status, text) = call(router.clone(), get(&format!("{NAME}/{version}"))?).await?;
        assert_eq!(StatusCode::OK, status, "{version} is held: {text}");
        let held: StoredQuery = serde_json::from_str(&text)?;
        assert_eq!(marked(mark), held.q, "{version} keeps its text");
    }
    let (status, text) = call(router.clone(), get(NAME)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let listed: Vec<StoredQuery> = serde_json::from_str(&text)?;
    assert_eq!(2, listed.len(), "every definition is held: {text}");

    let (status, text) = call(router.clone(), put("1.0.0", &marked("replaced"))?).await?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "N44: a held version is never replaced: {text}"
    );
    assert_eq!("stored-query-held", error_body(&text)?.code);
    let (status, text) = call(router.clone(), get(&format!("{NAME}/1.0.0"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: StoredQuery = serde_json::from_str(&text)?;
    assert_eq!(marked("first"), held.q, "the refused PUT changed nothing");
    let (status, text) = call(router, put("2.0.0", &marked("third"))?).await?;
    assert_eq!(StatusCode::OK, status, "a new version is stored: {text}");

    assert_eq!(every_migration(), ledger(&url).await?);
    assert_eq!(
        vec![String::from("1")],
        column(
            &url,
            "SELECT version::text FROM ferrofed.schema_version WHERE layout = 'stored_query_definition'"
        )
        .await?,
        "the earlier release's record is left for a rollback"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_database_a_newer_ferrofed_migrated_is_refused_and_left_as_it_was() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_newer", &[]).await?;
    let dir = tempfile::tempdir()?;
    let settings = store_settings(&server, "store_newer", dir.path())?;
    drop(tokio::task::block_in_place(|| Store::open(&settings))?);
    let url = server.plain_url("store_newer");
    let newer = KNOWN + 1;
    executed(
        &url,
        &format!("INSERT INTO {LEDGER} (version) VALUES ({newer})"),
    )
    .await?;
    let refused = tokio::task::block_in_place(|| Store::open(&settings))
        .err()
        .ok_or("a newer database refuses the start")?;
    let StoreError::Backend(error) = &refused else {
        return Err(format!("a backend refusal: {refused:?}").into());
    };
    assert_eq!(
        Some(&MigrationError::Newer {
            found: newer,
            known: KNOWN
        }),
        error.downcast_ref::<MigrationError>()
    );
    assert!(chain(&refused).contains("newer FerroFED"), "{refused}");
    let mut recorded = every_migration();
    recorded.push(newer.to_string());
    assert_eq!(
        recorded,
        ledger(&url).await?,
        "the database is not rewritten"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_database_stopped_the_store_is_not_ready_and_a_call_that_needs_it_is_refused()
-> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_down", &[]).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "store_down", dir.path())?;
    let (state, router) = gateway(dir.path(), &store)?;
    assert_eq!(
        (StatusCode::OK, String::from("up")),
        readiness(router.clone()).await?
    );
    let (status, text) = call(router.clone(), put("1.0.0", &marked("before"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    server.container().stop().await?;
    assert_eq!(
        (StatusCode::SERVICE_UNAVAILABLE, String::from("down")),
        readiness(router.clone()).await?,
        "the store's indicator takes the gateway out of rotation"
    );
    let shared = state.store().ok_or("the store is open")?;
    assert!(
        shared.ping().await.is_err(),
        "a call is refused, never answered"
    );
    let (status, text) = call(router.clone(), put("2.0.0", &marked("during"))?).await?;
    assert!(
        status.is_server_error(),
        "a write that needs the store is refused: {status} {text}"
    );
    let (status, text) = call(router, get(&format!("{NAME}/1.0.0"))?).await?;
    assert!(
        status.is_server_error(),
        "a read that needs the store is refused, never answered from memory: {status} {text}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sweep_deletes_the_rows_past_their_window_and_keeps_the_rest() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("store_sweep", &[]).await?;
    let dir = tempfile::tempdir()?;
    let settings = store_settings(&server, "store_sweep", dir.path())?;
    let store = tokio::task::block_in_place(|| Store::open(&settings))?;
    let url = server.plain_url("store_sweep");
    executed(
        &url,
        &format!(
            "CREATE TABLE ferrofed.window_probe (id text PRIMARY KEY, expires_at timestamptz NOT NULL);
             INSERT INTO ferrofed.window_probe VALUES
               ('past-1', now() - interval '1 hour'),
               ('past-2', now() - interval '1 second'),
               ('future', now() + interval '1 hour');
             INSERT INTO ferrofed.emergency_access VALUES
               ('expired', ARRAY[now() - interval '2 days'], true, now() - interval '1 day');
             CREATE TABLE public.window_elsewhere (id text, expires_at timestamptz NOT NULL);
             INSERT INTO public.window_elsewhere VALUES ('outside', now() - interval '1 hour');
             INSERT INTO {TABLE} (name, version, saved, aql)
               VALUES ('{NAME}', '1.0.0', '2026-10-05T00:00:00Z', '{}');",
            literal(&marked("kept"))
        ),
    )
    .await?;
    let swept = store.sweep().await?;
    assert_eq!(
        vec![
            (String::from("emergency_access"), 1),
            (String::from("window_probe"), 2)
        ],
        swept,
        "every table of the store with a window is swept, the emergency count's included"
    );
    assert_eq!(
        vec![String::from("future")],
        column(&url, "SELECT id FROM ferrofed.window_probe").await?,
        "a row inside its window is kept"
    );
    assert_eq!(
        vec![String::from("outside")],
        column(&url, "SELECT id FROM public.window_elsewhere").await?,
        "only the store's own schema is swept"
    );
    assert_eq!(
        vec![String::from("1")],
        column(&url, &format!("SELECT count(*)::text FROM {TABLE}")).await?,
        "a table without a window is never swept"
    );
    assert_eq!(
        vec![
            (String::from("emergency_access"), 0),
            (String::from("window_probe"), 0)
        ],
        store.sweep().await?
    );
    Ok(())
}
