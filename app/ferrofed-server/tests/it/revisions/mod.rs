// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration revisions under `config.ui = "managed"`: the store, the
//! revision API on the admin listener behind the operator scope, the file
//! load recorded as a revision, apply through the reload path, rollback,
//! optimistic concurrency, no secret value stored or returned, a revision
//! applied on one replica reaching every replica on the store, and
//! `config.ui = "off"` changing nothing. No specification governs this: our
//! own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod api;
mod console;
mod corrupt;
mod credentials;
mod exchange;
mod file_reload;
mod off;
mod ownership;
mod policy;
mod replicas;
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
use ferrofed_registry::revision::{Current, Revision, RevisionId, Signed};
use ferrofed_server::admin::{self, Access};
use ferrofed_server::config::Config;
use ferrofed_server::reload::Reloader;
use ferrofed_server::revision::embedded::RedbRevisions;
use ferrofed_server::revision::routes::Managed;
use ferrofed_server::revision::service::{self, Revisions};
use ferrofed_server::revision::{Draft, RevisionStore, Saved, StoreError, Swapped};
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
    operator_as(OPERATOR)
}

/// The `Authorization` value of the operator whose subject is `subject`.
pub(crate) fn operator_as(subject: &str) -> Result<String, Box<dyn Error>> {
    let mut claims = support::claims();
    subject.clone_into(&mut claims.sub);
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}").trim().to_owned());
    Ok(format!("Bearer {}", support::issuer().mint(&claims)?))
}

/// One member `name`, at an address nothing listens on.
pub(crate) fn member(name: &str) -> String {
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
pub(crate) fn rest(document: &Path, extra: &str) -> String {
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
/// kept at `store`, with `config` appended to its `[config]` table.
fn core(store: &Path, config: &str) -> Result<String, Box<dyn Error>> {
    let store = toml::Value::String(store.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[metrics]\nlisten = \"127.0.0.1:9\"\n\n\
         [config]\nui = \"managed\"\npath = {store}\n{config}\n{}\n",
        support::auth_toml()?
    ))
}

/// A managed development gateway started over a configuration file in a
/// temporary directory, with its admin listener's application.
pub(crate) struct Gateway {
    dir: tempfile::TempDir,
    config: PathBuf,
    store: PathBuf,
    pub(crate) state: Arc<AppState>,
    pub(crate) reloader: Arc<Reloader>,
    pub(crate) revisions: Arc<Revisions>,
    admin: Router,
}

impl Gateway {
    /// Starts a gateway whose file federates member A alone, with `extra`
    /// appended to the part a revision holds.
    fn start(extra: &str) -> Result<Self, Box<dyn Error>> {
        Self::build(extra, None)
    }

    /// Starts a gateway whose file federates member A alone, with `extra`
    /// appended, over `shared`: the store it shares with other gateways and
    /// the directory of the registry documents they all read.
    pub(crate) fn build(
        extra: &str,
        shared: Option<(Box<dyn RevisionStore>, &Path)>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::configured(("", extra), shared)
    }

    /// Starts a gateway whose file federates member A alone, with `config`
    /// appended to its `[config]` table and `extra` to the part a revision
    /// holds, over `shared` as [`Gateway::build`] takes it.
    pub(crate) fn configured(
        (config_extra, extra): (&str, &str),
        shared: Option<(Box<dyn RevisionStore>, &Path)>,
    ) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("one.toml"), member("a"))?;
        std::fs::write(dir.path().join("two.toml"), member("a") + &member("b"))?;
        let documents = shared
            .as_ref()
            .map_or(dir.path(), |(_, documents)| *documents);
        let store = dir.path().join("revisions.redb");
        let config = dir.path().join("ferrofed.toml");
        std::fs::write(
            &config,
            core(&store, config_extra)? + &rest(&documents.join("one.toml"), extra),
        )?;
        Self::boot(dir, (config, store), shared.map(|(store, _)| store))
    }

    /// Stops this gateway and starts it again over the same files and store.
    pub(crate) fn restart(self) -> Result<Self, Box<dyn Error>> {
        let Self {
            dir,
            config,
            store,
            state,
            reloader,
            revisions,
            admin,
        } = self;
        drop((admin, reloader, revisions, state));
        Self::boot(dir, (config, store), None)
    }

    /// Boots a gateway from the configuration file `config` in `dir`, its
    /// revisions at `store` or in `shared`.
    fn boot(
        dir: tempfile::TempDir,
        (config, store): (PathBuf, PathBuf),
        shared: Option<Box<dyn RevisionStore>>,
    ) -> Result<Self, Box<dyn Error>> {
        let (loaded, tree) = Config::load_with_tree(Some(&config))?;
        let settings = loaded.resolve()?;
        ferrofed_server::config::revisions::check_tree(settings.revisions.as_ref(), &tree)?;
        let (settings, booted) = match shared {
            None => {
                let (settings, booted) = service::boot(settings, &tree)?;
                (settings, booted.ok_or("the configuration is managed")?)
            }
            Some(store) => service::boot_over(store, &tree)?,
        };
        let state = Arc::new(AppState::build(&settings)?);
        booted.started()?;
        let revisions = booted.revisions;
        let reloader = Arc::new(
            Reloader::new(Some(config.clone()), settings, Arc::clone(&state))
                .with_revisions(Some(Arc::clone(&revisions))),
        );
        let managed = Arc::new(Managed::new(Arc::clone(&revisions), Arc::clone(&reloader)));
        let access = Access::new(&support::auth(), Profile::Development, Vec::new());
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
    pub(crate) fn members(&self) -> Result<usize, Box<dyn Error>> {
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
    pub(crate) async fn get(&self, path: &str) -> Result<(StatusCode, String), Box<dyn Error>> {
        self.send((http::Method::GET, path), None, Some(&operator()?))
            .await
    }

    /// `POST path` with `body` as the operator.
    pub(crate) async fn post(
        &self,
        path: &str,
        body: Option<String>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        self.send((http::Method::POST, path), body, Some(&operator()?))
            .await
    }

    /// Saves `toml` on `base` for `reason` as the operator.
    pub(crate) async fn save(
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
pub(crate) struct Standing {
    ui: String,
    head: Option<u64>,
    current: Option<Current>,
    pub(crate) running: Option<u64>,
    pub(crate) in_step: bool,
    pub(crate) refused: Option<u64>,
    #[serde(default)]
    pub(crate) review: Option<ferrofed_registry::revision::Review>,
}

/// The body of an apply or a rollback.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Applied {
    revision: Revision,
    current: Current,
}

/// One page of the history.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryPage {
    items: Vec<Revision>,
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

/// A revision store several gateways in one process share, as replicas
/// share one PostgreSQL database; a `redb` file opens in one place at a time.
#[derive(Debug, Clone)]
struct Shared(Arc<RedbRevisions>);

impl RevisionStore for Shared {
    fn save(&self, base: Option<RevisionId>, draft: Draft) -> Result<Saved, StoreError> {
        self.0.save(base, draft)
    }

    fn revision(&self, id: RevisionId) -> Result<Option<Revision>, StoreError> {
        self.0.revision(id)
    }

    fn history(&self) -> Result<Vec<Revision>, StoreError> {
        self.0.history()
    }

    fn head(&self) -> Result<Option<RevisionId>, StoreError> {
        self.0.head()
    }

    fn current(&self) -> Result<Option<Current>, StoreError> {
        self.0.current()
    }

    fn swap_current(
        &self,
        expected: Option<RevisionId>,
        next: &Current,
    ) -> Result<Swapped, StoreError> {
        self.0.swap_current(expected, next)
    }

    fn approvals(&self, id: RevisionId) -> Result<Vec<Signed>, StoreError> {
        self.0.approvals(id)
    }

    fn approve(&self, id: RevisionId, approval: &Signed) -> Result<(), StoreError> {
        self.0.approve(id, approval)
    }
}

/// Two gateways started from the same configuration over one shared store,
/// as two replicas of one deployment.
pub(crate) struct Replicas {
    /// The registry documents both read, and the shared store's file.
    documents: tempfile::TempDir,
    /// The replica that records the file as revision 1.
    one: Gateway,
    /// The replica that starts on revision 1, its file being the same.
    two: Gateway,
    /// The store both share, for a third start over it.
    shared: Shared,
}

impl Replicas {
    /// Starts both replicas over one store.
    fn start() -> Result<Self, Box<dyn Error>> {
        let documents = tempfile::tempdir()?;
        std::fs::write(documents.path().join("one.toml"), member("a"))?;
        std::fs::write(
            documents.path().join("two.toml"),
            member("a") + &member("b"),
        )?;
        let shared = Shared(Arc::new(RedbRevisions::open(
            &documents.path().join("shared.redb"),
        )?));
        let one = Gateway::build("", Some((Box::new(shared.clone()), documents.path())))?;
        let two = Gateway::build("", Some((Box::new(shared.clone()), documents.path())))?;
        Ok(Self {
            documents,
            one,
            two,
            shared,
        })
    }

    /// The registry document of members A and B both replicas read.
    fn two_members(&self) -> PathBuf {
        self.documents.path().join("two.toml")
    }
}
