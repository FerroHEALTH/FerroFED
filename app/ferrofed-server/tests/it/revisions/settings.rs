// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration and the save of settings under
//! `config.ui = "managed"`: each setting with the revision it comes from,
//! a saved setting applied as any revision, a setting that waits for the
//! next start named with the value that start takes, and only an editable
//! setting saved, a refused value never echoed.

use ferrofed_registry::revision::{Revision, RevisionId};
use ferrofed_registry::running::{RunningConfig, Source};
use http::{Method, StatusCode};

use super::{Gateway, TestResult, operator};

/// The running configuration the gateway answers.
async fn running(gateway: &Gateway) -> Result<RunningConfig, Box<dyn std::error::Error>> {
    let (status, body) = gateway.get("/admin/config/running").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(serde_json::from_str(&body)?)
}

/// Saves `settings` on `base` as the operator.
async fn save(
    gateway: &Gateway,
    base: u64,
    settings: &[(&str, &str)],
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    let settings: std::collections::BTreeMap<&str, &str> = settings.iter().copied().collect();
    let body = serde_json::json!({
        "base": base,
        "reason": "synthetic settings change",
        "settings": settings,
    });
    gateway
        .send(
            (Method::PUT, "/admin/config/settings"),
            Some(body.to_string()),
            Some(&operator()?),
        )
        .await
}

#[tokio::test]
async fn the_running_configuration_names_the_revision_each_setting_comes_from() -> TestResult {
    let gateway = Gateway::start("")?;
    let document = running(&gateway).await?;
    assert_eq!("managed", document.ui);
    assert_eq!(Some(1), document.revision.map(RevisionId::get));
    let per_node = document
        .setting("federation.per_node_timeout_ms")
        .ok_or("listed")?;
    assert_eq!(Some(toml::Value::Integer(5000)), per_node.value);
    assert_eq!(Source::Revision, per_node.source);
    assert!(per_node.restart);
    assert!(per_node.editable);
    let window = document
        .setting("federation.max_offset_window")
        .ok_or("listed")?;
    assert_eq!(Source::Default, window.source);
    assert_eq!(Some(toml::Value::Integer(1000)), window.value);

    let (status, _) = gateway
        .send((Method::GET, "/admin/config/running"), None, None)
        .await?;
    assert_ne!(StatusCode::OK, status, "the operator scope is required");
    Ok(())
}

#[tokio::test]
async fn a_saved_setting_applies_as_a_revision_and_a_restart_setting_names_the_next_start()
-> TestResult {
    let gateway = Gateway::start("")?;
    let (status, body) = save(
        &gateway,
        1,
        &[
            ("federation.per_node_timeout_ms", "5500"),
            ("federation.consent.disclose", "false"),
        ],
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let saved: Revision = serde_json::from_str(&body)?;
    assert_eq!(2, saved.id.get());
    let before = running(&gateway).await?;
    assert_eq!(
        Some(1),
        before.revision.map(RevisionId::get),
        "saved, not applied"
    );

    let (status, body) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let after = running(&gateway).await?;
    assert_eq!(Some(2), after.revision.map(RevisionId::get));
    let per_node = after
        .setting("federation.per_node_timeout_ms")
        .ok_or("listed")?;
    assert_eq!(
        Some(toml::Value::Integer(5000)),
        per_node.value,
        "the replica runs the boot value until it restarts"
    );
    assert_eq!(Some(toml::Value::Integer(5500)), per_node.next);
    let disclose = after
        .setting("federation.consent.disclose")
        .ok_or("listed")?;
    assert_eq!(Some(toml::Value::Boolean(false)), disclose.value);
    assert_eq!(Source::Revision, disclose.source);
    assert_eq!(None, disclose.next);
    Ok(())
}

#[tokio::test]
async fn only_an_editable_setting_is_saved_and_a_refused_value_is_never_echoed() -> TestResult {
    let gateway = Gateway::start("")?;
    for settings in [
        vec![("stored_queries.url", "postgres://synthetic-secret-93@db/x")],
        vec![("access_log.templates", "{}")],
        vec![("server.listen", "0.0.0.0:8080")],
        vec![("federation.unknown_key", "1")],
        vec![],
    ] {
        let (status, body) = save(&gateway, 1, &settings).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{settings:?}: {body}");
        assert!(!body.contains("synthetic-secret-93"), "{body}");
    }
    let (status, body) = save(
        &gateway,
        1,
        &[("federation.per_node_timeout_ms", "synthetic-not-a-number")],
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let (status, body) = save(&gateway, 1, &[("federation.best_effort", "false")]).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let (status, body) = save(&gateway, 1, &[("federation.best_effort", "true")]).await?;
    assert_eq!(StatusCode::CONFLICT, status, "a stale base: {body}");
    Ok(())
}
