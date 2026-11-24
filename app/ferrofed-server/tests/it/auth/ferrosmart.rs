// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway behind FerroSMART: one `[auth.issuer.<name>]` table naming
//! only the issuer trusts it, read from the configuration as an operator
//! writes it.
//!
//! A token's `typ` is `at+jwt` (RFC 9068 §2.1) and its `scope` is required
//! (SMART App Launch 2.2.0, Obtain access token, Response). FerroSMART's
//! registry is the one list of clients: any app whose token's `aud` names
//! the gateway reaches it (RFC 9068 §4). A Backend Services token, whose
//! `sub` is its `client_id` (RFC 9068 §2.2), reads as the system.

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthSettings;
use http::StatusCode;

use super::{Gateway, assert_refused, bearing, claims, minted, query, sent};
use crate::support::caller::{
    AUDIENCE, ISSUER, OPERATOR_SCOPE, attributes_toml, issuer_name, jwks_file,
};
use crate::support::{self, toml_path};

type TestResult = Result<(), Box<dyn Error>>;

/// A SMART app a person signs in through.
const APP: &str = "growth-chart";

/// The `[auth]` of a gateway trusting the suite's test issuer as FerroSMART:
/// one table naming only the issuer, read from the configuration text.
fn behind_ferrosmart() -> Result<AuthSettings, Box<dyn Error>> {
    let text = format!(
        "[auth]\nproduct_scope = \"{OPERATOR_SCOPE}\"\n\n[auth.issuer.{name}]\nissuer = \"{ISSUER}\"\naudience = [\"{AUDIENCE}\"]\njwks_file = {}\n{}\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\ndemographic_clients = [\"{APP}\"]\n",
        toml_path(jwks_file()),
        attributes_toml(ISSUER),
        name = issuer_name(ISSUER),
    );
    let settings =
        Config::from_sources(Some(&support::signed(&text)), &BTreeMap::new())?.resolve()?;
    Ok(settings.server.auth)
}

#[tokio::test]
async fn a_person_through_an_app_audienced_to_the_gateway_is_read() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut through = claims();
    APP.clone_into(&mut through.client_id);
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&through)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn a_token_audienced_to_another_server_is_refused() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut elsewhere = claims();
    APP.clone_into(&mut elsewhere.client_id);
    "https://other.example.org/openehr".clone_into(&mut elsewhere.aud);
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&elsewhere)?)?,
        Refusal::Invalid,
    )
    .await
}

#[tokio::test]
async fn a_backend_services_client_reads_as_the_system() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut backend = claims();
    "lab-backend".clone_into(&mut backend.sub);
    "lab-backend".clone_into(&mut backend.client_id);
    backend.scope = Some(String::from("system/aql-*.s"));
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&backend)?)?).await?;
    assert_ne!(StatusCode::UNAUTHORIZED, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn a_token_without_its_scope_is_refused() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut without = claims();
    APP.clone_into(&mut without.client_id);
    without.scope = None;
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&without)?)?,
        Refusal::Invalid,
    )
    .await
}
