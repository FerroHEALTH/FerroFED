// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The production guide's Keycloak recipe, held against a pinned Keycloak:
//! the recipe's `kcadm.sh` commands, its protocol mapper files and the
//! gateway's `[auth]` table are read from the book page and applied as the
//! page prints them, and the tokens Keycloak then issues get a federated
//! answer over the two FerroEHR nodes (§13.1, N25; RFC 9068).
//!
//! A user signed in to the clinical application and the reporting service's
//! client-credentials grant each get a `200`. With the client's
//! `access.token.header.type.rfc9068` attribute off, the same user's token
//! is refused `401` for its type (RFC 9068 §4). Keycloak is an issuer here,
//! never the oracle: a Keycloak release that changes what the recipe
//! produces fails this test, and the page is what gets fixed.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::containers::keycloak::recipe::{self, Recipe};
use ferrofed_testkit::containers::keycloak::{self, CLINICAL_APP, Keycloak, REPORTING_SERVICE};
use ferrofed_testkit::containers::{self, TwoNodes};
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{HeaderMap, StatusCode};

use crate::e2e::scenario::seed_both;
use crate::e2e::{
    Answer, TestResult, assert_no_patient_identifier_on_the_wire, dev_resolver, patient_query,
    query, registry_document,
};
use crate::support::send_as_is;

/// The gateway over node A and node B, resolving the patient through the
/// development cross-reference and trusting `idp` with the page's `[auth]`
/// table, its issuer and key set location moved to the Keycloak the test
/// started.
fn gateway(
    dir: &Path,
    nodes: &TwoNodes,
    recipe: &Recipe,
    idp: &Keycloak,
) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry_document(&nodes.a, &nodes.b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "{}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{}",
        dev_resolver(),
        recipe.auth_against(idp.origin())
    );
    let resolved =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&resolved)?;
    Ok(ferrofed_server::router(Arc::new(state), &resolved.server))
}

/// Sends the patient query with `token` and reads the answer.
async fn ask(app: Router, token: &str) -> Result<(StatusCode, HeaderMap, String), Box<dyn Error>> {
    let mut request = query(&patient_query())?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    let response = send_as_is(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024).await?;
    Ok((status, headers, String::from_utf8(body.to_vec())?))
}

#[tokio::test]
async fn the_keycloak_recipe_of_the_production_guide_admits_a_user_and_a_service() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let recipe = Recipe::from_page(&std::fs::read_to_string(recipe::PAGE)?)?;
    let (nodes, idp) = tokio::join!(
        Box::pin(containers::two_nodes()),
        Box::pin(keycloak::keycloak())
    );
    let (nodes, mut idp) = (nodes?, idp?);
    seed_both(&nodes).await?;
    let secrets = idp.apply(&recipe).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &nodes, &recipe, &idp)?;

    let user = idp.user_token(&secrets.clinical_app).await?;
    let service = idp
        .client_credentials_token(REPORTING_SERVICE, &secrets.reporting_service)
        .await?;
    for (caller, token) in [
        ("the signed-in user", &user),
        ("the reporting service", &service),
    ] {
        let (status, _, text) = ask(app.clone(), token).await?;
        assert_eq!(StatusCode::OK, status, "{caller}: {text}");
        let answer: Answer = serde_json::from_str(&text)?;
        assert!(
            answer.meta.federation.complete,
            "{caller}: both members answered: {text}"
        );
        assert_eq!(2, answer.rows.len(), "{caller}: one row per member: {text}");
    }
    assert_no_patient_identifier_on_the_wire(&nodes);

    idp.type_tokens(CLINICAL_APP, false).await?;
    let untyped = idp.user_token(&secrets.clinical_app).await?;
    let (status, headers, text) = ask(app, &untyped).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status,
        "a token not typed at+jwt is refused: {text}"
    );
    let challenge = headers
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .ok_or("a 401 carries its challenge")?;
    assert!(
        challenge.contains("at+jwt"),
        "the challenge names the type it wanted: {challenge}"
    );
    Ok(())
}
