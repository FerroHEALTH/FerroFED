// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration revisions under `config.ui = "managed"`: the store, the
//! revision API on the admin listener behind the operator scope, the file
//! load recorded as a revision, apply through the reload path, rollback,
//! optimistic concurrency, no secret value stored or returned, and
//! `config.ui = "off"` changing nothing. No specification governs this: our
//! own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod api;
mod off;
mod secrets;
mod store;

use std::error::Error;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_server::admin::{self, Access};
use ferrofed_server::config::Config;
use ferrofed_server::reload::Reloader;
use ferrofed_server::revision::routes::Managed;
use ferrofed_server::revision::service::{self, Revisions};
use ferrofed_server::state::AppState;
use http::{Request, StatusCode, header};

use crate::support::{self, OPERATOR_SCOPE, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic subject of the operator the tests mint a token for.
const OPERATOR: &str = "synthetic-config-operator-41";

/// A peer off loopback, so only the gate admits it.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The `Authorization` value of the operator [`OPERATOR`].
fn operator() -> Result<String, Box<dyn Error>> {
    let mut claims = support::claims();
    OPERATOR.clone_into(&mut claims.sub);
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}").trim().to_owned());
    Ok(format!("Bearer {}", support::issuer().mint(&claims)?))
}

/// One member `name`, at an address nothing listens on.
fn member(name: &str) -> String {
    format!(
        r#"
[[organisation]]
id = "org-{name}"

[[node]]
id = "node-{name}"
organisation = "org-{name}"
system_id = "cdr-{name}.example.org"

[[endpoint]]
id = "node-{name}-pub"
node = "node-{name}"
url = "http://127.0.0.1:9/{name}"
connection_type = "openehr-rest-query"
managing_organisation = "org-{name}"
"#
    )
}

/// The configuration less its bootstrap core over the registry `document`,
/// with `extra` appended.
fn rest(document: &Path, extra: &str) -> String {
    let document = toml::Value::String(document.display().to_string());
    format!(
        "[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 5000\n\
         overall_timeout_ms = 6000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\
         {}{}{extra}",
        support::signing_toml(),
        support::DEVELOPMENT_AUDIT_TOML
    )
}

/// The bootstrap core of a managed development gateway whose revisions are
/// kept at `store`.
fn core(store: &Path) -> Result<String, Box<dyn Error>> {
    let store = toml::Value::String(store.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[metrics]\nlisten = \"127.0.0.1:9\"\n\n\
         [config]\nui = \"managed\"\npath = {store}\n{}\n",
        support::auth_toml()?
    ))
}

/// A managed development gateway started over a configuration file in a
/// temporary directory, with its admin listener's application.
struct Gateway {
    dir: tempfile::TempDir,
    config: PathBuf,
    store: PathBuf,
    state: Arc<AppState>,
    reloader: Arc<Reloader>,
    revisions: Arc<Revisions>,
    admin: Router,
}

impl Gateway {
    /// Starts a gateway whose file federates member A alone, with `extra`
    /// appended to the part a revision holds.
    fn start(extra: &str) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("one.toml"), member("a"))?;
        std::fs::write(dir.path().join("two.toml"), member("a") + &member("b"))?;
        let store = dir.path().join("revisions.redb");
        let config = dir.path().join("ferrofed.toml");
        std::fs::write(
            &config,
            core(&store)? + &rest(&dir.path().join("one.toml"), extra),
        )?;
        let (loaded, tree) = Config::load_with_tree(Some(&config))?;
        let settings = loaded.resolve()?;
        ferrofed_server::config::revisions::check_tree(settings.revisions.as_ref(), &tree)?;
        let (settings, revisions) = service::boot(settings, &tree)?;
        let revisions = revisions.ok_or("the configuration is managed")?;
        let state = Arc::new(AppState::build(&settings)?);
        let reloader = Arc::new(
            Reloader::new(Some(config.clone()), settings, Arc::clone(&state))
                .with_revisions(Some(Arc::clone(&revisions))),
        );
        let managed = Arc::new(Managed::new(Arc::clone(&revisions), Arc::clone(&reloader)));
        let access = Access::new(&support::auth(), Profile::Development, None);
        let admin = admin::managed_router(Arc::clone(&state), access, Some(managed))
            .layer(MockConnectInfo(SocketAddr::from(REMOTE)));
        Ok(Self {
            dir,
            config,
            store,
            state,
            reloader,
            revisions,
            admin,
        })
    }

    /// The registry document of member A alone.
    fn one(&self) -> PathBuf {
        self.dir.path().join("one.toml")
    }

    /// The registry document of members A and B.
    fn two(&self) -> PathBuf {
        self.dir.path().join("two.toml")
    }

    /// How many members the running registry holds.
    fn members(&self) -> Result<usize, Box<dyn Error>> {
        Ok(self
            .state
            .federation()
            .ok_or("a registry is configured")?
            .snapshot()
            .nodes()
            .count())
    }

    /// Sends `method` of `path` with `body`, as `authorization`, and returns
    /// the status and the body text.
    async fn send(
        &self,
        (method, path): (http::Method, &str),
        body: Option<String>,
        authorization: Option<&str>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        let request = request.body(body.map_or_else(Body::empty, Body::from))?;
        let response = send_as_is(self.admin.clone(), request).await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        Ok((status, String::from_utf8(bytes.to_vec())?))
    }

    /// `GET path` as the operator.
    async fn get(&self, path: &str) -> Result<(StatusCode, String), Box<dyn Error>> {
        self.send((http::Method::GET, path), None, Some(&operator()?))
            .await
    }

    /// `POST path` with `body` as the operator.
    async fn post(
        &self,
        path: &str,
        body: Option<String>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        self.send((http::Method::POST, path), body, Some(&operator()?))
            .await
    }

    /// Saves `toml` on `base` for `reason` as the operator.
    async fn save(
        &self,
        base: Option<u64>,
        reason: &str,
        toml: &str,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        let body = SaveBody { base, reason, toml };
        self.post(
            "/admin/config/revisions",
            Some(serde_json::to_string(&body)?),
        )
        .await
    }
}

/// The body of a save.
#[derive(serde::Serialize)]
struct SaveBody<'a> {
    base: Option<u64>,
    reason: &'a str,
    toml: &'a str,
}

/// The body of `GET /admin/config`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Running {
    ui: String,
    head: Option<u64>,
    current: Option<ferrofed_server::revision::Current>,
}

/// The body of an apply or a rollback.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Applied {
    revision: ferrofed_server::revision::Revision,
    current: ferrofed_server::revision::Current,
}

/// One page of the history.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryPage {
    items: Vec<ferrofed_server::revision::Revision>,
    offset: u64,
    total: u64,
}

/// One key of a diff.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Keyed {
    key: String,
    change: String,
    from: Option<String>,
    to: Option<String>,
}

/// The body of a diff.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Diff {
    from: u64,
    to: u64,
    changes: Vec<Keyed>,
}
