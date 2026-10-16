// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security labels of each access record (Regulation (EU) 2025/327
//! Annex II 3.4): the record carries a label per category of the data and
//! per origin, in `meta.security` and on the entity each belongs to, so the
//! Audit Record Repository can limit who reads it (IHE `RESTful` ATNA
//! §3.81.4.1.3); `[access_log.labels]` replaces the defaults, and a label
//! the logging component refuses, or one for an endpoint the registry does
//! not hold, is refused when the configuration loads.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use ehds_logging::label::LabelError;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::access_log::AccessLogFault;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::state::{AppState, StateError};
use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;
use serde_json::Value;

use super::{
    DISCHARGE, LAB_REPORT, TestResult, UNMAPPED, accesses, composition, named, node_with_rows,
    settings,
};
use crate::facade::{NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

const UID_A: &str = "2c1d7c6e-2f43-4a51-9c3e-7a8b9c0d1e2f::cdr-a.example.org::1";
const UID_B: &str = "8e5d4c3b-2a19-4f08-8e7d-6c5b4a392817::cdr-b.example.org::1";

/// The code system of the priority categories, and its version.
const PRIORITY: &str =
    "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";
const PRIORITY_VERSION: &str = "1.0.0-ballot";

/// The code system an origin is labelled in unless the configuration names
/// another.
const ORIGIN_SYSTEM: &str = "https://ferrofed.eu/fhir/CodeSystem/access-log-origin";

/// A code system a deployment declares its own labels in.
const CLASSES: &str = "https://example.org/fhir/CodeSystem/access-class";

/// The patient query selecting whole compositions.
fn compositions() -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The one access record of a patient query node A answers with `a` and
/// node B with `b`, under the `[access_log]` tables `tables`.
async fn record_of(
    tables: &str,
    a: &[String],
    b: &[String],
) -> Result<Value, Box<dyn std::error::Error>> {
    let (node_a, node_b) = (node_with_rows(a).await, node_with_rows(b).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let nodes = (node_a.uri(), node_b.uri());
    let settings = settings(
        dir.path(),
        (&nodes.0, &nodes.1),
        &repository,
        ("", ""),
        tables,
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let (status, text) = call(app, post(body(&compositions())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one access record, got {}", records.len()).into());
    };
    Ok(record.clone())
}

/// The labels at `pointer` of `value`, each as `<system>|<code>` with
/// `@<version>` when it names one.
fn tokens(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|label| {
            let token = format!(
                "{}|{}",
                label["system"].as_str().unwrap_or_default(),
                label["code"].as_str().unwrap_or_default()
            );
            match label["version"].as_str() {
                Some(version) => format!("{token}@{version}"),
                None => token,
            }
        })
        .collect()
}

/// The labels on the `origin` entity of `endpoint` in `record`.
fn origin_labels(record: &Value, endpoint: &str) -> Vec<String> {
    named(record, "origin")
        .into_iter()
        .filter(|origin| {
            origin
                .pointer("/what/identifier/value")
                .and_then(Value::as_str)
                == Some(endpoint)
        })
        .flat_map(|origin| tokens(origin, "/securityLabel"))
        .collect()
}

/// The labels on the `ehds-categories` entity of `record`.
fn category_labels(record: &Value) -> Vec<String> {
    named(record, "ehds-categories")
        .into_iter()
        .flat_map(|entity| tokens(entity, "/securityLabel"))
        .collect()
}

fn priority(code: &str) -> String {
    format!("{PRIORITY}|{code}@{PRIORITY_VERSION}")
}

fn origin(endpoint: &str) -> String {
    format!("{ORIGIN_SYSTEM}|{endpoint}")
}

/// With no `[access_log.labels]` each category is labelled with its own
/// code and each origin, one that answered nothing included, with its
/// endpoint id in FerroFED's origin code system.
#[tokio::test]
async fn with_no_labels_table_a_record_carries_its_category_and_its_origins() -> TestResult {
    let record = record_of(
        "",
        &[composition(LAB_REPORT, UID_A)],
        &[composition(DISCHARGE, UID_B)],
    )
    .await?;
    assert_eq!(
        tokens(&record, "/meta/security"),
        [
            priority("Laboratory-Reports"),
            priority("Discharge-Reports"),
            origin("node-a-pub"),
            origin("node-b-pub"),
        ]
    );
    assert_eq!(
        category_labels(&record),
        [
            priority("Laboratory-Reports"),
            priority("Discharge-Reports")
        ]
    );
    assert_eq!(origin_labels(&record, "node-a-pub"), [origin("node-a-pub")]);
    assert_eq!(origin_labels(&record, "node-b-pub"), [origin("node-b-pub")]);
    Ok(())
}

#[tokio::test]
async fn the_labels_the_configuration_declares_replace_the_defaults() -> TestResult {
    let tables = format!(
        "\n[access_log.labels]\norigin_system = \"https://example.org/fhir/CodeSystem/origin\"\n\n\
         [access_log.labels.categories]\n\"Laboratory-Reports\" = [{{ system = \"{CLASSES}\", code = \"lab\" }}]\n\n\
         [access_log.labels.origins]\n\"node-a-pub\" = [{{ system = \"{CLASSES}\", code = \"hospital-a\" }}]\n"
    );
    let record = record_of(&tables, &[composition(LAB_REPORT, UID_A)], &[]).await?;
    assert_eq!(
        tokens(&record, "/meta/security"),
        [
            format!("{CLASSES}|lab"),
            format!("{CLASSES}|hospital-a"),
            "https://example.org/fhir/CodeSystem/origin|node-b-pub".to_owned(),
        ]
    );
    Ok(())
}

/// An access the map cannot classify may hold data of any category, so it
/// carries every category's label.
#[tokio::test]
async fn an_unclassified_access_carries_every_categorys_label() -> TestResult {
    let record = record_of("", &[composition(UNMAPPED, UID_A)], &[]).await?;
    assert_eq!(
        category_labels(&record),
        [
            "Patient-Summaries",
            "Electronic-Prescriptions",
            "Electronic-Dispensations",
            "Medical-Imaging",
            "Laboratory-Reports",
            "Discharge-Reports",
        ]
        .map(priority)
    );
    Ok(())
}

/// The name of the kind of `error`.
fn kind(error: &LabelError) -> &'static str {
    match error {
        LabelError::OriginSystem => "origin-system",
        LabelError::UnknownCategory { .. } => "unknown-category",
        LabelError::Empty { .. } => "empty",
        LabelError::Label { .. } => "label",
        _ => "other",
    }
}

#[test]
fn a_label_the_logging_component_refuses_is_refused_at_load() {
    for (table, expected) in [
        (
            "[access_log.labels]\norigin_system = \"origin\"\n",
            "origin-system",
        ),
        (
            "[access_log.labels.categories]\n\"nl-undeclared\" = [{ system = \"https://example.org/c\", code = \"x\" }]\n",
            "unknown-category",
        ),
        (
            "[access_log.labels.categories]\n\"Laboratory-Reports\" = []\n",
            "empty",
        ),
        (
            "[access_log.labels.origins]\n\"node-a-pub\" = [{ system = \"https://example.org/c\", code = \" x\" }]\n",
            "label",
        ),
    ] {
        let text = format!("profile = \"development\"\n\n{table}");
        let refused = Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve().map(|_| ()));
        assert!(
            matches!(&refused, Err(Error::AccessLog(AccessLogFault::Labels(error))) if kind(error) == expected),
            "{table}: {refused:?}"
        );
    }
}

#[test]
fn an_unknown_key_in_a_label_is_refused_at_load() {
    let text = "profile = \"development\"\n\n[access_log.labels.origins]\n\
                \"node-a-pub\" = [{ system = \"https://example.org/c\", code = \"x\", display = \"y\" }]\n";
    let refused = Config::from_sources(Some(text), &BTreeMap::new())
        .and_then(|config| config.resolve().map(|_| ()));
    assert!(refused.is_err(), "{refused:?}");
}

#[tokio::test]
async fn labels_for_an_endpoint_the_registry_does_not_hold_are_refused() -> TestResult {
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings(
        dir.path(),
        ("http://127.0.0.1:9/a", "http://127.0.0.1:9/b"),
        &repository,
        ("", ""),
        "\n[access_log.labels.origins]\n\"node-z-pub\" = [{ system = \"https://example.org/c\", code = \"z\" }]\n",
    )?;
    let refused = AppState::build(&settings).map(|_| ());
    assert!(
        matches!(
            &refused,
            Err(StateError::Federation(FederationError::LabelEndpointUnknown { endpoint }))
                if endpoint == "node-z-pub"
        ),
        "{refused:?}"
    );
    Ok(())
}
