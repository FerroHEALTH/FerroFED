// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's export of the routing state one process learned, for a
//! system that replaces the gateway (Regulation (EU) 2025/327 Annex II 2.6):
//! admitted only for the issuer's operator scope and recorded with the
//! operator, it answers the learned `creating_system_id` mappings (§12.2,
//! N21) and no patient identifier, even while sessions hold resolution
//! bindings (§5.4.1, N33). The format is our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use ferrofed_identity::dev::Profile;
use ferrofed_registry::export::{
    FORMAT_VERSION, IdentityStateExport, LearnedCreatingSystem, LearnedStatus, NotExported,
    StateScope,
};
use ferrofed_server::admin::{self, Access, FINISHED};
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{DEFAULT_FILTER, Rendering, subscriber};
use http::{Request, StatusCode, header};
use serde::Deserialize;

use crate::ehr_id_collision::captured;
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, gateway_within_state,
    node_answering, patient_query, post, registry,
};
use crate::support::{self, ISSUER, Logs, bearer, error_body, operator_bearer, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// A version node A holds, created by a system no member is.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::legacy-x.example.org::1";

/// A version node B holds, created by another system no member is.
const VERSION_B: &str = "2c1fd7e4-6a43-4d0e-9a5e-6f1b2f1d6a01::legacy-y.example.org::1";

/// A peer off loopback, so only the gate admits it.
const REMOTE: ([u8; 4], u16) = ([192, 0, 2, 10], 40_000);

/// The admin listener's application over `state`, in production.
fn admin_over(state: &Arc<AppState>) -> Router {
    let access = Access::new(&support::auth(), Profile::Production, None);
    admin::router(Arc::clone(state), access).layer(MockConnectInfo(SocketAddr::from(REMOTE)))
}

/// The export as `authorization` with `payload`, with its status, its
/// `Cache-Control` and its body text.
async fn export(
    admin: &Router,
    authorization: Option<&str>,
    payload: Body,
) -> Result<(StatusCode, Option<String>, String), Box<dyn Error>> {
    let mut request = Request::post(admin::EXPORT_IDENTITY_STATE);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let response = send_as_is(admin.clone(), request.body(payload)?).await?;
    let status = response.status();
    let cache = response
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|value| value.to_str().map(ToOwned::to_owned))
        .transpose()?;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, cache, String::from_utf8(bytes.to_vec())?))
}

/// The gateway over two members, each answering one version, after a
/// patient query that bound the caller's session to both and taught the
/// learned map both creating systems.
async fn learned_and_bound(dir: &std::path::Path) -> Result<Arc<AppState>, Box<dyn Error>> {
    let a = node_answering(VERSION_A).await;
    let b = node_answering(VERSION_B).await;
    let (app, state) = gateway_within_state(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        (2_000, 3_000),
    )?;
    let (answers, _) = captured(&app, vec![post(body(&patient_query())?)?]).await?;
    for (status, _, text) in &answers {
        assert_eq!(StatusCode::OK, *status, "{text}");
    }
    let federation = state.federation().ok_or("a federation")?;
    assert!(
        !federation.bindings().is_empty(),
        "the query bound the caller's session"
    );
    drop(federation);
    Ok(state)
}

#[tokio::test]
async fn the_export_is_refused_without_the_operator_scope() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = learned_and_bound(dir.path()).await?;
    let admin = admin_over(&state);
    for (authorization, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(bearer()?), StatusCode::FORBIDDEN),
    ] {
        let (status, _, text) = export(&admin, authorization.as_deref(), Body::empty()).await?;
        assert_eq!(expected, status, "{text}");
        assert!(!text.contains("legacy-x"), "nothing is exported: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn the_export_holds_the_learned_creating_system_mappings_in_its_format() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = learned_and_bound(dir.path()).await?;
    let (status, cache, text) = export(
        &admin_over(&state),
        Some(&operator_bearer()?),
        Body::empty(),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some("no-store"), cache.as_deref());
    let answer: IdentityStateExport = serde_json::from_str(&text)?;
    assert_eq!(
        (FORMAT_VERSION, "example-federation", StateScope::Replica),
        (
            answer.format_version,
            answer.federation_id.as_str(),
            answer.scope
        )
    );
    assert!(
        answer.exported_at.parse::<jiff::Timestamp>().is_ok(),
        "{text}"
    );
    assert_eq!(
        vec![
            LearnedCreatingSystem {
                creating_system_id: "legacy-x.example.org".to_owned(),
                status: LearnedStatus::Learned,
                node: Some("node-a".to_owned()),
                endpoint: Some("node-a-pub".to_owned()),
            },
            LearnedCreatingSystem {
                creating_system_id: "legacy-y.example.org".to_owned(),
                status: LearnedStatus::Learned,
                node: Some("node-b".to_owned()),
                endpoint: Some("node-b-pub".to_owned()),
            },
        ],
        answer.learned_creating_systems
    );
    assert_eq!(NotExported::ALL.to_vec(), answer.not_exported);
    Ok(())
}

/// The fields of an admin action record the test reads.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Record {
    event: String,
    issuer: Option<String>,
    subject: Option<String>,
    action: Option<String>,
    outcome: Option<String>,
}

// conformance: CP-26
#[tokio::test]
async fn the_export_names_no_patient_identifier_while_bindings_exist_and_is_recorded() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let state = learned_and_bound(dir.path()).await?;
    let admin = admin_over(&state);
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, DEFAULT_FILTER, false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, _, text) = export(&admin, Some(&operator_bearer()?), Body::empty()).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    let log = logs.text();
    for hidden in [PATIENT, PATIENT_TAIL, NAMESPACE, EHR_A, EHR_B] {
        assert!(
            !text.contains(hidden),
            "the export carries {hidden}: {text}"
        );
        assert!(!log.contains(hidden), "the log carries {hidden}: {log}");
    }
    let mut finished = Vec::new();
    for line in log.lines() {
        let record: Record = serde_json::from_str(line)?;
        if record.event == FINISHED {
            finished.push(record);
        }
    }
    assert_eq!(1, finished.len(), "{log}");
    let record = finished.first().ok_or("a record")?;
    assert_eq!(Some(ISSUER), record.issuer.as_deref());
    assert_eq!(
        Some(support::claims().sub.as_str()),
        record.subject.as_deref()
    );
    assert_eq!(
        Some(format!("POST {}", admin::EXPORT_IDENTITY_STATE).as_str()),
        record.action.as_deref()
    );
    assert_eq!(Some("200"), record.outcome.as_deref());
    Ok(())
}

#[tokio::test]
async fn an_export_with_a_body_is_refused_400() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = learned_and_bound(dir.path()).await?;
    let (status, _, text) = export(
        &admin_over(&state),
        Some(&operator_bearer()?),
        Body::from(PATIENT),
    )
    .await?;
    assert_eq!(
        (StatusCode::BAD_REQUEST, "body-invalid".to_owned()),
        (status, error_body(&text)?.code),
        "{text}"
    );
    assert!(!text.contains(PATIENT), "the body is not echoed: {text}");
    Ok(())
}

#[tokio::test]
async fn a_gateway_without_a_registry_answers_501() -> TestResult {
    let state = Arc::new(AppState::default());
    let (status, _, text) = export(
        &admin_over(&state),
        Some(&operator_bearer()?),
        Body::empty(),
    )
    .await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{text}");
    Ok(())
}
