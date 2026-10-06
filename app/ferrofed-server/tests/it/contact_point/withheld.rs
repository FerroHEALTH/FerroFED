// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A contact point's requests are served with consent exclusions withheld,
//! whatever the deployment's `[federation.consent] disclose`: the healthcare
//! provider of another Member State a contact point relays is a healthcare
//! provider to whom the fact of a restriction "shall not be visible"
//! (Regulation (EU) 2025/327 Art 8, Art 11(5)). Under a deployment that
//! discloses them to its own callers, a member the pre-filter excludes and a
//! member that does not know the patient answer the contact point alike, and
//! so do a restricted patient and an unknown one.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ferrofed_engine::dispatch::NodeClients;
use ferrofed_engine::fanout::Budget;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::config::auth::{AuthSettings, KeySource, Verification};
use ferrofed_server::config::settings::ConsentDisclosure;
use ferrofed_server::federation::Federation;
use ferrofed_server::node_transport::BoundedTransport;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::Issuer;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use openehr_federation::aql::{Context, Targeting};
use openehr_federation::id::FederationId;

use super::{declared, relaying};
use crate::auth::{TestResult, bearing, minted, query, sent};
use crate::consent_withheld::{
    AT_A, AT_B, BOTH, Crossref, Denies, NOWHERE, by_subject, ehr_node, names_consent, record_of,
};
use crate::facade::{Answer, node_answering, registry, settings_with_room, statuses, wire};
use crate::support::error_body;

/// A gateway over node A and node B resolving through `crossref`,
/// pre-filtering through `denies`, under a deployment that discloses consent
/// exclusions, authenticating its callers as `auth` says.
fn disclosing(
    (a, b): (&Server, &Server),
    (crossref, denies): (Crossref, Denies),
    auth: AuthSettings,
) -> Result<Router, Box<dyn Error>> {
    let snapshot = RegistrySnapshot::from_toml_str(&registry(&a.uri(), &b.uri(), ""))?;
    let transport = BoundedTransport::new(Duration::from_secs(5), 16 * 1024 * 1024)?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?;
    let federation = Federation::new(
        FederationId::new("example-federation")?,
        snapshot,
        clients,
        Some(Arc::new(crossref)),
        Context::new(Targeting::AskAll),
        Budget::new(Duration::from_secs(2), Duration::from_secs(3))?,
    )
    .with_consent_prefilter(Arc::new(denies))
    .with_consent_disclosure(ConsentDisclosure::Disclosed)
    .with_signer(crate::support::signer("example-federation")?);
    let mut server = settings_with_room();
    server.auth = auth;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &server,
    ))
}

/// `request` sent to `app` with the test contact point's token, and the
/// status and body of the answer.
async fn as_contact_point(
    app: &Router,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let (status, _, text) = sent(app, bearing(request, &minted(&relaying())?)?).await?;
    Ok((status, text))
}

/// Art 8, Art 11(5): under a disclosing deployment, a member the pre-filter
/// excludes answers the contact point exactly as a member that does not
/// know the patient, and the answer names no consent.
// conformance: CP-36
#[tokio::test]
async fn an_excluded_member_and_one_without_the_patient_answer_alike() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let restricted = disclosing((&a, &b), (Crossref::Knows(BOTH), Denies(true)), declared()?)?;
    let (status, hidden) = as_contact_point(&restricted, query()?).await?;
    assert_eq!(StatusCode::OK, status, "{hidden}");
    assert!(!names_consent(&hidden), "Art 8: {hidden}");
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");

    let unknown = disclosing(
        (&a, &b),
        (Crossref::Knows(AT_A), Denies(false)),
        declared()?,
    )?;
    let (_, absent) = as_contact_point(&unknown, query()?).await?;
    assert_eq!(
        record_of(&absent, "node-b-pub")?,
        record_of(&hidden, "node-b-pub")?,
        "Art 8: node B reads the same to the contact point either way"
    );
    Ok(())
}

/// The deployment's own callers still get its setting, the same exclusion
/// `consent-denied` to them (N27a), while a contact point it also trusts
/// gets it withheld.
// conformance: CP-36
#[tokio::test]
async fn the_deployments_own_caller_still_sees_the_exclusion_disclosed() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let contact_point = Issuer::new("https://ncp.example.test")?;
    let mut auth = crate::support::auth();
    let mut declared_issuer = declared()?
        .issuers
        .into_iter()
        .next()
        .ok_or("the declared issuer")?;
    contact_point.name().clone_into(&mut declared_issuer.issuer);
    declared_issuer.verification = Verification::KeySet(KeySource::Set(contact_point.jwks()));
    auth.issuers.push(declared_issuer);
    let app = disclosing((&a, &b), (Crossref::Knows(BOTH), Denies(true)), auth)?;
    let (status, _, text) = sent(
        &app,
        bearing(query()?, &minted(&crate::support::claims())?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer),
        "N27a: {text}"
    );

    let mut relayed = relaying();
    contact_point.name().clone_into(&mut relayed.iss);
    let request = bearing(query()?, &contact_point.mint(&relayed)?)?;
    let (status, _, text) = sent(&app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-resolved")],
        statuses(&answer),
        "Art 8: the same deployment withholds it from the contact point: {text}"
    );
    Ok(())
}

/// Art 8, Art 11(5): a read by subject only an excluded member holds
/// answers the contact point as one no member holds, by status, code and
/// message, under a deployment that discloses exclusions to its own callers.
// conformance: CP-36
#[tokio::test]
async fn a_restricted_patient_and_an_unknown_one_answer_alike() -> TestResult {
    let a = ehr_node("cdr-a.example.org", crate::facade::EHR_A).await;
    let b = ehr_node("cdr-b.example.org", crate::facade::EHR_B).await;
    let restricted = disclosing((&a, &b), (Crossref::Knows(AT_B), Denies(true)), declared()?)?;
    let (status, text) = as_contact_point(&restricted, by_subject()?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "RFC 9110 §15.5.5: {text}");
    let hidden = error_body(&text)?;
    assert_eq!("subject-unavailable", hidden.code);
    assert!(!names_consent(&text), "Art 8: {text}");
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");

    let unknown = disclosing(
        (&a, &b),
        (Crossref::Knows(NOWHERE), Denies(false)),
        declared()?,
    )?;
    let (absent_status, absent_text) = as_contact_point(&unknown, by_subject()?).await?;
    let absent = error_body(&absent_text)?;
    assert_eq!(
        (status, hidden.code, hidden.message),
        (absent_status, absent.code, absent.message),
        "Art 8: a restricted patient reads as one no member holds"
    );
    Ok(())
}
