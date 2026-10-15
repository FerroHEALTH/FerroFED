// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The revision API on the admin listener: the file load recorded as the
//! first revision, a save based on the newest revision and a stale one
//! refused, the diff, an apply through the reload path, a rollback in one
//! step, every revision recording who, when and why, and the refusals of a
//! configuration `config check` refuses. No specification governs this: our
//! own design.

use ferrofed_registry::revision::{Author, Origin, Outcome, RevisionId};
use ferrofed_server::config::Config;
use http::{Method, StatusCode};

use super::{Applied, Diff, Gateway, HistoryPage, OPERATOR, Standing, TestResult, rest};
use crate::support::{ISSUER, error_body};

/// The operator as a revision records them.
fn the_operator() -> Author {
    Author::Operator {
        issuer: ISSUER.to_owned(),
        subject: OPERATOR.to_owned(),
    }
}

#[tokio::test]
async fn the_file_load_is_the_first_revision_and_runs() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway.get("/admin/config").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let running: Standing = serde_json::from_str(&text)?;
    assert_eq!("managed", running.ui);
    assert_eq!(Some(1), running.head);
    let current = running.current.ok_or("a revision runs")?;
    assert_eq!(RevisionId::FIRST, current.revision);
    assert_eq!(None, current.previous);
    assert_eq!(Author::File, current.applied_by);
    let first = gateway
        .revisions
        .store()
        .revision(RevisionId::FIRST)?
        .ok_or("the file's revision is held")?;
    assert_eq!(Origin::File, first.origin);
    assert_eq!(Author::File, first.author);
    assert!(!first.toml.contains("[server]") && !first.toml.contains("[auth]"));
    assert!(!first.toml.contains("profile") && !first.toml.contains("[config]"));
    assert_eq!(1, gateway.members()?);
    Ok(())
}

#[tokio::test]
async fn a_save_records_who_when_and_why_and_a_stale_base_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let toml = rest(&gateway.two(), "");
    let (status, text) = gateway.save(None, "add member B", &toml).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("revision-stale", error_body(&text)?.code);

    let before = jiff::Timestamp::now();
    let (status, text) = gateway.save(Some(1), "add member B", &toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let saved: ferrofed_registry::revision::Revision = serde_json::from_str(&text)?;
    assert_eq!(RevisionId::new(2), Some(saved.id));
    assert_eq!(Some(RevisionId::FIRST), saved.base);
    assert_eq!(Origin::Operator, saved.origin);
    assert_eq!(the_operator(), saved.author);
    assert_eq!("add member B", saved.reason);
    assert!(saved.at >= before);
    assert_eq!(1, gateway.members()?, "a save applies nothing");

    let (status, text) = gateway.save(Some(1), "a second writer", &toml).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    let refused = error_body(&text)?;
    assert_eq!("revision-stale", refused.code);
    assert!(refused.message.contains("newest revision is 2"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_save_without_a_reason_or_with_a_body_that_does_not_read_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let toml = rest(&gateway.two(), "");
    let (status, text) = gateway.save(Some(1), "  ", &toml).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("configuration-invalid", error_body(&text)?.code);
    let (status, text) = gateway
        .post(
            "/admin/config/revisions",
            Some(String::from(r#"{"base": "synthetic-not-a-number"}"#)),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("body-invalid", error_body(&text)?.code);
    assert!(!text.contains("synthetic-not-a-number"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_diff_names_each_changed_key() -> TestResult {
    let gateway = Gateway::start("")?;
    let toml = rest(&gateway.two(), "\n[eehrxf]\n");
    let (status, text) = gateway.save(Some(1), "add member B", &toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway.get("/admin/config/revisions/1/diff/2").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let diff: Diff = serde_json::from_str(&text)?;
    assert_eq!((1, 2), (diff.from, diff.to));
    let keys: Vec<(&str, &str)> = diff
        .changes
        .iter()
        .map(|change| (change.key.as_str(), change.change.as_str()))
        .collect();
    assert_eq!(
        vec![("eehrxf", "added"), ("registry.document", "changed")],
        keys
    );
    let document = diff
        .changes
        .iter()
        .find(|change| change.key == "registry.document")
        .ok_or("the document changed")?;
    assert!(
        document
            .from
            .as_deref()
            .is_some_and(|from| from.contains("one.toml"))
    );
    assert!(
        document
            .to
            .as_deref()
            .is_some_and(|to| to.contains("two.toml"))
    );
    Ok(())
}

#[tokio::test]
async fn an_apply_runs_the_revision_through_the_reload_path() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway
        .save(Some(1), "add member B", &rest(&gateway.two(), ""))
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/1/apply", None)
        .await?;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "only the newest applies: {text}"
    );
    assert_eq!("revision-stale", error_body(&text)?.code);

    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let applied: Applied = serde_json::from_str(&text)?;
    assert_eq!(RevisionId::new(2), Some(applied.current.revision));
    assert_eq!(Some(RevisionId::FIRST), applied.current.previous);
    assert_eq!(Outcome::Applied, applied.current.outcome);
    assert!(applied.current.needs_restart.is_empty());
    assert_eq!(the_operator(), applied.current.applied_by);
    assert_eq!(2, gateway.members()?, "member B serves now");

    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "it already runs: {text}");
    Ok(())
}

#[tokio::test]
async fn a_restart_only_change_is_marked_pending_restart() -> TestResult {
    let gateway = Gateway::start("")?;
    let toml = rest(&gateway.one(), "").replace(
        "id = \"example-federation\"",
        "id = \"example-federation\"\nbest_effort = false",
    );
    let (status, text) = gateway.save(Some(1), "no best effort", &toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let applied: Applied = serde_json::from_str(&text)?;
    assert_eq!(Outcome::PendingRestart, applied.current.outcome);
    assert_eq!(
        vec![String::from("federation.best_effort")],
        applied.current.needs_restart
    );
    let held = gateway.revisions.store().current()?.ok_or("a mark")?;
    assert_eq!(applied.current, held);
    Ok(())
}

#[tokio::test]
async fn a_rollback_restores_the_previous_revision_in_one_step() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .save(Some(1), "add member B", &rest(&gateway.two(), ""))
        .await?;
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(2, gateway.members()?);

    let stale = r#"{"current": 1, "reason": "back to member A alone"}"#;
    let (status, text) = gateway
        .post("/admin/config/rollback", Some(stale.to_owned()))
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("revision-stale", error_body(&text)?.code);

    let body = r#"{"current": 2, "reason": "back to member A alone"}"#;
    let (status, text) = gateway
        .post("/admin/config/rollback", Some(body.to_owned()))
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let rolled: Applied = serde_json::from_str(&text)?;
    assert_eq!(RevisionId::new(3), Some(rolled.revision.id));
    assert_eq!(Origin::Rollback, rolled.revision.origin);
    assert_eq!(Some(RevisionId::FIRST), rolled.revision.restores);
    assert_eq!("back to member A alone", rolled.revision.reason);
    assert_eq!(the_operator(), rolled.revision.author);
    let first = gateway
        .revisions
        .store()
        .revision(RevisionId::FIRST)?
        .ok_or("held")?;
    assert_eq!(first.toml, rolled.revision.toml);
    assert_eq!(RevisionId::new(2), rolled.current.previous);
    assert_eq!(1, gateway.members()?, "member B is gone again");
    Ok(())
}

#[tokio::test]
async fn a_rollback_with_nothing_before_it_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let body = r#"{"current": 1, "reason": "nothing to go back to"}"#;
    let (status, text) = gateway
        .post("/admin/config/rollback", Some(body.to_owned()))
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("rollback-unavailable", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn a_configuration_config_check_refuses_is_refused_with_its_error() -> TestResult {
    let gateway = Gateway::start("")?;
    let toml =
        rest(&gateway.one(), "").replace("per_node_timeout_ms = 5000", "per_node_timeout_ms = 0");
    let (status, text) = gateway.save(Some(1), "a zero budget", &toml).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let refused = error_body(&text)?;
    assert_eq!("configuration-invalid", refused.code);
    let file = format!(
        "profile = \"development\"\n{}{toml}",
        crate::support::auth_toml()?
    );
    let check = Config::from_sources(Some(&file), &std::collections::BTreeMap::new())?
        .resolve()
        .err()
        .ok_or("config check refuses it too")?;
    assert!(
        refused.message.contains(&check.to_string()),
        "{} carries {check}",
        refused.message
    );
    assert_eq!(
        Some(1),
        gateway.revisions.store().head()?.map(RevisionId::get),
        "nothing is stored"
    );
    Ok(())
}

#[tokio::test]
async fn a_revision_never_sets_the_bootstrap_core() -> TestResult {
    let gateway = Gateway::start("")?;
    for core in [
        "profile = \"production\"\n",
        "[server]\nlisten = \"0.0.0.0:8080\"\n",
        "[metrics]\nlisten = \"0.0.0.0:9100\"\n",
        "[auth]\naudience = \"urn:example:other\"\n",
        "[config]\nui = \"off\"\n",
    ] {
        let toml = format!("{core}{}", rest(&gateway.one(), ""));
        let (status, text) = gateway.save(Some(1), "touch the core", &toml).await?;
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{core}: {text}");
        let refused = error_body(&text)?;
        assert_eq!("configuration-invalid", refused.code);
        assert!(refused.message.contains("bootstrap core"), "{text}");
    }
    Ok(())
}

#[tokio::test]
async fn the_history_reads_newest_first_and_one_revision_by_number() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .save(Some(1), "add member B", &rest(&gateway.two(), ""))
        .await?;
    let (status, text) = gateway.get("/admin/config/revisions?limit=1").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let page: HistoryPage = serde_json::from_str(&text)?;
    assert_eq!((0, 2), (page.offset, page.total));
    assert_eq!(
        vec![RevisionId::new(2)],
        page.items
            .iter()
            .map(|item| Some(item.id))
            .collect::<Vec<_>>()
    );
    let (status, text) = gateway.get("/admin/config/revisions/1").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for unknown in ["/admin/config/revisions/9", "/admin/config/revisions/x"] {
        let (status, text) = gateway.get(unknown).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{unknown}: {text}");
        assert_eq!("revision-unknown", error_body(&text)?.code);
    }
    let (status, text) = gateway.get("/admin/config/revisions?limit=0").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn every_route_needs_the_operator_scope() -> TestResult {
    let gateway = Gateway::start("")?;
    let client = crate::support::bearer()?;
    for (method, path) in [
        (Method::GET, "/admin/config"),
        (Method::GET, "/admin/config/revisions"),
        (Method::GET, "/admin/config/revisions/1"),
        (Method::GET, "/admin/config/revisions/1/diff/1"),
        (Method::POST, "/admin/config/revisions"),
        (Method::POST, "/admin/config/revisions/1/apply"),
        (Method::POST, "/admin/config/rollback"),
    ] {
        let (status, text) = gateway.send((method.clone(), path), None, None).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status, "{method} {path}: {text}");
        let (status, text) = gateway
            .send((method.clone(), path), None, Some(&client))
            .await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{method} {path}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn an_apply_takes_no_body() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .save(Some(1), "add member B", &rest(&gateway.two(), ""))
        .await?;
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", Some(String::from("{}")))
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(1, gateway.members()?);
    Ok(())
}

#[tokio::test]
async fn a_reload_keeps_the_running_revision_until_the_file_changes() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .save(Some(1), "add member B", &rest(&gateway.two(), ""))
        .await?;
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    gateway.reloader.reload()?;
    assert_eq!(
        2,
        gateway.members()?,
        "the file is unchanged, revision 2 runs"
    );
    assert_eq!(
        Some(2),
        gateway.revisions.store().head()?.map(RevisionId::get)
    );

    let text = std::fs::read_to_string(&gateway.config)?;
    std::fs::write(
        &gateway.config,
        text.replace(
            "id = \"example-federation\"",
            "id = \"example-federation\"\nbinding_ttl_ms = 600000",
        ),
    )?;
    gateway.reloader.reload()?;
    let current = gateway.revisions.store().current()?.ok_or("a mark")?;
    assert_eq!(RevisionId::new(3), Some(current.revision));
    assert_eq!(RevisionId::new(2), current.previous);
    let recorded = gateway
        .revisions
        .store()
        .revision(current.revision)?
        .ok_or("held")?;
    assert_eq!(Origin::File, recorded.origin);
    assert_eq!(1, gateway.members()?, "the file's registry runs again");
    Ok(())
}

#[tokio::test]
async fn a_file_change_that_needs_a_restart_is_marked_pending_restart_on_sighup() -> TestResult {
    let gateway = Gateway::start("")?;
    let text = std::fs::read_to_string(&gateway.config)?;
    std::fs::write(
        &gateway.config,
        text.replace(
            "id = \"example-federation\"",
            "id = \"example-federation\"\nbest_effort = false",
        ),
    )?;
    let applied = gateway.reloader.reload()?;
    assert_eq!(vec!["federation.best_effort"], applied.needs_restart);
    let current = gateway.revisions.store().current()?.ok_or("a mark")?;
    assert_eq!(RevisionId::new(2), Some(current.revision));
    let recorded = gateway
        .revisions
        .store()
        .revision(current.revision)?
        .ok_or("held")?;
    assert_eq!(Origin::File, recorded.origin);
    assert_eq!(Outcome::PendingRestart, current.outcome);
    assert_eq!(
        vec![String::from("federation.best_effort")],
        current.needs_restart
    );
    assert_eq!(Author::File, current.applied_by);
    Ok(())
}
