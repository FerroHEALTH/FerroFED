// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator console against a running gateway over a stub node: both
//! listen on loopback, the console reaches the gateway over HTTP as the
//! signed-in operator, and each view renders from the gateway's own answers.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::incident::{Detection, Incident};
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::session::SignedIn;
use secrecy::SecretString;
use tokio::net::TcpListener;

/// The issuer the gateway trusts and the operator's token comes from.
pub(crate) const ISSUER: &str = "https://issuer.example.test";

/// The audience the gateway is known by at [`ISSUER`].
pub(crate) const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// The scope the gateway admits operators with.
pub(crate) const OPERATOR_SCOPE: &str = "ferrofed:operator";

/// A synthetic `ehr_id` an incident names.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The registry of one node at `node`, with one registered
/// `creating_system_id`.
fn registry(node: &str) -> String {
    format!(
        "[[organisation]]\nid = \"org-a\"\n\n\
         [[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n\
         [[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{node}/openehr\"\n\
         connection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n\n\
         [[creating_system]]\ncreating_system_id = \"legacy-a.example.org\"\nendpoint = \"node-a-pub\"\n"
    )
}

/// The gateway's configuration over the registry `document`, trusting
/// `issuer` and admitting operators with [`OPERATOR_SCOPE`], signing with the
/// key in `key_file`.
fn gateway_configuration(
    document: &std::path::Path,
    issuer: &Issuer,
    key_file: &std::path::Path,
) -> Result<String, Box<dyn Error>> {
    let document = toml::Value::String(document.display().to_string());
    let key_file = toml::Value::String(key_file.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\n\
         [signing]\nkey_file = {key_file}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n\n\
         [auth]\naudience = \"{AUDIENCE}\"\n\n[[auth.issuer]]\nissuer = \"{ISSUER}\"\n\
         jwks = '{}'\noperator_scope = \"{OPERATOR_SCOPE}\"\n",
        issuer.jwks_json()?
    ))
}

/// A running gateway over one stub node and a running console pointed at
/// it, with one operator signed in.
struct Running {
    /// The stub node, kept so it outlives the gateway.
    _node: Server,
    /// The temporary directory the registry and the signing key live in.
    _dir: tempfile::TempDir,
    /// The console's base URL.
    console: String,
    /// The `Cookie` value of the signed-in operator's session.
    cookie: String,
}

impl Running {
    /// Starts the stub node, the gateway and the console.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let node = Server::start().await;
        let dir = tempfile::tempdir()?;
        let document = dir.path().join("registry.toml");
        std::fs::write(&document, registry(&node.uri()))?;
        let key_file = dir.path().join("signing-key.pem");
        std::fs::write(&key_file, ferrofed_testkit::oauth::es384_pem()?)?;
        let issuer = Issuer::new(ISSUER)?;
        let text = gateway_configuration(&document, &issuer, &key_file)?;
        let settings =
            ferrofed_server::config::Config::from_sources(Some(&text), &BTreeMap::new())?
                .resolve()?;
        let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
        let gateway = TcpListener::bind("127.0.0.1:0").await?;
        let gateway_address = gateway.local_addr()?;
        tokio::spawn(ferrofed_server::serve_until(
            gateway,
            ferrofed_server::router(
                Arc::new(AppState::with_federation(federation)),
                &settings.server,
            ),
            Duration::from_secs(1),
            std::future::pending(),
        ));

        let console_settings = ferrofed_viewer::config::Config::from_sources(
            Some(&format!(
                "[gateway]\nbase_url = \"http://{gateway_address}/\"\n\n[session]\nsecure_cookie = false\n"
            )),
            &BTreeMap::new(),
        )?
        .resolve()?;
        let state = ferrofed_viewer::server::ViewerState::new(console_settings)?;
        let mut claims = Claims::new(ISSUER, AUDIENCE);
        let scope = claims.scope.take().unwrap_or_default();
        claims.scope = Some(format!("{scope} {OPERATOR_SCOPE}"));
        let session = state.sessions().establish(SignedIn {
            access_token: SecretString::from(issuer.mint(&claims)?),
            expires_in: None,
            id_token: None,
        })?;
        let console = TcpListener::bind("127.0.0.1:0").await?;
        let console_address = console.local_addr()?;
        tokio::spawn(ferrofed_server::serve_until(
            console,
            ferrofed_viewer::server::router(state),
            Duration::from_secs(1),
            std::future::pending(),
        ));
        Ok(Self {
            _node: node,
            _dir: dir,
            console: format!("http://{console_address}"),
            cookie: format!("{}={}", ferrofed_viewer::session::COOKIE, session.as_str()),
        })
    }

    /// The status and the page of the view at `path`, as the operator.
    async fn view(&self, path: &str) -> Result<(reqwest::StatusCode, String), Box<dyn Error>> {
        let response = reqwest::Client::new()
            .get(format!("{}{path}", self.console))
            .header("cookie", &self.cookie)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.text().await?))
    }
}

/// Asserts that `page` rendered a view, never a refusal of it.
fn rendered(path: &str, page: &str) -> Result<(), Box<dyn Error>> {
    if page.contains("role=\"alert\"") {
        return Err(format!("{path} rendered a refusal: {page}").into());
    }
    Ok(())
}

#[tokio::test]
async fn the_members_view_renders_from_the_running_gateway() -> Result<(), Box<dyn Error>> {
    let running = Running::start().await?;
    let (status, page) = running.view("/members").await?;
    assert_eq!(reqwest::StatusCode::OK, status, "{page}");
    rendered("/members", &page)?;
    assert!(page.contains("<th scope=\"row\">node-a-pub</th>"), "{page}");
    assert!(page.contains("<td>org-a</td>"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_integrity_view_renders_the_incidents_and_the_routing_table()
-> Result<(), Box<dyn Error>> {
    let running = Running::start().await?;
    Incident::EhrIdCollision {
        ehr_id: EHR_ID.parse()?,
        detection: Detection::Index,
        claimants: vec!["node-a-pub".parse()?],
    }
    .emit();
    let (status, page) = running.view("/integrity").await?;
    assert_eq!(reqwest::StatusCode::OK, status, "{page}");
    rendered("/integrity", &page)?;
    assert!(page.contains("EhrIdCollision"), "{page}");
    assert!(page.contains(EHR_ID), "{page}");
    assert!(
        page.contains("<th scope=\"row\">legacy-a.example.org</th>"),
        "{page}"
    );
    assert!(page.contains("<td>registered</td>"), "{page}");
    assert!(page.contains("Rows 1 to 2 of 2."), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_stored_query_view_renders_what_the_gateway_holds() -> Result<(), Box<dyn Error>> {
    let running = Running::start().await?;
    let (status, page) = running.view("/stored-queries").await?;
    assert_eq!(reqwest::StatusCode::OK, status, "{page}");
    rendered("/stored-queries", &page)?;
    assert!(
        page.contains("The gateway holds no stored query."),
        "{page}"
    );
    Ok(())
}

#[tokio::test]
async fn the_self_description_view_renders_the_gateways_options() -> Result<(), Box<dyn Error>> {
    let running = Running::start().await?;
    let (status, page) = running.view("/federation").await?;
    assert_eq!(reqwest::StatusCode::OK, status, "{page}");
    rendered("/federation", &page)?;
    assert!(page.contains("example-federation"), "{page}");
    Ok(())
}

// A page whose content comes from the gateway is whole in the HTML the
// console sends, every time: nothing waits in a `<template>` for an inline
// script to move it into place, so the page reads with no script at all.
#[tokio::test(flavor = "multi_thread")]
async fn every_gateway_page_is_whole_in_the_html_the_console_sends() -> Result<(), Box<dyn Error>> {
    let running = Running::start().await?;
    for (path, content) in [
        ("/members", "<th scope=\"row\">node-a-pub</th>"),
        (
            "/integrity",
            "<caption>The creating_system_id routing table</caption>",
        ),
        ("/stored-queries", "The gateway holds no stored query."),
        ("/federation", "example-federation"),
        (
            "/query",
            r#"<button type="submit" disabled>Run the query</button>"#,
        ),
    ] {
        for round in 1..=10 {
            let (status, page) = running.view(path).await?;
            assert_eq!(reqwest::StatusCode::OK, status, "{path} #{round}: {page}");
            let document = page
                .split_once("</html>")
                .map_or(page.as_str(), |(document, _after)| document);
            assert!(document.contains(content), "{path} #{round}: {page}");
            assert!(!page.contains("<template id="), "{path} #{round}: {page}");
            assert!(
                !page.contains("createTreeWalker"),
                "{path} #{round}: {page}"
            );
        }
    }
    Ok(())
}
