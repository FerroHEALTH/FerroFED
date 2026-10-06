// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gate every answer passes: an answer of a patient-data operation that
//! carries data and no access is withheld, so a path that forgot to report
//! its access cannot hand out data without a record (Annex II 3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use ferrofed_server::access::{Gate, record};
use ferrofed_server::base_path::BasePath;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use http::{Request, StatusCode};

use super::{RETENTION, TestResult, map_toml};
use crate::facade::{EHR_A, registry};
use crate::feed_audit::audit_tables;
use crate::support::call;

/// What a handler that forgot its access answers.
const FORGOTTEN: &str = "Qz7-unrecorded-data";

/// A surface whose patient-data routes answer data and report no access,
/// behind the gate of a federation that keeps an access log.
fn forgetful(
    dir: &std::path::Path,
    repository: &FeedRepository,
) -> Result<Router, Box<dyn std::error::Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n{}{}",
        audit_tables(repository, ""),
        map_toml(RETENTION)
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let gate = Arc::new(Gate::new(state, BasePath::default()));
    Ok(Router::new()
        .route("/v1/ehr/{ehr_id}", get(|| async { FORGOTTEN }))
        .route(
            "/v1/definition/template/adl1.4",
            get(|| async { FORGOTTEN }),
        )
        .layer(axum::middleware::from_fn_with_state(gate, record)))
}

#[tokio::test]
async fn a_patient_data_answer_that_reports_no_access_is_withheld() -> TestResult {
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = forgetful(dir.path(), &repository)?;
    let request = Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(text.contains("access-unrecorded"), "{text}");
    assert!(
        !text.contains(FORGOTTEN),
        "no data leaves without its record"
    );
    let request = Request::get("/v1/definition/template/adl1.4").body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "a definition is no patient data: {text}"
    );
    Ok(())
}
