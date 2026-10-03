// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reloading the registry while the gateway serves, driven through
//! [`Reloader::reload`], the function the `SIGHUP` handler calls: a valid
//! document replaces the running registry for the requests that start after
//! it, a request in flight finishes on the registry it took, learned state is
//! held to the new document, and a document that does not load is refused
//! with the running registry kept. No specification governs reloading: our
//! own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use axum::Router;
use ferrofed_identity::binding::{Bound, SessionKey};
use ferrofed_registry::creating_system::{CreatingSystemRoute, Sighting};
use ferrofed_registry::ehr_index::Indexed;
use ferrofed_registry::error::CreatingSystemMiss;
use ferrofed_registry::id::{EhrId, EndpointId, NodeId, SystemId};
use ferrofed_registry::incident::Incident;
use ferrofed_server::config::Config;
use ferrofed_server::reload::{Applied, ReloadError, Reloader};
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::StatusCode;
use openehr_base::prelude::ObjectVersionId;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::facade::{
    EHR_A, EHR_B, body, crossref, node_answering, patient_query, post, settings_with_room,
};
use crate::support::{Logs, call};

type TestResult = Result<(), Box<dyn Error>>;

/// The `ehr_id` of the patient at node C.
const EHR_C: &str = "3333cccc-3333-4333-8333-333333333333";

/// A synthetic `creating_system_id` no member carries as its own.
const LEGACY: &str = "legacy-x.example.org";

/// A synthetic bearer token for node C, which no log line may carry.
const TOKEN_C: &str = "synthetic-reload-token-c";

/// One member `name` whose endpoint is at `url`: `org-<name>`,
/// `node-<name>` with `system_id` `cdr-<name>.example.org`, and
/// `node-<name>-pub`.
fn member(name: &str, url: &str) -> String {
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
url = "{url}"
connection_type = "openehr-rest-query"
managing_organisation = "org-{name}"
"#
    )
}

/// A gateway serving over a configuration file and a registry document in a
/// temporary directory, with the reloader its `SIGHUP` handler would call.
struct Gateway {
    _dir: tempfile::TempDir,
    config: PathBuf,
    document: PathBuf,
    state: Arc<AppState>,
    reloader: Reloader,
    app: Router,
}

impl Gateway {
    /// Starts a development gateway over `registry`, with `top` as its
    /// top-level keys and `tables` appended.
    fn start(registry: &str, top: &str, tables: &str) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let config = dir.path().join("ferrofed.toml");
        let document = dir.path().join("registry.toml");
        std::fs::write(&document, registry)?;
        std::fs::write(&config, configuration(&document, top, tables))?;
        let settings = Config::load(Some(&config))?.resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        let app = ferrofed_server::router(Arc::clone(&state), &settings_with_room());
        let reloader = Reloader::new(Some(config.clone()), settings, Arc::clone(&state));
        Ok(Self {
            _dir: dir,
            config,
            document,
            state,
            reloader,
            app,
        })
    }

    /// Rewrites the registry document.
    fn write_registry(&self, registry: &str) -> std::io::Result<()> {
        std::fs::write(&self.document, registry)
    }

    /// Rewrites the configuration file.
    fn write_config(&self, top: &str, tables: &str) -> std::io::Result<()> {
        std::fs::write(&self.config, configuration(&self.document, top, tables))
    }

    /// The running federation.
    fn federation(&self) -> Result<Arc<ferrofed_server::federation::Federation>, &'static str> {
        self.state.federation().ok_or("a registry is configured")
    }

    /// Asks the patient query and returns the status and the answer text.
    async fn ask(&self) -> Result<(StatusCode, String), Box<dyn Error>> {
        call(self.app.clone(), post(body(&patient_query())?)?).await
    }
}

/// The configuration text over `document`, a development profile with
/// `top`, and `tables` appended.
fn configuration(document: &Path, top: &str, tables: &str) -> String {
    let document = toml::Value::String(document.display().to_string());
    format!(
        "profile = \"development\"\n{top}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 5000\noverall_timeout_ms = 6000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{tables}"
    )
}

fn ids<T: std::str::FromStr>(values: &[&str]) -> Result<Vec<T>, T::Err> {
    values.iter().map(|value| value.parse()).collect()
}

/// How many requests `server` received.
async fn hits(server: &MockServer) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

#[tokio::test]
async fn a_reload_adds_removes_and_changes_members() -> TestResult {
    let a1 = node_answering("uid-a1::cdr-a.example.org::1").await;
    let a2 = node_answering("uid-a2::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let c = node_answering("uid-c::cdr-c.example.org::1").await;
    let gateway = Gateway::start(
        &(member("a", &a1.uri()) + &member("b", &b.uri())),
        "",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
    )?;
    let (status, before) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{before}");

    gateway.write_registry(&(member("a", &a2.uri()) + &member("c", &c.uri())))?;
    gateway.write_config("", &crossref(&[("node-a", EHR_A), ("node-c", EHR_C)]))?;
    let applied = gateway.reloader.reload()?;

    assert_eq!(2, applied.members);
    assert_eq!(ids::<EndpointId>(&["node-c-pub"])?, applied.endpoints_added);
    assert_eq!(
        ids::<EndpointId>(&["node-b-pub"])?,
        applied.endpoints_removed
    );
    assert_eq!(ids::<NodeId>(&["node-b"])?, applied.members_removed);
    assert!(applied.needs_restart.is_empty(), "{applied:?}");
    let federation = gateway.federation()?;
    let snapshot = federation.snapshot();
    assert!(snapshot.node(&"node-b".parse()?).is_none());
    let moved = snapshot
        .endpoint(&"node-a-pub".parse()?)
        .ok_or("node-a-pub stays")?;
    assert_eq!(a2.uri(), moved.url().as_str().trim_end_matches('/'));

    let (status, after) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{after}");
    assert!(
        after.contains("uid-a2::") && after.contains("uid-c::"),
        "{after}"
    );
    assert!(
        !after.contains("uid-a1::") && !after.contains("uid-b::"),
        "{after}"
    );
    assert_eq!(
        (1, 1, 1, 1),
        (
            hits(&a1).await?,
            hits(&b).await?,
            hits(&a2).await?,
            hits(&c).await?
        ),
        "the moved and the removed endpoint are never called again"
    );
    Ok(())
}

#[tokio::test]
async fn credentials_follow_the_reloaded_registry() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let c = node_answering("uid-c::cdr-c.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    gateway.write_registry(&(member("a", &a.uri()) + &member("c", &c.uri())))?;
    let credentials = format!("[credentials.\"node-c-pub\"]\nbearer_token = \"{TOKEN_C}\"\n");
    gateway.write_config("", &(crossref(&[("node-c", EHR_C)]) + "\n" + &credentials))?;
    gateway.reloader.reload()?;

    let (status, text) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let requests = c.received_requests().await.ok_or("recording is on")?;
    let sent = requests.first().ok_or("node C was asked")?;
    let authorization = sent
        .headers
        .get(http::header::AUTHORIZATION)
        .ok_or("node C's client sends its credentials")?;
    assert_eq!(format!("Bearer {TOKEN_C}"), authorization.to_str()?);
    Ok(())
}

#[tokio::test]
async fn credentials_for_an_endpoint_the_document_dropped_refuse_the_reload() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let credentials = format!("[credentials.\"node-b-pub\"]\nbearer_token = \"{TOKEN_C}\"\n");
    let tables = crossref(&[("node-a", EHR_A)]) + "\n" + &credentials;
    let gateway = Gateway::start(
        &(member("a", &a.uri()) + &member("b", &b.uri())),
        "",
        &tables,
    )?;
    let running = gateway.federation()?;
    gateway.write_registry(&member("a", &a.uri()))?;

    let refused = gateway
        .reloader
        .reload()
        .err()
        .ok_or("credentials naming no endpoint are refused, as at boot")?;
    assert_eq!("credentials", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_reload_that_contradicts_a_learned_route_withdraws_it_with_its_incident() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let members = member("a", &a.uri()) + &member("b", &b.uri());
    let gateway = Gateway::start(&members, "", &crossref(&[("node-a", EHR_A)]))?;
    let legacy: SystemId = LEGACY.parse()?;
    let version =
        ObjectVersionId::new(format!("8849182c-82ad-4088-a07f-48ead4180515::{LEGACY}::1"))?;
    let running = gateway.federation()?;
    let sighting =
        running
            .learned()
            .observe(running.snapshot(), &version, &"node-a-pub".parse()?)?;
    assert!(matches!(sighting, Sighting::Learned(_)), "{sighting:?}");

    let mapped = format!(
        "{members}\n[[creating_system]]\ncreating_system_id = \"{LEGACY}\"\nendpoint = \"node-b-pub\"\n"
    );
    gateway.write_registry(&mapped)?;
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());
    let applied = reloaded?;

    assert_eq!(
        vec![Incident::RegisteredCreatingSystemConflict {
            creating_system_id: legacy.clone(),
            registered: "node-b".parse()?,
            learned: "node-a-pub".parse()?,
        }],
        applied.reconciled.incidents
    );
    let text = logs.text();
    assert_eq!(
        1,
        text.matches("\"RegisteredCreatingSystemConflict\"").count(),
        "the incident is emitted once: {text}"
    );
    let federation = gateway.federation()?;
    let route = federation.learned().route(federation.snapshot(), &legacy)?;
    assert!(
        matches!(&route, CreatingSystemRoute::Registered { .. }),
        "the document routes it now: {route:?}"
    );

    gateway.write_registry(&members)?;
    gateway.reloader.reload()?;
    let federation = gateway.federation()?;
    let route = federation.learned().route(federation.snapshot(), &legacy);
    assert_eq!(
        Err(CreatingSystemMiss::Conflicted(legacy)),
        route,
        "the learned route stays withdrawn when the mapping goes again"
    );
    Ok(())
}

#[tokio::test]
async fn an_invalid_document_is_refused_and_the_running_registry_stays() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    let sentinel = "SENTINEL-DOCUMENT-CONTENT-7xq";
    gateway.write_registry(&format!(
        "{}\n[[node]]\nid = \"{sentinel}\"\n",
        member("a", &a.uri())
    ))?;

    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());

    let Err(refused) = reloaded else {
        return Err("a document the boot check refuses is refused".into());
    };
    assert!(
        matches!(refused, ReloadError::Federation { .. }),
        "{refused:?}"
    );
    assert_eq!("registry-invalid", refused.class());
    assert!(
        Arc::ptr_eq(&running, &gateway.federation()?),
        "the running registry stays"
    );
    let text = logs.text();
    assert!(text.contains("registry-invalid"), "{text}");
    assert!(
        text.contains(&gateway.document.display().to_string()),
        "the refusal names the document: {text}"
    );
    assert!(
        !text.contains(sentinel),
        "the refusal never quotes the document: {text}"
    );
    let (status, answer) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{answer}");
    Ok(())
}

#[tokio::test]
async fn an_unreadable_document_is_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    std::fs::remove_file(&gateway.document)?;

    let refused = gateway.reloader.reload().err().ok_or("refused")?;
    assert_eq!("registry-unreadable", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_configuration_that_does_not_load_is_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    let credentials =
        format!("[credentials.\"node-a-pub\"]\nbearer_token = \"{TOKEN_C}\"\nuser = \"both\"\n");

    let logs = Logs::default();
    gateway.write_config("", &(crossref(&[("node-a", EHR_A)]) + "\n" + &credentials))?;
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());

    let refused = reloaded
        .err()
        .ok_or("two schemes are refused, as at boot")?;
    assert_eq!("configuration", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    let text = logs.text();
    assert!(!text.contains(TOKEN_C), "no credential is logged: {text}");
    assert!(
        text.contains(&gateway.config.display().to_string()),
        "the refusal names the configuration file: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn setting_or_unsetting_the_registry_takes_a_restart() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    std::fs::write(&gateway.config, "profile = \"development\"\n")?;

    let refused = gateway.reloader.reload().err().ok_or("refused")?;
    assert!(
        matches!(refused, ReloadError::RegistryPresence),
        "{refused:?}"
    );
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_changed_setting_outside_the_registry_is_reported_and_the_rest_applies() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    gateway.write_registry(&(member("a", &a.uri()) + &member("b", &b.uri())))?;
    gateway.write_config(
        "[server]\nlisten = \"127.0.0.1:18080\"",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let applied: Applied = gateway.reloader.reload()?;
    assert_eq!(vec!["server.listen"], applied.needs_restart);
    assert_eq!(ids::<EndpointId>(&["node-b-pub"])?, applied.endpoints_added);
    Ok(())
}

#[tokio::test]
async fn a_departed_member_leaves_the_index_and_the_bindings() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let tables = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(
        &(member("a", &a.uri()) + &member("b", &b.uri())),
        "",
        &tables,
    )?;
    let (ehr_a, ehr_b): (EhrId, EhrId) = (EHR_A.parse()?, EHR_B.parse()?);
    let (node_a, node_b): (NodeId, NodeId) = ("node-a".parse()?, "node-b".parse()?);
    let session = SessionKey::new("session-reload");
    let now = Instant::now();
    let running = gateway.federation()?;
    running.index().learn(&ehr_a, &node_a);
    running.index().learn(&ehr_b, &node_b);
    running
        .bindings()
        .record(&session, now, [(&node_a, &ehr_a), (&node_b, &ehr_b)]);

    gateway.write_registry(&member("a", &a.uri()))?;
    let applied = gateway.reloader.reload()?;

    assert_eq!(
        (1, 1),
        (
            applied.reconciled.index_dropped,
            applied.reconciled.bindings_dropped
        )
    );
    let federation = gateway.federation()?;
    assert_eq!(Indexed::None, federation.index().lookup(&ehr_b));
    assert_eq!(
        Indexed::One(node_a.clone()),
        federation.index().lookup(&ehr_a)
    );
    assert_eq!(
        Bound::None,
        federation.bindings().lookup(&session, now, &ehr_b)
    );
    assert_eq!(
        Bound::One(node_a),
        federation.bindings().lookup(&session, now, &ehr_a)
    );
    Ok(())
}

/// A node that holds every request until the test releases it, and says
/// when one arrived.
struct Held {
    arrived: tokio::sync::mpsc::UnboundedSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    answer: String,
}

impl Respond for Held {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        // NOTE: a closed channel means the test already failed, so the send
        // result has nothing to report.
        let _sent: Result<(), _> = self.arrived.send(());
        let released = self
            .release
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(Duration::from_secs(10));
        match released {
            Ok(()) => ResponseTemplate::new(200)
                .set_body_raw(self.answer.clone().into_bytes(), "application/json"),
            Err(_) => ResponseTemplate::new(503),
        }
    }
}

#[tokio::test]
async fn a_request_in_flight_finishes_on_the_registry_it_started_with() -> TestResult {
    let (arrived, mut arrival) = tokio::sync::mpsc::unbounded_channel();
    let (release, held) = mpsc::channel();
    let old = MockServer::start().await;
    let answer = String::from(
        r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-old::cdr-a.example.org::1"]]}"##,
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(Held {
            arrived,
            release: Mutex::new(held),
            answer,
        })
        .mount(&old)
        .await;
    let new = node_answering("uid-new::cdr-a.example.org::1").await;
    let gateway = Gateway::start(
        &member("a", &old.uri()),
        "",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let first = gateway.ask();
    let then = async {
        tokio::time::timeout(Duration::from_secs(5), arrival.recv())
            .await?
            .ok_or("the first request reached the old node")?;
        gateway.write_registry(&member("a", &new.uri()))?;
        gateway.reloader.reload()?;
        let second = gateway.ask().await?;
        release.send(())?;
        Ok::<_, Box<dyn Error>>(second)
    };
    let (first, second) = tokio::join!(first, then);
    let ((first_status, first), (second_status, second)) = (first?, second?);

    assert_eq!(StatusCode::OK, first_status, "{first}");
    assert!(
        first.contains("uid-old::") && !first.contains("uid-new::"),
        "the request in flight finished on its own registry: {first}"
    );
    assert_eq!(StatusCode::OK, second_status, "{second}");
    assert!(
        second.contains("uid-new::") && !second.contains("uid-old::"),
        "a request after the reload sees the new registry: {second}"
    );
    assert_eq!(1, hits(&old).await?, "the old address got only the first");
    Ok(())
}
