// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How long the Audit Record Repository keeps each access record
//! (Regulation (EU) 2025/327 Art 9(2), Annex II 3.4): the record states the
//! period `[access_log.retention]` gives its categories and origins, never
//! under three years from the date of access, and the longest declared for
//! an access the map cannot classify; a retention under three years, or for
//! an endpoint the registry does not hold, is refused when the configuration
//! loads.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use ehds_logging::retention::RetentionError;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::state::{AppState, StateError};
use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;
use jiff::civil::Date;
use jiff::tz::Offset;
use jiff::{Span, Timestamp};

use super::{
    DISCHARGE, DISCHARGE_CATEGORY, LAB_REPORT, RETENTION, TestResult, UNMAPPED, accesses,
    composition, details, node_with_rows, settings,
};
use crate::facade::{NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

const UID_A: &str = "0b1d7c6e-2f43-4a51-9c3e-7a8b9c0d1e2f::cdr-a.example.org::1";
const UID_B: &str = "6e5d4c3b-2a19-4f08-8e7d-6c5b4a392817::cdr-b.example.org::1";

/// The patient query selecting whole compositions.
fn compositions() -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The retention details of the one access record of a patient query
/// node A answers with `a` and node B with `b`, both asked, under the
/// `[access_log.retention]` tables `retention`: the years, the first date it
/// may be deleted on, the ground, and the date the access was recorded.
async fn retention_of(
    retention: &str,
    a: &[String],
    b: &[String],
) -> Result<(String, String, String, Date), Box<dyn std::error::Error>> {
    let (node_a, node_b) = (node_with_rows(a).await, node_with_rows(b).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let nodes = (node_a.uri(), node_b.uri());
    let settings = settings(
        dir.path(),
        (&nodes.0, &nodes.1),
        &repository,
        ("", ""),
        retention,
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let (status, text) = call(app, post(body(&compositions())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one access record, got {}", records.len()).into());
    };
    let one = |kind: &str| -> Result<String, Box<dyn std::error::Error>> {
        let values = details(record, "ehds-categories", kind);
        match values.as_slice() {
            [value] => Ok(value.clone()),
            _ => Err(format!("one {kind}, got {values:?}").into()),
        }
    };
    let recorded: Timestamp = record["recorded"].as_str().ok_or("recorded")?.parse()?;
    Ok((
        one("ehds-retention-years")?,
        one("ehds-retention-ends")?,
        one("ehds-retention-ground")?,
        Offset::UTC.to_datetime(recorded).date(),
    ))
}

/// The first date a record of an access on `accessed` kept `years` may be
/// deleted on.
fn deletable(accessed: Date, years: i64) -> Result<String, Box<dyn std::error::Error>> {
    Ok(accessed
        .checked_add(Span::new().years(years).days(1))?
        .to_string())
}

/// With no `[access_log.retention]` every record is kept the three years of
/// Art 9(2).
#[tokio::test]
async fn with_no_retention_table_a_record_is_kept_three_years() -> TestResult {
    let (years, ends, ground, accessed) =
        retention_of("", &[composition(LAB_REPORT, UID_A)], &[]).await?;
    assert_eq!("3", years);
    assert_eq!("default", ground);
    assert_eq!(deletable(accessed, 3)?, ends);
    Ok(())
}

/// A record whose categories and origins call for no more than the default
/// is kept the default years.
#[tokio::test]
async fn a_record_whose_parts_call_for_no_more_is_kept_the_default_years() -> TestResult {
    let retention = "\n[access_log.retention]\nyears = 6\n\n\
                     [access_log.retention.origins]\n\"node-b-pub\" = 5\n";
    let (years, ends, ground, accessed) =
        retention_of(retention, &[composition(LAB_REPORT, UID_A)], &[]).await?;
    assert_eq!("6", years);
    assert_eq!("default", ground);
    assert_eq!(deletable(accessed, 6)?, ends);
    Ok(())
}

#[tokio::test]
async fn a_category_kept_longer_sets_the_period() -> TestResult {
    let (years, ends, ground, accessed) = retention_of(
        RETENTION,
        &[composition(LAB_REPORT, UID_A)],
        &[composition(DISCHARGE, UID_B)],
    )
    .await?;
    assert_eq!("20", years);
    assert_eq!(format!("category:{DISCHARGE_CATEGORY}"), ground);
    assert_eq!(deletable(accessed, 20)?, ends);
    Ok(())
}

/// Node B is asked and answers nothing: it is an origin of the access all
/// the same, so its twelve years apply.
#[tokio::test]
async fn an_origin_kept_longer_sets_the_period() -> TestResult {
    let (years, _, ground, _) =
        retention_of(RETENTION, &[composition(LAB_REPORT, UID_A)], &[]).await?;
    assert_eq!("12", years);
    assert_eq!("origin:node-b-pub", ground);
    Ok(())
}

/// An access the map cannot classify takes the longest period declared
/// anywhere, here the discharge report's twenty years.
#[tokio::test]
async fn an_unclassified_access_is_kept_the_longest_period_declared() -> TestResult {
    let (years, _, ground, _) =
        retention_of(RETENTION, &[composition(UNMAPPED, UID_A)], &[]).await?;
    assert_eq!("20", years);
    assert_eq!("unclassified", ground);
    Ok(())
}

/// Art 9(2): "at least three years from each date of access".
#[test]
fn a_retention_under_three_years_is_refused() {
    for (table, key) in [
        ("[access_log.retention]\nyears = 2\n", "years"),
        (
            "[access_log.retention.categories]\n\"Laboratory-Reports\" = 1\n",
            "categories.Laboratory-Reports",
        ),
        (
            "[access_log.retention.origins]\n\"node-a-pub\" = 2\n",
            "origins.node-a-pub",
        ),
    ] {
        let text = format!("profile = \"development\"\n\n{table}");
        let refused = Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve().map(|_| ()));
        assert!(
            matches!(
                &refused,
                Err(Error::AccessLogRetention(RetentionError::UnderFloor { key: named, .. }))
                    if named == key
            ),
            "{key}: {refused:?}"
        );
    }
}

#[test]
fn a_retention_for_a_category_the_map_does_not_declare_is_refused() {
    let text = "profile = \"development\"\n\n\
                [access_log.retention.categories]\n\"nl-undeclared\" = 10\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())
        .and_then(|config| config.resolve().map(|_| ()));
    assert!(
        matches!(
            refused,
            Err(Error::AccessLogRetention(
                RetentionError::UnknownCategory { .. }
            ))
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_retention_for_an_endpoint_the_registry_does_not_hold_is_refused() -> TestResult {
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings(
        dir.path(),
        ("http://127.0.0.1:9/a", "http://127.0.0.1:9/b"),
        &repository,
        ("", ""),
        "\n[access_log.retention.origins]\n\"node-z-pub\" = 10\n",
    )?;
    let refused = AppState::build(&settings).map(|_| ());
    assert!(
        matches!(
            &refused,
            Err(StateError::Federation(FederationError::RetentionEndpointUnknown { endpoint }))
                if endpoint == "node-z-pub"
        ),
        "{refused:?}"
    );
    Ok(())
}
