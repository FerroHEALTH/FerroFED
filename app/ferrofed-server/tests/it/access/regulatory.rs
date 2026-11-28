// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The regulatory metadata of an access record (Xt-EHR D5.2 §7.2.7): the
//! legal basis `[access_log.regulatory]` declares, the GDPR Art 6(1) point
//! and Art 9(2) condition, and its controllers and processors ride on the
//! record's categories entity; a deployment that declares none writes none.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::sync::Arc;

use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;
use serde_json::Value;

use super::{
    LAB_REPORT, RETENTION, TestResult, accesses, composition, details, node_with_rows, settings,
};
use crate::facade::{NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

const UID_A: &str = "3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f::cdr-a.example.org::1";

/// The declaration of a public-task basis for health care, one controller
/// and one processor.
const DECLARED: &str = "\n[access_log.regulatory]\n\
    legal_basis = { lawfulness = \"public-task\", condition = \"health-or-social-care\" }\n\
    controllers = [\"urn:oid:2.999.7.1\"]\n\
    processors = [\"urn:oid:2.999.7.2\"]\n";

/// The patient query selecting whole compositions.
fn compositions() -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The one access record of a patient query under the `[access_log]`
/// tables `extra`.
async fn record_under(extra: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", ""),
        &format!("{RETENTION}{extra}"),
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

/// D5.2 §7.2.7: the declared legal basis, controller and processor are on
/// the record's categories entity.
#[tokio::test]
async fn the_declared_legal_basis_and_roles_reach_the_record() -> TestResult {
    let record = record_under(DECLARED).await?;
    let on = |kind: &str| details(&record, "ehds-categories", kind);
    assert_eq!(on("ehds-legal-basis"), ["gdpr-art-6-1-e"], "{record}");
    assert_eq!(on("ehds-legal-condition"), ["gdpr-art-9-2-h"]);
    assert_eq!(on("ehds-controller"), ["urn:oid:2.999.7.1"]);
    assert_eq!(on("ehds-processor"), ["urn:oid:2.999.7.2"]);
    Ok(())
}

/// A deployment that declares none writes none.
#[tokio::test]
async fn an_undeclared_basis_writes_none() -> TestResult {
    let record = record_under("").await?;
    for kind in [
        "ehds-legal-basis",
        "ehds-legal-condition",
        "ehds-controller",
        "ehds-processor",
    ] {
        assert!(
            details(&record, "ehds-categories", kind).is_empty(),
            "{kind}: {record}"
        );
    }
    Ok(())
}
