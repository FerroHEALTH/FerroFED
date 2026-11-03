// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation or a withdrawal the owner already holds is refused as a
//! duplicate with nothing written; another dispensation of the patient is
//! written.

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    BY_IDENTIFIER, DISPENSATION_SYSTEM, DISPENSATION_VALUE, created, creates, dispensation,
    gateway, members, none_written, post,
};
use crate::facade::EHR_A;
use crate::fhir::TestResult;
use crate::fhir::prescriptions::{Members, RX_A, RX_B, face_id, page, searchset};
use crate::support::call;

/// Node A's own endpoint answering the patient's dispensations with
/// `answer`, above the default answer of none.
async fn dispensations_at_a(members: &Members, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(answer)
        .with_priority(1)
        .mount(&members.own_a)
        .await;
}

/// A dispensation node A already holds, under the test dispensation's
/// business identifier.
fn already_held() -> String {
    format!(
        r#"{{"resourceType":"MedicationDispense","id":"md-0","status":"completed","identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}],"medicationCodeableConcept":{{"text":"Synthetic medicine A"}},"subject":{{"reference":"Patient/p-a"}}}}"#
    )
}

#[tokio::test]
async fn a_dispensation_the_owner_already_holds_is_refused_as_a_duplicate_with_nothing_written()
-> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    dispensations_at_a(
        &members,
        page(searchset(
            "https://cdr-a.example.org",
            &[&already_held()],
            None,
        )),
    )
    .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert!(text.contains("\"code\":\"duplicate\""), "{text}");
    let held = face_id("node-a-mpd", EHR_A, "md-0")?;
    assert!(
        text.contains(&format!("MedicationDispense/{held}")),
        "the refusal names the dispensation already written: {text}"
    );
    assert!(none_written(&members).await?, "nothing is written twice");
    for capture in members.proxy_a.journal() {
        let outside = format!(
            "{} {}",
            capture.path,
            capture.query.as_deref().unwrap_or_default()
        );
        assert!(
            !outside.contains(DISPENSATION_VALUE),
            "N33: the identifier is matched at the gateway, never sent: {outside}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_withdrawal_sent_again_is_refused_as_a_duplicate_with_nothing_written() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    dispensations_at_a(
        &members,
        page(searchset(
            "https://cdr-a.example.org",
            &[&already_held()],
            None,
        )),
    )
    .await;
    let withdrawal = dispensation(BY_IDENTIFIER).replace("\"completed\"", "\"entered-in-error\"");
    assert!(withdrawal.contains("\"status\":\"entered-in-error\""));
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(&members, dir.path())?, post(&withdrawal)?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert!(text.contains("\"code\":\"duplicate\""), "{text}");
    assert!(
        none_written(&members).await?,
        "a withdrawal sent as a create is a duplicate: nothing reaches the owner"
    );
    Ok(())
}

#[tokio::test]
async fn another_dispensation_of_the_patient_is_no_duplicate_and_is_written() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let other = already_held().replace(DISPENSATION_VALUE, "SYNTHETIC-MD-OTHER");
    dispensations_at_a(
        &members,
        page(searchset("https://cdr-a.example.org", &[&other], None)),
    )
    .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(1, creates(&members.own_a).await?.len(), "one create");
    Ok(())
}

#[tokio::test]
async fn an_owner_that_does_not_answer_the_duplicate_check_writes_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    dispensations_at_a(&members, ResponseTemplate::new(503)).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(
        text.contains("cannot be ruled out"),
        "a read that fails is never taken as no duplicate: {text}"
    );
    assert!(none_written(&members).await?);
    Ok(())
}
