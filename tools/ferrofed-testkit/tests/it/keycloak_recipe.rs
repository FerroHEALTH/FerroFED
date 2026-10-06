// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The production guide's Keycloak recipe, read from the book page offline:
//! the commands, the four protocol mapper files they read and the gateway's
//! `[auth]` table, which the gated test applies to a pinned Keycloak.

use std::error::Error;

use ferrofed_testkit::containers::keycloak::recipe::{PAGE, PAGE_SERVER, Recipe};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "test assertions in a test that returns its setup errors"
)]
fn the_production_guide_carries_the_recipe_the_gated_test_applies() -> Result<(), Box<dyn Error>> {
    let recipe = Recipe::from_page(&std::fs::read_to_string(PAGE)?)?;

    let names: Vec<&str> = recipe
        .files()
        .iter()
        .map(|file| file.name.as_str())
        .collect();
    assert_eq!(
        vec![
            "audience.json",
            "purpose-of-use.json",
            "client-id.json",
            "professional.json"
        ],
        names
    );
    for file in recipe.files() {
        serde_json::from_str::<serde::de::IgnoredAny>(&file.content)
            .map_err(|error| format!("{} is not JSON: {error}", file.name))?;
    }
    let commands = recipe.commands_against("http://keycloak:8080")?;
    assert!(!commands.contains(PAGE_SERVER), "{commands}");
    assert!(
        commands.contains("access.token.header.type.rfc9068\"=true"),
        "the clients type their tokens at+jwt: {commands}"
    );
    let auth = recipe.auth_against("http://127.0.0.1:8080");
    assert!(auth.contains("[[auth.issuer]]"), "{auth}");
    assert!(
        auth.contains("client_tokens_act_for_professional = true")
            && auth.contains("[auth.issuer.assurance]"),
        "the service acts for a named professional, at a declared level: {auth}"
    );
    assert!(
        commands.contains("acr.loa.map"),
        "the realm names its levels in acr: {commands}"
    );
    assert!(!auth.contains(PAGE_SERVER), "{auth}");
    Ok(())
}
