// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A list for a patient the cross-reference could not resolve at every
//! member: it fails under all-or-nothing, and under `partial` the member is
//! named in an `outcome` entry (§11.3, §11.4).

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::{Mock, ResponseTemplate};

use super::{
    DOCUMENT_A, DOCUMENT_B, Members, by_custodian, entries, fhir_document, names, native_endpoint,
};
use crate::facade::settings_with_room;
use crate::fhir::{FHIR, TestResult, text_registered};
use crate::support::call;

/// The list for a patient the PIX Manager resolves at node A, while the
/// Manager node B is resolved through answers `500`, with the
/// `[federation]` keys `federation` and the completeness header
/// `completeness` when given: its status and its text.
async fn listed_with_a_failed_resolution(
    federation: &str,
    completeness: Option<&str>,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    use ferrofed_testkit::pixm_stub;
    use ferrofed_testkit::seed::{CrossReferenceSeed, EhrDomain, PatientId};
    use wiremock::matchers::any;

    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let (domain, patient) = (EhrDomain::new(1), PatientId::new(1, 46));
    let feed = CrossReferenceSeed {
        patient,
        ehrs: vec![(
            domain,
            uuid::Uuid::from_u128(0x3a3a_3a3a_3a3a_4a3a_8a3a_3a3a_3a3a_3a3a),
        )],
    };
    let pix = pixm_stub::manager(std::slice::from_ref(&feed)).await;
    let down = Server::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&down)
        .await;
    let resolvers = format!(
        "\n[[pixm.manager]]\nendpoint.url = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\n\
         [[pixm.manager]]\nendpoint.url = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-b\" = \"urn:oid:2.999.32\"\n",
        pix.uri(),
        domain.system(),
        down.uri()
    );
    let own = format!(
        "{}{}",
        native_endpoint("node-a-eps", ("node-a", "org-a"), members.proxy_a.origin()),
        native_endpoint("node-b-eps", ("node-b", "org-b"), members.proxy_b.origin())
    );
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (
            &members.a.uri(),
            &members.b.uri(),
            &pdqm_stub::base_url(&members.pdq),
        ),
        &[],
        &own,
        (federation, &resolvers),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let mut request = Request::get(format!(
        "{FHIR}/DocumentReference?patient.identifier={}%7C{}",
        patient.namespace().replace(':', "%3A"),
        patient.value()
    ))
    .body(Body::empty())?;
    if let Some(completeness) = completeness {
        request
            .headers_mut()
            .insert("openehr-federation-completeness", completeness.parse()?);
    }
    call(app, request).await
}

#[tokio::test]
async fn a_resolution_the_cross_reference_could_not_answer_fails_the_list() -> TestResult {
    let (status, text) = listed_with_a_failed_resolution("", None).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "all-or-nothing, as the summary and the prescription search: {text}"
    );
    assert!(text.contains("node-b-pub (not-resolved)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_member_whose_resolution_failed_is_named_in_an_outcome_entry() -> TestResult
{
    let (status, text) =
        listed_with_a_failed_resolution("best_effort = true\n", Some("partial")).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let own = by_custodian(&entries(&bundle, "match"), "org-a").ok_or("node A's document")?;
    names(own, DOCUMENT_A)?;
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    assert!(
        outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("node-b-pub (not-resolved)")),
        "{outcome}"
    );
    Ok(())
}
