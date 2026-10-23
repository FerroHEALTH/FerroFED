// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stack a journey runs against: two stub nodes, a running gateway over
//! them, the test OpenID Provider, and a running console that serves its
//! release site bundle, each on loopback.
//!
//! Node A answers every query and node B fails it, so one run shows both an
//! answering and a failing node; the gateway offers best effort, so the same
//! query can come back complete, incomplete or failed. The registry holds
//! more `creating_system_id` routes and the gateway more stored queries than
//! one page of a view shows, so both pagers have a next page.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::definition::store::{DefinitionStore as _, Insertion};
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};
use ferrofed_server::state::AppState;
use ferrofed_server::stored::embedded::RedbStore;
use ferrofed_testkit::mock::Server;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::console::{AUDIENCE, ISSUER, OPERATOR_SCOPE};
use crate::journeys::provider::Provider;

/// The environment variable naming the site bundle's directory, when it is
/// not the workspace's `target/site`.
const SITE_ENV: &str = "FERROFED_JOURNEYS_SITE";

/// How many `creating_system_id` routes the registry registers.
const REGISTERED: u64 = 99;

/// How many rows the routing table holds: the registered routes and each
/// member's own `system_id`, one more than a page of the integrity view.
pub(crate) const ROUTES: u64 = REGISTERED + 2;

/// How many stored queries the deployment's store holds.
pub(crate) const STORED: u64 = 101;

/// The stored query the deployment holds at a second version,
/// [`SECOND_VERSION`], beside its first, `1.0.0`.
pub(crate) const VERSIONED: &str = "org.example::journey_001";

/// The second version of [`VERSIONED`].
pub(crate) const SECOND_VERSION: &str = "1.0.1";

/// The text of [`VERSIONED`] at [`SECOND_VERSION`], which names no patient.
pub(crate) const SECOND_TEXT: &str = "SELECT c/name/value FROM EHR e CONTAINS COMPOSITION c";

/// How many stored-query versions the gateway holds: the deployment's, with
/// the second version of [`VERSIONED`], and its own read-only section
/// queries, more than a page of the stored-query view.
pub(crate) fn held() -> Result<u64, Box<dyn Error>> {
    let own = u64::try_from(ferrofed_eehrxf::patient_summary::Section::ALL.len())?;
    Ok(STORED + 1 + own)
}

/// The synthetic patient the query journeys name, visibly synthetic, under
/// the example OID arc.
pub(crate) const PATIENT: &str = "SENTINEL-PATIENT-38kq";

/// The issuing namespace of [`PATIENT`].
pub(crate) const NAMESPACE: &str = "urn:oid:2.999.1";

/// The patient's `ehr_id` at node A and at node B.
const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const EHR_B: &str = "1111bbbb-1111-4111-8111-111111111111";

/// The row node A answers with.
pub(crate) const UID_AT_A: &str = "uid-at-a";

/// The query that names [`PATIENT`] through the parameter `$patient`.
pub(crate) fn patient_query() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The running stack.
#[derive(Debug)]
pub(crate) struct Stack {
    /// The node that answers, kept so it outlives the gateway.
    _answering: Server,
    /// The node that fails, kept so it outlives the gateway.
    _failing: Server,
    /// The registry, the signing key and the stored queries.
    _dir: tempfile::TempDir,
    /// The test OpenID Provider.
    provider: Provider,
    /// The console's base URL.
    console: String,
    /// The name of the console's session cookie.
    cookie: String,
}

impl Stack {
    /// Starts the nodes, the provider, the gateway and the console.
    pub(crate) async fn start() -> Result<Self, Box<dyn Error>> {
        Self::start_with(None).await
    }

    /// Starts the stack with the console configured with one replica,
    /// `gateway-0`, whose admin listener is at `admin`.
    pub(crate) async fn start_with_admin(admin: &str) -> Result<Self, Box<dyn Error>> {
        Self::start_with(Some(admin)).await
    }

    /// Starts the stack, the console naming the admin listener `admin` when
    /// one is given.
    async fn start_with(admin: Option<&str>) -> Result<Self, Box<dyn Error>> {
        let site = site_root()?;
        let answering = node(200, &format!(
            r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["{UID_AT_A}"]]}}"##
        ))
        .await;
        let failing = node(500, r#"{"message":"synthetic node failure"}"#).await;
        let dir = tempfile::tempdir()?;
        let provider = Provider::start().await?;
        let gateway = gateway(dir.path(), (&answering.uri(), &failing.uri()), &provider).await?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let console = format!("http://{}", listener.local_addr()?);
        let site = toml::Value::String(site.display().to_string());
        let text = format!(
            "[server]\nsite_root = {site}\n\n[gateway]\nbase_url = \"{gateway}/\"\n\n\
             [session]\nsecure_cookie = false\n\n{}",
            provider.console_configuration(&console)
        );
        let text = match admin {
            Some(admin) => format!(
                "{text}\n[[admin.replicas]]\nname = \"gateway-0\"\nadmin_url = \"{admin}\"\n"
            ),
            None => text,
        };
        let settings =
            ferrofed_console::config::Config::from_sources(Some(&text), &BTreeMap::new())?
                .resolve()?;
        let state = ferrofed_console::server::ConsoleState::new(settings)?;
        let cookie = state
            .sessions()
            .cookie_name(ferrofed_console::session::COOKIE);
        tokio::spawn(ferrofed_server::serve_until(
            listener,
            ferrofed_console::server::router(state),
            Duration::from_secs(1),
            std::future::pending(),
        ));
        Ok(Self {
            _answering: answering,
            _failing: failing,
            _dir: dir,
            provider,
            console,
            cookie,
        })
    }

    /// The URL of `path` on the console.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.console)
    }

    /// The test OpenID Provider.
    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    /// The name of the console's session cookie.
    pub(crate) fn cookie(&self) -> &str {
        &self.cookie
    }
}

/// The site bundle's directory: [`SITE_ENV`], or the workspace's
/// `target/site`, which must hold the WebAssembly cargo-leptos wrote.
fn site_root() -> Result<PathBuf, Box<dyn Error>> {
    let site = std::env::var_os(SITE_ENV).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/site"),
        PathBuf::from,
    );
    let bundle = site.join("pkg/ferrofed-console.wasm");
    if !bundle.is_file() {
        return Err(format!(
            "{} holds no site bundle: build it with scripts/release/console-site.sh --release, or name its directory in {SITE_ENV}",
            site.display()
        )
        .into());
    }
    Ok(site)
}

/// A stub node answering every `POST /openehr/v1/query/aql` with `status`
/// and `body`.
async fn node(status: u16, body: &str) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_raw(body.as_bytes().to_vec(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The registry of node A at `a` and node B at `b`, with [`REGISTERED`]
/// `creating_system_id` routes to node A.
fn registry(a: &str, b: &str) -> String {
    let mut text = String::new();
    for (member, url) in [("a", a), ("b", b)] {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "[[organisation]]\nid = \"org-{member}\"\n\n\
             [[node]]\nid = \"node-{member}\"\norganisation = \"org-{member}\"\n\
             system_id = \"cdr-{member}.example.org\"\n\n\
             [[endpoint]]\nid = \"node-{member}-pub\"\nnode = \"node-{member}\"\n\
             url = \"{url}/openehr\"\nconnection_type = \"openehr-rest-query\"\n\
             managing_organisation = \"org-{member}\"\n\n"
        );
    }
    for route in 1..=REGISTERED {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "[[creating_system]]\ncreating_system_id = \"legacy-{route:03}.example.org\"\n\
             endpoint = \"node-a-pub\"\n\n"
        );
    }
    text
}

/// Writes [`STORED`] stored queries into the embedded store file
/// `definitions`, one version each, and the second version of
/// [`VERSIONED`].
///
/// The store takes writes, so the gateway marks only its own namespace
/// read-only.
fn stored_queries(definitions: &Path) -> Result<(), Box<dyn Error>> {
    let store = RedbStore::open(definitions)?;
    let saved: jiff::Timestamp = "2026-10-01T08:00:00Z".parse()?;
    let first: QueryVersion = "1.0.0".parse()?;
    let mut held = Vec::new();
    for number in 1..=STORED {
        let name = QueryName::new(&format!("org.example::journey_{number:03}"))?;
        held.push(StoredDefinition::new(name, first, patient_query(), saved));
    }
    held.push(StoredDefinition::new(
        QueryName::new(VERSIONED)?,
        SECOND_VERSION.parse()?,
        SECOND_TEXT.to_owned(),
        saved,
    ));
    for definition in &held {
        if store.insert_if_absent(definition)? != Insertion::Stored {
            return Err(format!("{} is stored twice", definition.name()).into());
        }
    }
    Ok(())
}

/// Starts the gateway over the nodes at `a` and `b`, trusting the operator's
/// token `provider` issues, and returns its base URL.
async fn gateway(
    dir: &Path,
    (a, b): (&str, &str),
    provider: &Provider,
) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b))?;
    let key_file = dir.join("signing-key.pem");
    std::fs::write(&key_file, ferrofed_testkit::oauth::es384_pem()?)?;
    let definitions = dir.join("stored.redb");
    stored_queries(&definitions)?;
    let jwks = dir.join("issuer-jwks.json");
    std::fs::write(&jwks, provider.jwks())?;
    let [document, key_file, definitions, jwks] = [document, key_file, definitions, jwks]
        .map(|file| toml::Value::String(file.display().to_string()));
    let mut text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         best_effort = true\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\n\
         [stored_queries]\nbackend = \"redb\"\npath = {definitions}\n\n\
         [signing]\nkey_file = {key_file}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n\n\
         [auth]\nissuer = \"{ISSUER}\"\naudience = [\"{AUDIENCE}\"]\njwks_file = {jwks}\n\
         product_scope = \"{OPERATOR_SCOPE}\"\n"
    );
    for (member, ehr_id) in [("node-a", EHR_A), ("node-b", EHR_B)] {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\n\
             member = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        );
    }
    let settings =
        ferrofed_server::config::Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&settings)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(ferrofed_server::serve_until(
        listener,
        ferrofed_server::router(Arc::new(state), &settings.server),
        Duration::from_secs(1),
        std::future::pending(),
    ));
    Ok(format!("http://{address}"))
}
