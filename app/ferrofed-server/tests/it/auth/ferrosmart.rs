// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway behind FerroSMART: FerroSMART-shaped tokens read under
//! `ferrohealth-smart`'s `ferrosmart` issuer preset, the issuer table read
//! from the configuration as an operator writes it.
//!
//! The preset keeps every `rfc9068` default but two: `typ` is `at+jwt` alone
//! (RFC 9068 §2.1) and `scope` is required (SMART App Launch 2.2.0, Obtain
//! access token, Response). One `persons` table names the FerroSMART issuer
//! (FerroHEALTH/FerroSMART#591): a person reaches the gateway through a SMART
//! app it lists in `delegates`, read as delegated, and a Backend Services
//! token, whose `sub` is its `client_id` (RFC 9068 §2.2), reads as the system.

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

/// The SMART app the deployment admits for a person.
const APP: &str = "growth-chart";

/// A SMART app the deployment does not list.
const UNLISTED: &str = "unlisted-app";

/// The `[auth]` of a gateway trusting the suite's test issuer as FerroSMART:
/// one `persons` table under the `ferrosmart` preset, [`APP`] its delegate,
/// read from the configuration text.
fn behind_ferrosmart() -> Result<AuthSettings, Box<dyn Error>> {
    let text = format!(
        "[auth]\nproduct_scope = \"{OPERATOR_SCOPE}\"\n\n[auth.issuer.{name}]\nissuer = \"{ISSUER}\"\nkind = \"persons\"\npreset = \"ferrosmart\"\naudience = [\"{AUDIENCE}\"]\njwks_file = {}\n{}\n\n[auth.issuer.{name}.profile]\ndelegates = [{{ client = \"{APP}\" }}]\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\ndemographic_clients = [\"{APP}\"]\n",
        toml_path(jwks_file()),
        attributes_toml(ISSUER),
        name = issuer_name(ISSUER),
    );
    let settings =
        Config::from_sources(Some(&support::signed(&text)), &BTreeMap::new())?.resolve()?;
    Ok(settings.server.auth)
}

#[tokio::test]
async fn a_person_through_a_listed_app_is_read() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut through = claims();
    APP.clone_into(&mut through.client_id);
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&through)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn a_person_through_an_unlisted_app_is_refused() -> TestResult {
    let gateway = Gateway::with(behind_ferrosmart()?).await?;
    let mut through = claims();
    UNLISTED.clone_into(&mut through.client_id);
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&through)?)?,
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
