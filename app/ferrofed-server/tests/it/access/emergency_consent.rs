// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The consent pre-filter under an emergency purpose, `[federation.consent]
//! emergency` (Regulation (EU) 2025/327 Art 11(5); N26, N27, N27a).
//!
//! Under `"apply"`, the default, a member the pre-filter denies is not asked,
//! whatever the caller's purpose. Under `"pass-to-node"`, a request whose
//! verified token declares an `[[access_log.emergency_purpose]]` still asks
//! the pre-filter, asks every member all the same, and its access record
//! names each member whose denial was set aside; the answer says nothing of
//! it (Art 8). A request without the purpose is filtered as before, and the
//! setting needs a declared emergency purpose to load.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;

use super::emergency::{break_the_glass, declaring};
use super::{
    LAB_REPORT, RETENTION, TestResult, accesses, composition, details, named, node_with_rows,
    settings_resolving,
};
use crate::auth::{bearing, minted, query};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, received, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::{self, call};

const UID_A: &str = "5d6e7f80-1a2b-4c3d-8e4f-5a6b7c8d9e0f::cdr-a.example.org::1";
const UID_B: &str = "6e7f8091-2b3c-4d4e-9f5a-6b7c8d9e0f1a::cdr-b.example.org::1";

/// The entity that marks an emergency access.
const MARK: &str = "ehds-emergency-access";

/// The `detail` that names a member whose denial was set aside.
const SET_ASIDE: &str = "ehds-consent-set-aside";

/// `[federation.consent]` passing an emergency request to the node.
const PASS_TO_NODE: &str = "\n[federation.consent]\nemergency = \"pass-to-node\"\n";

/// The cross-reference of the patient at both members, and the development
/// pre-filter denying asking node B about the patient.
fn denying_node_b() -> String {
    format!(
        "{}\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-b\"\n",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
    )
}

/// What one access left behind.
struct Asked {
    status: StatusCode,
    text: String,
    record: Value,
    node_b: Server,
}

/// Sends the suite's patient query with a token over `claims` to a gateway
/// whose `[federation]` adds `federation`, whose identity tables are
/// `identity`, and whose `[access_log]` names break the glass, and returns
/// what the access left behind.
async fn asked(federation: &str, identity: &str, claims: &Claims) -> Result<Asked, Box<dyn Error>> {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[composition(LAB_REPORT, UID_B)]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings_resolving(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", federation),
        (&format!("{RETENTION}{}", break_the_glass()), identity),
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    let mut server = settings_with_room();
    server.auth = support::auth();
    #[cfg(feature = "binding-nl")]
    if identity.contains("[nl_gf.mitz]") {
        for issuer in &mut server.auth.issuers {
            issuer.requester = Some(crate::mitz::requester_claims());
        }
    }
    let app = ferrofed_server::router(state, &server);
    let (status, text) = call(app, bearing(query()?, &minted(claims)?)?).await?;
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the access, got {records:?}").into());
    };
    Ok(Asked {
        status,
        text,
        record: record.clone(),
        node_b,
    })
}

/// The status of each endpoint the answer `text` names.
fn endpoint_statuses(text: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let answer: Value = serde_json::from_str(text)?;
    let endpoints = answer
        .pointer("/meta/federation/endpoints")
        .and_then(Value::as_array)
        .ok_or("meta.federation.endpoints")?;
    Ok(endpoints
        .iter()
        .map(|endpoint| {
            let read = |key: &str| endpoint[key].as_str().unwrap_or_default().to_owned();
            (read("id"), read("status"))
        })
        .collect())
}

/// Art 11(5), N26, N27: under `"pass-to-node"` an emergency request asks the
/// member the pre-filter denies, its node decides, and the record names the
/// member whose denial was set aside; the answer says nothing of it (Art 8).
#[tokio::test]
async fn an_emergency_request_asks_the_denied_member_and_records_the_set_aside() -> TestResult {
    let Asked {
        status,
        text,
        record,
        node_b,
    } = asked(PASS_TO_NODE, &denying_node_b(), &declaring(&["BTG"])).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, received(&node_b).await?.len(), "N27: node B decides");
    assert_eq!(
        endpoint_statuses(&text)?,
        [
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "active".to_owned())
        ]
    );
    assert_eq!(details(&record, MARK, SET_ASIDE), ["node-b"], "{record}");
    for leaked in ["set aside", "set-aside", "consent-denied", "emergency"] {
        assert!(
            !text.contains(leaked),
            "Art 8: {leaked} in the answer: {text}"
        );
    }
    Ok(())
}

/// N27a: under `"apply"`, the default, a member the pre-filter denies is not
/// asked, an emergency purpose or not, and nothing is set aside.
#[tokio::test]
async fn by_default_the_pre_filter_applies_to_an_emergency_request() -> TestResult {
    let Asked {
        text,
        record,
        node_b,
        ..
    } = asked("", &denying_node_b(), &declaring(&["BTG"])).await?;
    assert!(received(&node_b).await?.is_empty(), "node B is not asked");
    assert_eq!(
        endpoint_statuses(&text)?,
        [
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "consent-denied".to_owned())
        ]
    );
    assert_eq!(
        named(&record, MARK).len(),
        1,
        "the access is marked: {record}"
    );
    assert!(details(&record, MARK, SET_ASIDE).is_empty(), "{record}");
    Ok(())
}

/// §13.4: only a declared purpose makes a request an emergency, so under
/// `"pass-to-node"` a request without one is filtered as any other.
#[tokio::test]
async fn a_request_without_the_emergency_purpose_is_filtered_as_before() -> TestResult {
    let Asked { record, node_b, .. } =
        asked(PASS_TO_NODE, &denying_node_b(), &declaring(&["ETREAT"])).await?;
    assert!(received(&node_b).await?.is_empty(), "node B is not asked");
    assert!(named(&record, MARK).is_empty(), "{record}");
    Ok(())
}

/// Under `"pass-to-node"`, an emergency request still asks Mitz, asks the
/// member Mitz denies, and records that its denial was set aside.
#[cfg(feature = "binding-nl")]
#[tokio::test]
async fn under_mitz_the_question_is_still_asked_and_the_denial_set_aside() -> TestResult {
    let mitz = ferrofed_testkit::mitz::Mitz::start().await;
    mitz.deny(PATIENT, crate::mitz::URA_B);
    let identity = format!(
        "{}{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        crate::mitz::mitz_table(&mitz.endpoint(), &crate::mitz::holders())
    );
    let mut claims = declaring(&["BTG"]);
    crate::mitz::naming_requester(&mut claims, crate::mitz::CALLER);
    let Asked {
        status,
        text,
        record,
        node_b,
    } = asked(PASS_TO_NODE, &identity, &claims).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!mitz.questions().await.is_empty(), "Mitz is still asked");
    assert_eq!(1, received(&node_b).await?.len(), "N27: node B decides");
    assert_eq!(details(&record, MARK, SET_ASIDE), ["node-b"], "{record}");
    Ok(())
}

/// A gateway whose pre-filter denies node B, the patient's one holder, under
/// `federation`, recording with the `[audit.repository]` keys `audit`, and
/// node B answering a read of its EHR with `status`.
struct SubjectGateway {
    app: Router,
    repository: FeedRepository,
    node_a: Server,
    node_b: Server,
    _dir: tempfile::TempDir,
}

impl SubjectGateway {
    async fn start(
        federation: &str,
        audit: &str,
        status: StatusCode,
    ) -> Result<Self, Box<dyn Error>> {
        let node_a = Server::start().await;
        let node_b = Server::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!("/v1/ehr/{EHR_B}")))
            .respond_with(wiremock::ResponseTemplate::new(status.as_u16()).set_body_raw(
                format!(
                    r#"{{"system_id":{{"value":"cdr-b.example.org"}},"ehr_id":{{"value":"{EHR_B}"}}}}"#
                )
                .into_bytes(),
                "application/json",
            ))
            .mount(&node_b)
            .await;
        let identity = format!(
            "{}\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-b\"\n",
            crossref(&[("node-b", EHR_B)])
        );
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let settings = settings_resolving(
            dir.path(),
            (&node_a.uri(), &node_b.uri()),
            &repository,
            (audit, federation),
            (&format!("{RETENTION}{}", break_the_glass()), &identity),
        )?;
        let mut server = settings_with_room();
        server.auth = support::auth();
        let app = ferrofed_server::router(Arc::new(AppState::build(&settings)?), &server);
        Ok(Self {
            app,
            repository,
            node_a,
            node_b,
            _dir: dir,
        })
    }

    /// The read of the patient's EHR by subject with a token over `claims`.
    async fn read(&self, claims: &Claims) -> Result<(StatusCode, String), Box<dyn Error>> {
        let request = Request::get(format!(
            "/v1/ehr?subject_id={PATIENT}&subject_namespace={NAMESPACE}"
        ))
        .body(Body::empty())?;
        call(self.app.clone(), bearing(request, &minted(claims)?)?).await
    }

    /// The access records the repository holds once `count` arrived.
    async fn records(&self, count: usize) -> Result<Vec<Value>, Box<dyn Error>> {
        accesses(&self.repository.wait_for(count, SETTLE).await)
    }
}

/// The read of an EHR by subject honours the setting as the federated query
/// does: under `"pass-to-node"` the denied holder is asked and named in the
/// record, under `"apply"` it is not asked.
#[tokio::test]
async fn a_read_by_subject_sets_the_denial_aside_only_under_pass_to_node() -> TestResult {
    let passing = SubjectGateway::start(PASS_TO_NODE, "", StatusCode::OK).await?;
    let (status, text) = passing.read(&declaring(&["BTG"])).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        received(&passing.node_b).await?.len(),
        "N27: node B decides"
    );
    let records = passing.records(1).await?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the read, got {records:?}").into());
    };
    assert_eq!(details(record, MARK, SET_ASIDE), ["node-b"], "{record}");
    let applying = SubjectGateway::start("", "", StatusCode::OK).await?;
    let (status, _) = applying.read(&declaring(&["BTG"])).await?;
    assert_ne!(StatusCode::OK, status, "the one holder is not asked");
    assert!(
        received(&applying.node_b).await?.is_empty(),
        "node B is not asked"
    );
    Ok(())
}

/// Art 11(5): a read that set a denial aside is recorded whatever the node
/// answered, a refusal included, naming the member it set aside.
#[tokio::test]
async fn a_refused_read_that_set_a_denial_aside_is_still_recorded() -> TestResult {
    let gateway = SubjectGateway::start(PASS_TO_NODE, "", StatusCode::FORBIDDEN).await?;
    let (status, text) = gateway.read(&declaring(&["BTG"])).await?;
    assert_ne!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        received(&gateway.node_b).await?.len(),
        "node B was asked"
    );
    let records = gateway.records(1).await?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the read, got {records:?}").into());
    };
    assert_eq!(details(record, MARK, SET_ASIDE), ["node-b"], "{record}");
    Ok(())
}

/// A read that set a denial aside and whose record cannot be stored is
/// refused `503 access-unrecorded`, with none of the node's answer.
#[tokio::test]
async fn a_read_that_set_a_denial_aside_and_cannot_be_recorded_is_refused() -> TestResult {
    let gateway =
        SubjectGateway::start(PASS_TO_NODE, "spool_max_events = 1", StatusCode::OK).await?;
    gateway.repository.set_up(false);
    let (status, text) = gateway.read(&declaring(&["BTG"])).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the first record fills the spool: {text}"
    );
    let (status, text) = gateway.read(&declaring(&["BTG"])).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(text.contains("access-unrecorded"), "{text}");
    assert!(
        !text.contains(EHR_B),
        "no data leaves without its record: {text}"
    );
    Ok(())
}

/// A denial set aside holds for its one request: the next ordinary read by
/// the same caller is filtered again, its member not asked, and nothing the
/// emergency read resolved is reused for it.
#[tokio::test]
async fn the_next_ordinary_read_is_filtered_again() -> TestResult {
    let gateway = SubjectGateway::start(PASS_TO_NODE, "", StatusCode::OK).await?;
    let (status, _) = gateway.read(&declaring(&["BTG"])).await?;
    assert_eq!(StatusCode::OK, status);
    let (status, text) = gateway.read(&declaring(&["ETREAT"])).await?;
    assert_ne!(
        StatusCode::OK,
        status,
        "the ordinary read is filtered: {text}"
    );
    assert_eq!(
        1,
        received(&gateway.node_b).await?.len(),
        "node B is asked by the emergency read alone"
    );
    let path = format!("/v1/ehr/{EHR_B}");
    let request = Request::get(path).body(Body::empty())?;
    call(
        gateway.app.clone(),
        bearing(request, &minted(&declaring(&["ETREAT"]))?)?,
    )
    .await?;
    let probed = gateway
        .node_a
        .received_requests()
        .await
        .ok_or("recording is on")?;
    assert!(
        probed
            .iter()
            .any(|request| request.url.path() == format!("/v1/ehr/{EHR_B}")),
        "§12.5.1: the emergency read taught no binding and no index entry, so the ehr_id is probed"
    );
    Ok(())
}

/// A denial set aside holds for its one request: the next ordinary query
/// by the same caller is filtered again.
#[tokio::test]
async fn the_next_ordinary_query_is_filtered_again() -> TestResult {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[composition(LAB_REPORT, UID_B)]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings_resolving(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", PASS_TO_NODE),
        (
            &format!("{RETENTION}{}", break_the_glass()),
            &denying_node_b(),
        ),
    )?;
    let mut server = settings_with_room();
    server.auth = support::auth();
    let app = ferrofed_server::router(Arc::new(AppState::build(&settings)?), &server);
    let (status, text) = call(
        app.clone(),
        bearing(query()?, &minted(&declaring(&["BTG"]))?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (status, text) = call(app, bearing(query()?, &minted(&declaring(&["ETREAT"]))?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        endpoint_statuses(&text)?,
        [
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "consent-denied".to_owned())
        ]
    );
    assert_eq!(
        1,
        received(&node_b).await?.len(),
        "node B is asked by the emergency query alone"
    );
    Ok(())
}

/// The setting needs a declared emergency purpose: without one no request
/// could be an emergency, so the configuration does not load.
#[test]
fn pass_to_node_without_an_emergency_purpose_is_refused() {
    let text = format!("profile = \"development\"\n{PASS_TO_NODE}");
    let refused = Config::from_sources(Some(&text), &BTreeMap::new())
        .and_then(|config| config.resolve().map(|_| ()));
    assert!(
        matches!(
            refused,
            Err(ferrofed_server::config::error::Error::EmergencyWithoutPurpose)
        ),
        "{refused:?}"
    );
    let declared = format!("{text}{}", break_the_glass());
    assert!(
        Config::from_sources(Some(&declared), &BTreeMap::new())
            .and_then(|config| config.resolve().map(|_| ()))
            .is_ok()
    );
}

/// §7a.2: `OPTIONS {base}/` declares the setting beside the pre-filter.
#[tokio::test]
async fn options_declares_the_emergency_setting() -> TestResult {
    let (node_a, node_b) = (Server::start().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    for (federation, declared) in [("", "apply"), (PASS_TO_NODE, "pass-to-node")] {
        let settings = settings_resolving(
            dir.path(),
            (&node_a.uri(), &node_b.uri()),
            &repository,
            ("", federation),
            (
                &format!("{RETENTION}{}", break_the_glass()),
                &denying_node_b(),
            ),
        )?;
        let app =
            ferrofed_server::router(Arc::new(AppState::build(&settings)?), &settings_with_room());
        let (status, text) = call(app, Request::options("/").body(Body::empty())?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        crate::facade::schema::validate_options(&text)?;
        let body: Value = serde_json::from_str(&text)?;
        assert_eq!(
            body.pointer("/federation/consent/emergency")
                .and_then(Value::as_str),
            Some(declared),
            "{text}"
        );
    }
    Ok(())
}

/// `config check` notes that Wabvpz and Mitz name no emergency route when
/// `"pass-to-node"` is set beside `[nl_gf.mitz]`, and not under `"apply"`.
#[cfg(feature = "binding-nl")]
#[test]
fn config_check_notes_pass_to_node_beside_mitz() -> TestResult {
    let note = "Wabvpz Art 15a has no emergency exception";
    let mitz = crate::mitz::mitz_table("http://127.0.0.1:9", &crate::mitz::holders());
    let registry = "\n[registry]\ndocument = \"registry.toml\"\n";
    for (federation, noted) in [(PASS_TO_NODE, true), ("", false)] {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("registry.toml"),
            crate::facade::registry("http://127.0.0.1:9", "http://127.0.0.1:10", ""),
        )?;
        let registry = registry.replace(
            "registry.toml",
            &dir.path().join("registry.toml").display().to_string(),
        );
        let text = format!(
            "profile = \"development\"\n{registry}\n[federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n{federation}{mitz}{}",
            break_the_glass()
        );
        let checked = crate::run::binary(&["config", "check"], &text)?;
        assert_eq!(
            Some(0),
            checked.status.code(),
            "{}",
            String::from_utf8_lossy(&checked.stderr)
        );
        let stdout = String::from_utf8_lossy(&checked.stdout);
        assert_eq!(noted, stdout.contains(note), "{stdout}");
    }
    Ok(())
}
