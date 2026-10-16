// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the operator console manages: each member and stored
//! query is named with its owner, a member the registry document provides
//! cannot be changed or removed from the console, a console member is saved
//! as a revision, previewed with the diff, applied and rolled back, it
//! survives a reload of the file until the file provides the same member,
//! the file never sets one, and a stored-query version is stored once
//! (§12.7, N44). No specification governs the ownership: our own design.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use ferrofed_registry::console::{ConsoleMember, ConsoleState, Owner, QueryRegistry, Resources};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::operator::{Page, StoredVersion};
use ferrofed_registry::revision::{Origin, Revision, RevisionId};
use ferrofed_server::config::Config;
use http::{Method, Request, StatusCode, header};
use openehr_its::rest::generated::definition::StoredQuery;

use super::{Diff, Gateway, TestResult, member, operator};
use crate::support::error_body;

/// The body of a member's save of `member` for `reason` on `base`.
fn save_body(base: u64, reason: &str, member: &str) -> String {
    format!(r#"{{"base": {base}, "reason": "{reason}", "member": {member}}}"#)
}

/// Console member `name`, at an address nothing listens on, as JSON.
fn console_member(name: &str) -> String {
    format!(
        r#"{{"organisation": {{"id": "org-{name}", "name": "Organisation {name}"}},
            "node": {{"id": "node-{name}", "system_id": "cdr-{name}.example.org"}},
            "endpoints": [{{"id": "node-{name}-pub", "url": "http://127.0.0.1:9/{name}",
                            "connection_type": "openehr-rest-query"}}]}}"#
    )
}

impl Gateway {
    /// `PUT path` with `body` as the operator.
    async fn put(
        &self,
        path: &str,
        body: String,
    ) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
        self.send((Method::PUT, path), Some(body), Some(&operator()?))
            .await
    }

    /// The resources the admin listener reports.
    async fn resources(&self) -> Result<Resources, Box<dyn std::error::Error>> {
        let (status, text) = self.get("/admin/config/resources").await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        Ok(serde_json::from_str(&text)?)
    }

    /// Whether the running registry holds `node` as the console's.
    fn runs_console(&self, node: &str) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self
            .state
            .federation()
            .ok_or("a registry is configured")?
            .snapshot()
            .is_console(&node.parse::<NodeId>()?))
    }
}

/// The owner and the state the report gives each member, by `node_id`.
fn owners(resources: &Resources) -> Vec<(String, Owner, Option<ConsoleState>)> {
    resources
        .members
        .iter()
        .map(|member| (member.node.as_str().to_owned(), member.owner, member.state))
        .collect()
}

#[tokio::test]
async fn the_report_names_the_owner_of_every_member_and_stored_query() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = toml::Value::String(dir.path().join("q.redb").display().to_string());
    let gateway = Gateway::start(&format!("\n[stored_queries]\npath = {store}\n"))?;
    let resources = gateway.resources().await?;
    assert_eq!(Some(RevisionId::FIRST), resources.running);
    assert_eq!(Some(RevisionId::FIRST), resources.head);
    assert_eq!(QueryRegistry::Writable, resources.stored_query_registry);
    assert_eq!(
        vec![(String::from("node-a"), Owner::File, None)],
        owners(&resources)
    );
    assert!(!resources.stored_queries.is_empty());
    assert!(
        resources
            .stored_queries
            .iter()
            .all(|query| query.owner == Owner::Gateway),
        "only the reserved namespace is held: {:?}",
        resources.stored_queries
    );
    Ok(())
}

/// The console report and the operator listing read one rule, so a version
/// the console names as the store's is the one the listing calls writable.
#[tokio::test]
async fn the_console_owner_and_the_operator_read_only_flag_agree() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = toml::Value::String(dir.path().join("q.redb").display().to_string());
    let gateway = Gateway::start(&format!("\n[stored_queries]\npath = {store}\n"))?;
    let body = r#"{"aql": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"}"#;
    let (status, text) = gateway
        .put(
            "/admin/config/stored-queries/org.example::compositions/1.0.0",
            body.to_owned(),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let owners: BTreeMap<(String, String), Owner> = gateway
        .resources()
        .await?
        .stored_queries
        .into_iter()
        .map(|query| ((query.name, query.version), query.owner))
        .collect();

    let client = ferrofed_server::router(
        Arc::clone(&gateway.state),
        &crate::facade::settings_with_room(),
    );
    let request = Request::get("/operator/stored-queries")
        .header(header::AUTHORIZATION, crate::support::operator_bearer()?)
        .body(Body::empty())?;
    let (status, text) = crate::support::call(client, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let page: Page<StoredVersion<StoredQuery>> = serde_json::from_str(&text)?;
    let flags: BTreeMap<(String, String), bool> = page
        .items
        .into_iter()
        .map(|entry| {
            (
                (entry.definition.name, entry.definition.version),
                entry.read_only,
            )
        })
        .collect();

    assert_eq!(
        owners.keys().collect::<Vec<_>>(),
        flags.keys().collect::<Vec<_>>(),
        "both name every held version"
    );
    for (version, owner) in &owners {
        assert_eq!(
            Some(&(*owner != Owner::Store)),
            flags.get(version),
            "{version:?} is {owner:?}"
        );
    }
    assert!(owners.values().any(|owner| *owner == Owner::Store));
    assert!(owners.values().any(|owner| *owner == Owner::Gateway));
    Ok(())
}

#[tokio::test]
async fn the_report_says_when_no_stored_query_registry_is_offered() -> TestResult {
    let gateway = Gateway::start("")?;
    let resources = gateway.resources().await?;
    assert_eq!(QueryRegistry::Absent, resources.stored_query_registry);
    assert!(resources.stored_queries.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_member_the_file_provides_cannot_be_changed_or_removed_from_the_console() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway
        .put(
            "/admin/config/members/node-a",
            save_body(1, "take over A", &console_member("a")),
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);

    let sharing = console_member("z").replace("node-z-pub", "node-a-pub");
    let (status, text) = gateway
        .put(
            "/admin/config/members/node-z",
            save_body(1, "share an endpoint", &sharing),
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);

    let removal = r#"{"base": 1, "reason": "remove A"}"#.to_owned();
    let (status, text) = gateway
        .post("/admin/config/members/node-a/remove", Some(removal.clone()))
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);

    let (status, text) = gateway
        .post("/admin/config/members/node-z/remove", Some(removal))
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("resource-unknown", error_body(&text)?.code);

    assert_eq!(Some(RevisionId::FIRST), gateway.revisions.store().head()?);
    Ok(())
}

#[tokio::test]
async fn a_change_without_the_operators_token_is_refused_before_anything_is_read() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, _) = gateway
        .send(
            (Method::PUT, "/admin/config/members/node-c"),
            Some(save_body(1, "add C", &console_member("c"))),
            None,
        )
        .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    let (status, _) = gateway
        .send((Method::GET, "/admin/config/resources"), None, None)
        .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(Some(RevisionId::FIRST), gateway.revisions.store().head()?);
    Ok(())
}

#[tokio::test]
async fn a_console_member_is_saved_previewed_applied_and_rolled_back() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway
        .put(
            "/admin/config/members/node-c",
            save_body(1, "add C", &console_member("c")),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let saved: Revision = serde_json::from_str(&text)?;
    assert_eq!(RevisionId::new(2), Some(saved.id));
    assert_eq!(1, gateway.members()?, "a save applies nothing");
    let pending = gateway.resources().await?;
    assert!(owners(&pending).contains(&(
        String::from("node-c"),
        Owner::Console,
        Some(ConsoleState::Pending)
    )));

    let (status, text) = gateway.get("/admin/config/revisions/1/diff/2").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let diff: Diff = serde_json::from_str(&text)?;
    let added = diff
        .changes
        .iter()
        .find(|change| change.key == "registry.members[0].node.id")
        .ok_or("the diff names the member's node")?;
    assert_eq!("added", added.change);
    assert_eq!(Some("\"node-c\""), added.to.as_deref());

    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(2, gateway.members()?);
    assert!(gateway.runs_console("node-c")?);
    assert!(!gateway.runs_console("node-a")?);
    let running = gateway.resources().await?;
    assert_eq!(
        vec![
            (String::from("node-a"), Owner::File, None),
            (
                String::from("node-c"),
                Owner::Console,
                Some(ConsoleState::Running)
            ),
        ],
        owners(&running)
    );

    let (status, text) = gateway
        .post(
            "/admin/config/rollback",
            Some(r#"{"current": 2, "reason": "C was premature"}"#.to_owned()),
        )
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, gateway.members()?);
    assert_eq!(
        vec![(String::from("node-a"), Owner::File, None)],
        owners(&gateway.resources().await?)
    );
    Ok(())
}

#[tokio::test]
async fn a_console_member_is_edited_and_removed_through_revisions() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .put(
            "/admin/config/members/node-c",
            save_body(1, "add C", &console_member("c")),
        )
        .await?;
    let renamed = console_member("c").replace("Organisation c", r#"Organisation <c> & \"sons\""#);
    let (status, text) = gateway
        .put(
            "/admin/config/members/node-c",
            save_body(2, "rename C", &renamed),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let head = gateway.resources().await?;
    let member = head
        .members
        .iter()
        .find(|member| member.owner == Owner::Console)
        .and_then(|member| member.member.clone())
        .ok_or("the console member is reported whole")?;
    assert_eq!(
        Some("Organisation <c> & \"sons\""),
        member.organisation.name.as_deref()
    );
    assert_eq!(
        1,
        head.members
            .iter()
            .filter(|member| member.owner == Owner::Console)
            .count(),
        "an edit replaces the member of its node_id"
    );

    let (status, text) = gateway
        .post(
            "/admin/config/members/node-c/remove",
            Some(r#"{"base": 3, "reason": "remove C"}"#.to_owned()),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let removed: Revision = serde_json::from_str(&text)?;
    assert!(!removed.toml.contains("members"), "{}", removed.toml);
    Ok(())
}

// NOTE: no specification governs this: our own design; the operator console
// shows a configuration-invalid message, so a URL credential never reaches it.
#[tokio::test]
async fn a_refused_member_url_never_shows_its_credential() -> TestResult {
    let gateway = Gateway::start("")?;
    for url in [
        "http://127.0.0.1:9/c?token=s3cret-q",
        "http://127.0.0.1:9/c#s3cret-q",
        "http://op:s3cret-q@127.0.0.1:9/c",
    ] {
        let member = console_member("c").replace("http://127.0.0.1:9/c", url);
        let (status, text) = gateway
            .put(
                "/admin/config/members/node-c",
                save_body(1, "add C", &member),
            )
            .await?;
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{url}: {text}");
        assert_eq!("configuration-invalid", error_body(&text)?.code);
        assert!(!text.contains("s3cret-q"), "{url}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn a_console_member_survives_a_file_reload_until_the_file_provides_it() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .put(
            "/admin/config/members/node-c",
            save_body(1, "add C", &console_member("c")),
        )
        .await?;
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");

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
    let recorded = gateway
        .revisions
        .store()
        .revision(current.revision)?
        .ok_or("held")?;
    assert_eq!(Origin::File, recorded.origin);
    assert!(recorded.toml.contains("node-c"), "{}", recorded.toml);
    assert_eq!(2, gateway.members()?, "the console member survives");
    assert!(gateway.runs_console("node-c")?);

    std::fs::write(gateway.one(), member("a") + &member("c"))?;
    gateway.reloader.reload()?;
    assert_eq!(2, gateway.members()?);
    assert!(
        !gateway.runs_console("node-c")?,
        "the document's member C runs in place of the console's"
    );
    let resources = gateway.resources().await?;
    assert_eq!(
        vec![
            (String::from("node-a"), Owner::File, None),
            (String::from("node-c"), Owner::File, None),
            (
                String::from("node-c"),
                Owner::Console,
                Some(ConsoleState::Shadowed)
            ),
        ],
        owners(&resources),
        "the console's copy is reported, never dropped"
    );
    Ok(())
}

#[tokio::test]
async fn the_configuration_file_never_sets_a_console_member() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("ferrofed.toml");
    std::fs::write(
        &file,
        format!(
            "[registry]\ndocument = \"/nowhere.toml\"\n\n[[registry.members]]\n{}",
            "organisation = { id = \"org-c\" }\nnode = { id = \"node-c\", system_id = \"cdr-c.example.org\" }\nendpoints = []\n"
        ),
    )?;
    let refused = Config::load_with_tree(Some(&file))
        .err()
        .ok_or("the file is refused")?;
    assert!(
        refused.to_string().contains("registry.members"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn a_revision_with_a_console_member_round_trips_through_the_store() -> TestResult {
    let gateway = Gateway::start("")?;
    gateway
        .put(
            "/admin/config/members/node-c",
            save_body(1, "add C", &console_member("c")),
        )
        .await?;
    let (status, text) = gateway.get("/admin/config/revisions/2").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let held: Revision = serde_json::from_str(&text)?;
    let table: toml::Table = toml::from_str(&held.toml)?;
    let members: Vec<ConsoleMember> = table
        .get("registry")
        .and_then(|registry| registry.get("members"))
        .cloned()
        .ok_or("the revision holds the member")?
        .try_into()?;
    let expected: ConsoleMember = serde_json::from_str(&console_member("c"))?;
    assert_eq!(vec![expected], members);

    let (status, text) = gateway.save(Some(2), "the same again", &held.toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway.get("/admin/config/revisions/2/diff/3").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let diff: Diff = serde_json::from_str(&text)?;
    assert!(diff.changes.is_empty(), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_stored_query_version_is_stored_once_and_never_in_the_reserved_namespace() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = toml::Value::String(dir.path().join("q.redb").display().to_string());
    let gateway = Gateway::start(&format!("\n[stored_queries]\npath = {store}\n"))?;
    let body = r#"{"aql": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"}"#;
    let path = "/admin/config/stored-queries/org.example::compositions/1.0.0";
    let (status, text) = gateway.put(path, body.to_owned()).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway.put(path, body.to_owned()).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("stored-query-held", error_body(&text)?.code);

    let reserved = format!(
        "/admin/config/stored-queries/{}::mine/1.0.0",
        ferrofed_eehrxf::reserved::NAMESPACE
    );
    let (status, text) = gateway.put(&reserved, body.to_owned()).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("stored-query-reserved", error_body(&text)?.code);

    let literal = r#"{"aql": "SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_status/subject/external_ref/id/value = '12345'"}"#;
    let (status, text) = gateway
        .put(
            "/admin/config/stored-queries/org.example::literal/1.0.0",
            literal.to_owned(),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");

    let resources = gateway.resources().await?;
    let stored: Vec<(&str, &str, Owner)> = resources
        .stored_queries
        .iter()
        .filter(|query| query.owner != Owner::Gateway)
        .map(|query| (query.name.as_str(), query.version.as_str(), query.owner))
        .collect();
    assert_eq!(
        vec![("org.example::compositions", "1.0.0", Owner::Store)],
        stored
    );
    Ok(())
}
