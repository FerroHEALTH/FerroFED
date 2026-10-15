// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document a representative inserts in a person's EHR under the person's
//! grant (Regulation (EU) 2025/327 Art 4(2), Art 5): when the token's issuer
//! declares the RFC 8693 §4.1 `act` claim of its patient tokens a
//! representative, every entry names that representative as its provider, a
//! `PARTY_IDENTIFIED` (openEHR RM Release 1.1.0, Common IM, §Referring to
//! Demographic Entities), with nothing of the patient in it (§5.4.1, N33);
//! an issuer that declares nothing leaves the mark the person's own.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use axum::Router;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::inserted::{PERSON, own_grant, person_gateway, providers, written};
use super::{
    EHR_SYSTEM, bound, creating, document, gateway_with, issue, post, receiving, rows, untouched,
};
use crate::auth::{bearing, minted};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL};
use crate::fhir::TestResult;
use crate::support::{ISSUER, call};

/// The synthetic representative who acts for [`PERSON`]: the `act` `sub`.
pub(super) const REPRESENTATIVE: &str = "synthetic-representative-5t7w";

/// The person's own grant, its `act` claim naming the current actor by the
/// string claims `act`.
pub(super) fn acted_for(act: &[(&str, &str)]) -> Claims {
    let mut granted = own_grant();
    granted.act = Some(
        act.iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    );
    granted
}

/// The suite's server settings with every issuer declaring the `act` claim
/// of its patient tokens a representative.
fn declaring() -> Result<ServerSettings, Box<dyn Error>> {
    let mut server = bound()?;
    for issuer in &mut server.auth.issuers {
        if let Some(patient) = issuer.patient.as_mut() {
            patient.act_is_representative = true;
        }
    }
    Ok(server)
}

/// The gateway receiving patient summaries at node A, its issuer declaring
/// the `act` claim of its patient tokens a representative, with the tables
/// `tables` added.
pub(super) fn representing_gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
    ]);
    let fhir = format!("{}{tables}", receiving("node-a", &[NAMESPACE]));
    gateway_with(dir, (a, b), (&crossref, &fhir), &declaring()?)
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_representatives_document_names_them_as_every_entrys_provider() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&acted_for(&[("sub", REPRESENTATIVE)]))?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 4(2), Art 5: {text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    assert!(!providers.is_empty(), "the composition holds an entry");
    for provider in &providers {
        assert_eq!(
            provider,
            &serde_json::json!({
                "_type": "PARTY_IDENTIFIED",
                "identifiers": [{
                    "_type": "DV_IDENTIFIER",
                    "issuer": ISSUER,
                    "id": REPRESENTATIVE,
                }],
            }),
            "Common IM: a party other than the subject, named as the token states: {body}"
        );
        let named = provider.to_string();
        for withheld in [PATIENT, PATIENT_TAIL, PERSON] {
            assert!(
                !named.contains(withheld),
                "N33: the provider names nothing of the person: {named}"
            );
        }
    }
    let captured = super::write::composed(&a).await?;
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !captured.contains(withheld),
            "N33: no identifier in the path or the headers: {captured}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_act_the_issuer_does_not_declare_leaves_the_persons_own_mark() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = person_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&acted_for(&[("sub", REPRESENTATIVE)]))?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    assert!(!providers.is_empty(), "the composition holds an entry");
    assert!(
        providers
            .iter()
            .all(|provider| provider == &serde_json::json!({ "_type": "PARTY_SELF" })),
        "an actor the issuer does not declare a representative is not read as one: {body}"
    );
    assert!(
        !body.contains(REPRESENTATIVE),
        "the undeclared actor is written nowhere: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_declaring_issuers_token_with_no_act_is_the_persons_own() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&own_grant())?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    assert!(
        !providers.is_empty()
            && providers
                .iter()
                .all(|provider| provider == &serde_json::json!({ "_type": "PARTY_SELF" })),
        "Art 5: the person's own insertion stays PARTY_SELF: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_representative_named_by_an_identifier_of_the_person_is_refused() -> TestResult {
    for named in [PATIENT, PERSON] {
        let (a, b) = (creating().await, Server::start().await);
        let dir = tempfile::tempdir()?;
        let app = representing_gateway(dir.path(), (&a, &b), "")?;
        let token = minted(&acted_for(&[("sub", named)]))?;
        let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
        let (code, diagnostics) = issue(&text)?;
        assert_eq!("forbidden", code, "{text}");
        assert!(
            !diagnostics.contains(named),
            "the refusal quotes no value: {diagnostics}"
        );
        assert!(
            untouched(&a, &b).await?,
            "nothing is sent to any member when the representative names the person"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_representative_named_by_nothing_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&acted_for(&[("iss", "https://proxy.example.test")]))?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert!(
        untouched(&a, &b).await?,
        "nothing is sent when the token names its representative by nothing"
    );
    Ok(())
}

#[tokio::test]
async fn a_representatives_own_issuer_names_their_identifier() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), "")?;
    let token = minted(&acted_for(&[
        ("sub", REPRESENTATIVE),
        ("iss", "https://proxy.example.test"),
    ]))?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let body = written(&a).await?;
    let providers = providers(&body)?;
    let issuers: Vec<&Value> = providers
        .iter()
        .map(|provider| &provider["identifiers"][0]["issuer"])
        .collect();
    assert!(
        !issuers.is_empty()
            && issuers
                .iter()
                .all(|issuer| *issuer == "https://proxy.example.test"),
        "RFC 8693 §4.1: the actor's own iss names the identifier's issuer: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_professionals_document_ignores_an_act_claim() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), "")?;
    let mut professional = crate::auth::claims();
    professional.act = Some(BTreeMap::from([(
        String::from("sub"),
        String::from(REPRESENTATIVE),
    )]));
    let token = minted(&professional)?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let body = written(&a).await?;
    assert!(
        providers(&body)?.iter().all(Value::is_null),
        "a professional's receipt names no provider: {body}"
    );
    Ok(())
}
