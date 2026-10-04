// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer's audit messages sent to an ATNA Audit Record
//! Repository, `[xcpd] audit = "repository"` (ITI TF-2 §3.20, §3.55.5.1.1):
//! each discovery's message reaches the testkit's harness repository over
//! TLS; a repository that is down holds the messages in the spool and shows
//! on the health report and the metrics, and a full spool fails the query
//! closed (§14.1). The configuration refuses plain TCP and a spool in memory
//! outside development.
//!
//! In process: three mock CDRs, the development cross-reference resolving
//! the patient at every one, the testkit's stub responding gateway, and the
//! harness repository.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error;
use ferrofed_server::federation::{Federation, FederationError};
use ferrofed_server::localization::LocalizationError;
use ferrofed_server::metrics::Metrics;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna::AuditRepository;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::xcpd::{Answer, Community, RespondingGateway};
use http::{Request, StatusCode};
use openehr_federation::options::OptionsRoot;
use serde::Deserialize;

use crate::facade::{
    Answer as Federated, NAMESPACE, PATIENT, body, node_answering, patient_query, post, received,
    settings_with_room,
};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

const MEMBERS: [&str; 3] = ["node-a", "node-b", "node-c"];

const EHR_IDS: [&str; 3] = [
    "7a7a7a7a-7a7a-4a7a-8a7a-7a7a7a7a7a7a",
    "7b7b7b7b-7b7b-4b7b-8b7b-7b7b7b7b7b7b",
    "7c7c7c7c-7c7c-4c7c-8c7c-7c7c7c7c7c7c",
];

const COMMUNITIES: [&str; 3] = ["2.999.50", "2.999.60", "2.999.70"];

/// The configuration text of a gateway under `profile` over the members at
/// `urls`, localized by `[xcpd]` at `gateway`, with the
/// `[xcpd.audit_repository]` keys `repository`.
fn config(
    dir: &Path,
    profile: &str,
    urls: [&str; 3],
    gateway: &str,
    repository: &str,
) -> Result<String, Box<dyn Error>> {
    let mut registry = String::new();
    let mut rows = String::new();
    let mut communities = String::new();
    for ((member, url), (ehr_id, community)) in MEMBERS
        .into_iter()
        .zip(urls)
        .zip(EHR_IDS.into_iter().zip(COMMUNITIES))
    {
        write!(
            registry,
            "\n[[organisation]]\nid = \"org-{member}\"\n\n[[node]]\nid = \"{member}\"\norganisation = \"org-{member}\"\nsystem_id = \"{member}.example.org\"\n\n[[endpoint]]\nid = \"{member}-pub\"\nnode = \"{member}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-{member}\"\n"
        )?;
        if profile == "development" {
            write!(
                rows,
                "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
            )?;
        }
        writeln!(communities, "\"{community}\" = \"{member}\"")?;
    }
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    Ok(format!(
        "profile = \"{profile}\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"localized\"\nid = \"example-federation\"\n\n[federation.localization]\ntimeout_ms = 1000\n\n[xcpd]\nsender_device = \"2.999.40.1\"\nhome_community = \"2.999.40\"\naudit = \"repository\"\n\n[[xcpd.gateway]]\nurl = \"{gateway}\"\ndevice = \"2.999.50.1\"\n\n[xcpd.communities]\n{communities}\n[xcpd.audit_repository]\nhostname = \"gateway.example.org\"\n{repository}\n{rows}",
        document = toml::Value::String(document.display().to_string()),
    ))
}

/// The `[xcpd.audit_repository]` keys that reach `repository` over TLS,
/// trusting its CA, written beside it in `dir`, with `extra` keys.
fn reaching(
    dir: &Path,
    repository: &AuditRepository,
    extra: &str,
) -> Result<String, Box<dyn Error>> {
    let roots = dir.join("atna-roots.pem");
    std::fs::write(&roots, repository.trust_roots())?;
    Ok(format!(
        "url = \"{}\"\ntrust_roots_file = {}\nconnect_timeout_ms = 500\n{extra}",
        repository.url(),
        toml::Value::String(roots.display().to_string())
    ))
}

/// The router and state of the federation `text` loads.
fn gateway(text: &str) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let settings =
        Config::from_sources(Some(&crate::support::signed(text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let state = Arc::new(AppState::with_federation(federation));
    Ok((
        ferrofed_server::router(Arc::clone(&state), &settings_with_room()),
        state,
    ))
}

/// The federation `text` loads, or why it does not.
fn load(text: &str) -> Result<Result<Option<Federation>, FederationError>, Box<dyn Error>> {
    let settings =
        Config::from_sources(Some(&crate::support::signed(text)), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings))
}

async fn members() -> [Server; 3] {
    [
        node_answering("a-uid::node-a.example.org::1").await,
        node_answering("b-uid::node-b.example.org::1").await,
        node_answering("c-uid::node-c.example.org::1").await,
    ]
}

fn urls(servers: &[Server; 3]) -> [String; 3] {
    [servers[0].uri(), servers[1].uri(), servers[2].uri()]
}

async fn holding() -> RespondingGateway {
    RespondingGateway::answering(Answer::Holds(vec![Community::new(
        COMMUNITIES[0],
        "2.999.50.2",
        "PID-SYNTH-A",
    )]))
    .await
}

/// The state `GET /health/dependencies` reports of the audit repository.
async fn repository_state(app: &Router) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        audit_repository: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str::<Report>(&text)?.audit_repository)
}

/// Waits up to five seconds until `GET /health/dependencies` reports the
/// audit repository `expected`, and returns the last state it reported.
async fn await_state(app: &Router, expected: &str) -> Result<Option<String>, Box<dyn Error>> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let state = repository_state(app).await?;
        if state.as_deref() == Some(expected) || Instant::now() >= until {
            return Ok(state);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn each_discovery_reaches_the_audit_repository_without_the_identifier_in_any_log()
-> TestResult {
    let servers = members().await;
    let [a, b, c] = urls(&servers);
    let stub = holding().await;
    let repository = AuditRepository::start().await?;
    let dir = tempfile::tempdir()?;
    let keys = reaching(dir.path(), &repository, "")?;
    let (app, _state) = gateway(&config(
        dir.path(),
        "development",
        [&a, &b, &c],
        &stub.endpoint(),
        &keys,
    )?)?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    let messages = repository.wait_for(1, Duration::from_secs(5)).await;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, messages.len(), "one ITI-20 message per exchange");
    assert!(messages[0].starts_with("<85>1 "), "{}", messages[0]);
    assert!(
        messages[0].contains("<EventTypeCode csd-code=\"ITI-55\""),
        "{}",
        messages[0]
    );
    // NOTE: dXJuOm9pZDoyLjk5OS40MA== is the base64 of urn:oid:2.999.40, the
    // configured home_community (ITI TF-2 §3.55.5.1.1, PS3.15 A.5.1 ValuePair).
    assert!(
        messages[0].contains(
            "<ParticipantObjectDetail type=\"ihe:homeCommunityID\" value=\"dXJuOm9pZDoyLjk5OS40MA==\"/>"
        ),
        "the configured homeCommunityID: {}",
        messages[0]
    );
    assert!(
        !messages[0].contains(PATIENT),
        "the identifier is only base64 inside the query parameters"
    );
    assert_eq!(Some("up"), await_state(&app, "up").await?.as_deref());

    let request = Request::options("/").body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status);
    let options: OptionsRoot = serde_json::from_str(&text)?;
    assert_eq!(
        Some("\"repository\""),
        options
            .federation
            .localization
            .extra
            .get("audit")
            .map(serde_json::value::RawValue::get)
    );

    let exposition = Metrics::default().render()?;
    let samples = parse(&exposition)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, "ferrofed_audit_delivered_total", &[])
    );
    assert_eq!(
        Some("0".to_owned()),
        count(&samples, "ferrofed_audit_spool_events", &[])
    );
    assert!(!exposition.contains(PATIENT));
    let log = logs.text();
    assert!(!log.contains(PATIENT), "no identifier in the log: {log}");
    Ok(())
}

#[tokio::test]
async fn a_repository_that_is_down_holds_the_messages_and_shows_it() -> TestResult {
    let servers = members().await;
    let [a, b, c] = urls(&servers);
    let stub = holding().await;
    let repository = AuditRepository::start().await?;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let spool = dir.path().join("spool");
    let keys = reaching(
        dir.path(),
        &repository,
        &format!(
            "spool_dir = {}",
            toml::Value::String(spool.display().to_string())
        ),
    )?;
    let (app, _state) = gateway(&config(
        dir.path(),
        "development",
        [&a, &b, &c],
        &stub.endpoint(),
        &keys,
    )?)?;

    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Federated = serde_json::from_str(&text)?;
    assert_eq!(
        "active", answer.meta.federation.endpoints[0].status,
        "an outage of the repository is stored, not refused (ITI TF-2 §3.20.4.1.1)"
    );
    assert_eq!(
        Some("degraded"),
        await_state(&app, "degraded").await?.as_deref(),
        "degraded while the forwarder retries"
    );
    let samples = parse(&Metrics::default().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, "ferrofed_audit_spool_events", &[])
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let retries = loop {
        let samples = parse(&Metrics::default().render()?)?;
        let retries: u64 = count(&samples, "ferrofed_audit_retries_total", &[])
            .ok_or("the retries are counted")?
            .parse()?;
        if retries >= 1 || Instant::now() >= deadline {
            break retries;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert!(retries >= 1, "every failed attempt is counted: {retries}");
    assert_eq!(1, spooled(&spool)?, "the message is on disk");

    repository.set_up(true);
    let messages = repository.wait_for(1, Duration::from_secs(10)).await;
    assert_eq!(
        1,
        messages.len(),
        "the spool drains once the repository is back"
    );
    assert_eq!(Some("up"), await_state(&app, "up").await?.as_deref());
    assert_eq!(0, spooled(&spool)?);
    Ok(())
}

/// The messages in the spool directory `spool`, its quarantine left out.
fn spooled(spool: &Path) -> Result<usize, Box<dyn Error>> {
    let mut files = 0;
    for entry in std::fs::read_dir(spool)? {
        if entry?.path().is_file() {
            files += 1;
        }
    }
    Ok(files)
}

// conformance: CP-5
#[tokio::test]
async fn a_full_spool_fails_the_query_closed_and_asks_no_member() -> TestResult {
    let servers = members().await;
    let [a, b, c] = urls(&servers);
    let stub = holding().await;
    let repository = AuditRepository::start().await?;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let keys = reaching(dir.path(), &repository, "spool_max_events = 1")?;
    let (app, _state) = gateway(&config(
        dir.path(),
        "development",
        [&a, &b, &c],
        &stub.endpoint(),
        &keys,
    )?)?;

    let (status, _) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status);
    let asked = received(&servers[0]).await?.len();
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Federated = serde_json::from_str(&text)?;
    for endpoint in &answer.meta.federation.endpoints {
        assert_eq!("not-localized", endpoint.status, "{text}");
        let error = serde_json::to_string(&endpoint.error)?;
        assert!(error.contains("audit"), "{error}");
        assert!(!error.contains(PATIENT), "{error}");
    }
    assert_eq!(
        asked,
        received(&servers[0]).await?.len(),
        "no member is asked without the audit record (§14.1)"
    );
    Ok(())
}

/// A configuration text over unreachable members and an `https` gateway,
/// for a check that never sends.
fn unreachable(dir: &Path, profile: &str, repository: &str) -> Result<String, Box<dyn Error>> {
    config(
        dir,
        profile,
        [
            "https://a.example.org",
            "https://b.example.org",
            "https://c.example.org",
        ],
        "https://xcpd.example.org/RespondingGateway",
        repository,
    )
}

#[test]
fn plain_tcp_outside_development_refuses_to_boot_naming_its_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let spool = toml::Value::String(dir.path().join("spool").display().to_string());
    let text = unreachable(
        dir.path(),
        "production",
        &format!("url = \"tcp://arr.example.org:601\"\nspool_dir = {spool}"),
    )?;
    match Config::from_sources(Some(&text), &BTreeMap::new())?.resolve() {
        Err(error::Error::Cleartext(refused)) => {
            assert_eq!("xcpd.audit_repository.url", refused.site.url_key);
            Ok(())
        }
        other => Err(format!("the audit message names the patient: {other:?}").into()),
    }
}

#[test]
fn a_repository_audit_without_a_home_community_refuses_to_boot_naming_the_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let spool = toml::Value::String(dir.path().join("spool").display().to_string());
    let text = unreachable(
        dir.path(),
        "production",
        &format!("url = \"tls://arr.example.org\"\nspool_dir = {spool}"),
    )?
    .replace("home_community = \"2.999.40\"\n", "");
    match Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve() {
        Err(error::Error::Missing { key }) if key == "xcpd.home_community" => {}
        other => return Err(format!("§3.55.5.1.1 needs the homeCommunityID: {other:?}").into()),
    }
    let logged = text
        .replace("audit = \"repository\"", "audit = \"log\"")
        .split("[xcpd.audit_repository]")
        .next()
        .ok_or("the table")?
        .to_owned();
    Config::from_sources(Some(&crate::support::signed(&logged)), &BTreeMap::new())?.resolve()?;
    Ok(())
}

#[test]
fn a_spool_in_memory_outside_development_refuses_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = unreachable(dir.path(), "production", "url = \"tls://arr.example.org\"")?;
    match Config::from_sources(Some(&text), &BTreeMap::new())?.resolve() {
        Err(error::Error::Missing { key }) if key == "xcpd.audit_repository.spool_dir" => Ok(()),
        other => Err(format!("a restart would lose the spool: {other:?}").into()),
    }
}

#[test]
fn the_repository_table_and_the_destination_go_together() -> TestResult {
    let dir = tempfile::tempdir()?;
    let unused = unreachable(dir.path(), "development", "url = \"tls://arr.example.org\"")?
        .replace("audit = \"repository\"", "audit = \"log\"");
    match Config::from_sources(Some(&unused), &BTreeMap::new())?.resolve() {
        Err(error::Error::AuditRepositoryUnused) => {}
        other => return Err(format!("a table no message reaches: {other:?}").into()),
    }
    let text = unreachable(dir.path(), "development", "url = \"tls://arr.example.org\"")?;
    let missing = text
        .split("[xcpd.audit_repository]")
        .next()
        .ok_or("the table")?
        .to_owned();
    match Config::from_sources(Some(&missing), &BTreeMap::new())?.resolve() {
        Err(error::Error::Missing { key }) if key == "xcpd.audit_repository" => Ok(()),
        other => Err(format!("audit = \"repository\" names its table: {other:?}").into()),
    }
}

#[cfg(unix)]
#[test]
fn a_spool_open_to_other_users_refuses_to_load() -> TestResult {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir()?;
    let spool = dir.path().join("spool");
    std::fs::create_dir_all(&spool)?;
    std::fs::set_permissions(&spool, std::fs::Permissions::from_mode(0o755))?;
    let text = unreachable(
        dir.path(),
        "production",
        &format!(
            "url = \"tls://arr.example.org\"\nspool_dir = {}",
            toml::Value::String(spool.display().to_string())
        ),
    )?;
    match load(&text)? {
        Err(FederationError::Localization(LocalizationError::AuditTrail(refused))) => {
            let mut text = refused.to_string();
            let mut cause = Error::source(&refused);
            while let Some(link) = cause {
                text.push_str(": ");
                text.push_str(&link.to_string());
                cause = link.source();
            }
            assert!(
                text.contains("open to its group or to other users"),
                "{text}"
            );
            Ok(())
        }
        other => Err(format!("the spool holds patient identifiers: {other:?}").into()),
    }
}
