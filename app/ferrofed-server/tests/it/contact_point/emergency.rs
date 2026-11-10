// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The limit of the emergency accesses a national contact point relays for
//! one health professional (eHDSI Section II Security Services 10.0.0 §2.5;
//! `NCPeH` Architecture Specifications 10.0.0 §5.1.9).
//!
//! `[emergency]` defaults to the §2.5 baseline, five accesses per
//! professional per 24 hours, and a looser limit refuses the configuration
//! naming §2.5; a contact point's emergency access the gateway cannot count
//! is refused with no member asked, and recorded with the emergency mark;
//! and a request that is no emergency access, or is not relayed by a
//! contact point, is never counted. The count itself runs against
//! PostgreSQL in `e2e::emergency`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the records are read as JSON values"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::audit::AuditSettings;
use ferrofed_server::config::emergency::{EmergencyFault, EmergencyLimit};
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::{declared, relaying};
use crate::access::{RETENTION, accesses, details, named, node_with_rows, settings};
use crate::auth::{query, sent};
use crate::facade::{received, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::run::binary;
use crate::support::error_body;

type TestResult = Result<(), Box<dyn Error>>;

/// The code system the test contact point's tokens name the eHDSI purposes
/// of use in: the identifier eHDSI SAML Profile 10.0.0 §2.3 gives its
/// temporary `eHDSI PurposeOfUse` system, as a URN.
pub(crate) const EHDSI_PURPOSE: &str = "urn:uuid:3bc18518-d305-46c2-a8d6-94bd59856e9e";

/// The entity that marks an emergency access.
pub(crate) const MARK: &str = "ehds-emergency-access";

/// The anomaly warning the record of a denied emergency access names.
pub(crate) const ANOMALY: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS";

/// `[[access_log.emergency_purpose]]` naming the eHDSI `EMERGENCY` purpose.
pub(crate) fn emergency_purpose() -> String {
    format!(
        "\n[[access_log.emergency_purpose]]\nsystem = \"{EHDSI_PURPOSE}\"\ncode = \"EMERGENCY\"\n"
    )
}

/// The test contact point's claims, relaying the professional
/// `hp_identifier` for the purpose `purpose` (eHDSI SAML Profile 10.0.0
/// §2.3).
pub(crate) fn relayed_for(hp_identifier: &str, purpose: &str) -> Claims {
    let mut claims = relaying();
    for (field, value) in [
        (
            "professional.hp_identifier",
            serde_json::json!(hp_identifier),
        ),
        (
            "purpose_of_use",
            serde_json::json!([{ "system": EHDSI_PURPOSE, "code": purpose }]),
        ),
    ] {
        claims.ehdsi.insert(field.to_owned(), value);
    }
    claims
}

/// The settings of a development gateway over node A at `a` and node B at
/// `b`, recording to `repository`, naming `EMERGENCY` an emergency purpose,
/// with the top-level tables `tables` added.
pub(crate) fn emergency_settings(
    dir: &Path,
    (a, b): (&str, &str),
    repository: &FeedRepository,
    tables: &str,
) -> Result<Settings, Box<dyn Error>> {
    settings(
        dir,
        (a, b),
        repository,
        ("", ""),
        &format!("{RETENTION}{}{tables}", emergency_purpose()),
    )
}

/// The router over `state`, trusting the test contact point.
pub(crate) fn contact_point_router(state: Arc<AppState>) -> Router {
    let mut server = settings_with_room();
    server.auth = declared();
    ferrofed_server::router(state, &server)
}

/// Sends the patient query with a token over `claims` to `app`, and reads
/// the status and the body.
pub(crate) async fn asked(
    app: &Router,
    claims: &Claims,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let (status, _, text) = sent(app, super::presenting(query()?, claims)?).await?;
    Ok((status, text))
}

/// The access records `repository` holds once `count` messages arrived.
pub(crate) async fn records(
    repository: &FeedRepository,
    count: usize,
) -> Result<Vec<Value>, Box<dyn Error>> {
    accesses(&repository.wait_for(count, SETTLE).await)
}

/// Whether `record` carries the emergency mark (Regulation (EU) 2025/327
/// Art 11(5)).
pub(crate) fn marked(record: &Value) -> bool {
    named(record, MARK).len() == 1 && details(record, MARK, MARK) == ["true"]
}

/// A gateway with no store, naming `EMERGENCY` an emergency purpose, over
/// two nodes answering no rows.
async fn storeless()
-> Result<(Router, Server, Server, FeedRepository, tempfile::TempDir), Box<dyn Error>> {
    let (a, b) = (node_with_rows(&[]).await, node_with_rows(&[]).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = emergency_settings(dir.path(), (&a.uri(), &b.uri()), &repository, "")?;
    let app = contact_point_router(Arc::new(AppState::build(&settings)?));
    Ok((app, a, b, repository, dir))
}

/// §2.5, fail closed: an emergency access the gateway cannot count is
/// refused `emergency-uncounted` before any member is asked, and its record
/// carries the emergency mark.
#[tokio::test]
async fn an_emergency_access_no_store_counts_is_refused_with_no_member_asked() -> TestResult {
    let (app, a, b, repository, _dir) = storeless().await?;
    let (status, text) = asked(&app, &relayed_for("Qz7-hp-71", "EMERGENCY")).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!("emergency-uncounted", error_body(&text)?.code);
    assert!(received(&a).await?.is_empty(), "node A was not asked");
    assert!(received(&b).await?.is_empty(), "node B was not asked");
    let [record] = records(&repository, 1)
        .await?
        .try_into()
        .map_err(|_unexpected| "one record")?;
    assert!(marked(&record), "{record}");
    assert_ne!(Some("0"), record["outcome"].as_str(), "a failure: {record}");
    Ok(())
}

/// §2.5 asks for the count whatever becomes of the access record: under
/// development with no `[audit]`, so no access log, an emergency access the
/// gateway cannot count is still refused before any member is asked.
#[tokio::test]
async fn with_no_access_log_an_emergency_access_is_still_counted() -> TestResult {
    let (a, b) = (node_with_rows(&[]).await, node_with_rows(&[]).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut settings = emergency_settings(dir.path(), (&a.uri(), &b.uri()), &repository, "")?;
    settings.audit = AuditSettings::default();
    let app = contact_point_router(Arc::new(AppState::build(&settings)?));
    let (status, text) = asked(&app, &relayed_for("Qz7-hp-73", "EMERGENCY")).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!("emergency-uncounted", error_body(&text)?.code);
    assert!(received(&a).await?.is_empty(), "node A was not asked");
    assert!(received(&b).await?.is_empty(), "node B was not asked");
    Ok(())
}

/// Only an emergency access is counted: a contact point's request for
/// treatment is served with no store.
#[tokio::test]
async fn a_contact_point_request_for_treatment_is_not_counted() -> TestResult {
    let (app, a, _b, _repository, _dir) = storeless().await?;
    let (status, text) = asked(&app, &relayed_for("Qz7-hp-72", "TREATMENT")).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!received(&a).await?.is_empty(), "node A was asked");
    Ok(())
}

/// The resolution of a development configuration with `[emergency]`
/// written as `table`.
fn resolved(table: &str) -> Result<Settings, ConfigError> {
    let text = format!("profile = \"development\"\n\n[emergency]\n{table}");
    Config::from_sources(Some(&text), &BTreeMap::new()).and_then(|config| config.resolve())
}

/// §2.5: "No more than five emergency access attempts per HP per 24-hour
/// period" is the limit when none is declared.
#[test]
fn the_default_limit_is_five_per_professional_per_24_hours() -> TestResult {
    let text = "profile = \"development\"\n";
    let limit = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()?
        .emergency;
    assert_eq!(5, limit.threshold.get());
    assert_eq!(Duration::from_hours(24), limit.window);
    assert_eq!(EmergencyLimit::default(), limit);
    Ok(())
}

/// §2.5 lets a country tighten the limit.
#[test]
fn a_tighter_limit_is_taken() -> TestResult {
    let limit = resolved("threshold = 2\nwindow = '1h'\n")?.emergency;
    assert_eq!(2, limit.threshold.get());
    assert_eq!(Duration::from_secs(3600), limit.window);
    Ok(())
}

/// §2.5 lets a country tighten the limit and no more: a threshold above
/// five refuses the configuration naming §2.5.
#[test]
fn a_threshold_above_five_is_refused_naming_section_2_5() {
    let refused = resolved("threshold = 6\n").err();
    assert!(
        matches!(
            &refused,
            Some(ConfigError::Emergency(EmergencyFault::Threshold {
                value: 6
            }))
        ),
        "{refused:?}"
    );
    let message = refused.map(|error| error.to_string()).unwrap_or_default();
    assert!(message.contains("§2.5"), "{message}");
    assert!(message.contains("emergency.threshold"), "{message}");
}

/// A window longer than the §2.5 24-hour period refuses the configuration
/// naming §2.5.
#[test]
fn a_window_longer_than_24_hours_is_refused_naming_section_2_5() {
    let refused = resolved("window = '86401s'\n").err();
    assert!(
        matches!(
            &refused,
            Some(ConfigError::Emergency(EmergencyFault::Window {
                value: 86_401
            }))
        ),
        "{refused:?}"
    );
    let message = refused.map(|error| error.to_string()).unwrap_or_default();
    assert!(message.contains("§2.5"), "{message}");
    assert!(message.contains("emergency.window"), "{message}");
}

/// §2.5: every country "MUST provide" emergency access, so a limit that
/// grants none is refused, and so is an empty window.
#[test]
fn a_threshold_or_a_window_of_zero_is_refused() {
    for (table, key) in [
        ("threshold = 0\n", "emergency.threshold"),
        ("window = '0ms'\n", "emergency.window"),
    ] {
        let refused = resolved(table).err();
        assert!(
            matches!(&refused, Some(ConfigError::Zero { key: refused }) if refused == key),
            "{table}: {refused:?}"
        );
    }
}

#[test]
fn an_unknown_key_of_the_table_is_refused() {
    assert!(resolved("limit = 3\n").is_err());
}

/// The boot refuses a looser limit as `config check` does, naming §2.5.
#[test]
fn config_check_refuses_a_looser_limit_naming_section_2_5() -> TestResult {
    for table in ["threshold = 10000\n", "window = '2d'\n"] {
        let output = binary(
            &["config", "check"],
            &format!("profile = \"development\"\n\n[emergency]\n{table}"),
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(Some(0), output.status.code(), "{table}: {stderr}");
        assert!(stderr.contains("§2.5"), "{table}: {stderr}");
    }
    Ok(())
}
