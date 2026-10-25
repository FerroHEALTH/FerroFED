// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member serving prescriptions that the cross-reference could not answer
//! for is never read as a member with no prescription (§11.1, §11.3): it
//! fails the search under all-or-nothing as `not-resolved`, and is named in
//! an `outcome` entry under `partial` (§11.4).

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pixm_stub;
use ferrofed_testkit::seed::{CrossReferenceSeed, EhrDomain, PatientId};
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use super::{AT_A, Members, RX_A, RX_B, entries, page, relays, searchset};
use crate::fhir::{FHIR, TestResult};
use crate::support::call;

/// The search for a patient the PIX Manager resolves at node A, while the
/// Manager node B is resolved through answers `500`, with the
/// `[federation]` keys `federation` and the completeness header
/// `completeness` when given: its status and its text, and the members.
async fn searched_with_a_failed_resolution(
    federation: &str,
    completeness: Option<&str>,
) -> Result<(StatusCode, String, Members), Box<dyn Error>> {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
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
        "\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\n\
         [[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-b\" = \"urn:oid:2.999.32\"\n",
        pix.uri(),
        domain.system(),
        down.uri()
    );
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), &[], (federation, &resolvers))?;
    let mut request = Request::get(format!(
        "{FHIR}/MedicationRequest?patient.identifier={}%7C{}",
        patient.namespace().replace(':', "%3A"),
        patient.value()
    ))
    .body(Body::empty())?;
    if let Some(completeness) = completeness {
        request
            .headers_mut()
            .insert("openehr-federation-completeness", completeness.parse()?);
    }
    let (status, text) = call(app, request).await?;
    Ok((status, text, members))
}

#[tokio::test]
async fn a_member_the_cross_reference_could_not_answer_for_fails_the_search_and_is_named()
-> TestResult {
    let (status, text, members) = searched_with_a_failed_resolution("", None).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        text.contains("the search is incomplete") && text.contains("node-b-pub (not-resolved)"),
        "the member is named: {text}"
    );
    assert!(
        !relays(&text, RX_A, AT_A)?,
        "no partial list under all-or-nothing"
    );
    assert!(
        members.proxy_b.journal().is_empty(),
        "node B is never asked"
    );
    Ok(())
}

#[tokio::test]
async fn under_partial_a_member_whose_resolution_failed_is_named_in_an_outcome_entry() -> TestResult
{
    let (status, text, _members) =
        searched_with_a_failed_resolution("best_effort = true\n", Some("partial")).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(1, entries(&bundle, "match").len(), "node A's: {bundle}");
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    assert_eq!(
        outcome.pointer("/issue/0/severity"),
        Some(&Value::from("warning"))
    );
    assert!(
        outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("node-b-pub (not-resolved)")),
        "{outcome}"
    );
    Ok(())
}
