// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The withdrawal of a dispensation (MyHealth@EU Requirements Catalogue
//! v10.0.0 07.04, §1.15.4): `PUT {fhir-base}/MedicationDispense/{id}` with
//! `status` `entered-in-error` is held to the HL7 Europe MPD profile, read
//! at the member the id names by that member's own `ehr_id`, and sent there
//! once as an update by the member's own id, its body unchanged but for its
//! top-level `id` (FHIR R4 `http.html` §3.1.0.4; §5.4.1, N33).

use std::error::Error;
use std::time::Duration;

use axum::body::Body;
use ferrofed_testkit::leak::{self, Scope};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::receive::issue;
use super::dispense::{BY_IDENTIFIER, CREATED, created, dispensation, gateway, members};
use super::dispensed::{HELD, dispensed};
use super::{Members, RX_A, RX_B, face_id, page};
use crate::facade::{EHR_A, PATIENT, PATIENT_TAIL};
use crate::fhir::{FHIR, TestResult};
use crate::support::call;

/// The withdrawal of node A's dispensation [`CREATED`], named by the face's
/// id `id`, with whitespace no serialiser would keep around that id.
pub(in crate::fhir) fn withdrawal(id: &str) -> String {
    dispensation(BY_IDENTIFIER)
        .replacen(
            r#"{"resourceType":"MedicationDispense","#,
            &format!(r#"{{"resourceType":"MedicationDispense", "id" :  "{id}" ,"#),
            1,
        )
        .replace(r#""status":"completed""#, r#""status":"entered-in-error""#)
}

/// `PUT {fhir-base}/MedicationDispense/{id}` of `body`.
fn put(id: &str, body: &str) -> Result<Request<Body>, http::Error> {
    Request::put(format!("{FHIR}/MedicationDispense/{id}"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_owned()))
}

/// Members whose node A holds the dispensation [`CREATED`] and answers its
/// update with `updated`.
async fn holding(updated: ResponseTemplate) -> Result<Members, Box<dyn Error>> {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(page(dispensed(HELD)))
        .mount(&members.own_a)
        .await;
    for own in [&members.own_a, &members.own_b] {
        Mock::given(method("PUT"))
            .and(path(format!("/fhir/MedicationDispense/{CREATED}")))
            .respond_with(updated.clone())
            .mount(own)
            .await;
    }
    Ok(members)
}

/// A member's `200 OK` of the update, at version `2`.
fn updated() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("etag", "W/\"2\"")
        .insert_header("last-modified", "Mon, 02 Nov 2026 10:00:00 GMT")
}

/// The updates that reached `server`, each with its path and its body as it
/// arrived.
async fn updates(server: &Server) -> Result<Vec<(String, Vec<u8>)>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .filter(|request| request.method == http::Method::PUT)
        .map(|request| (request.url.path().to_owned(), request.body))
        .collect())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_withdrawal_reaches_its_member_alone_with_its_body_unchanged_but_for_its_id() -> TestResult
{
    let members = holding(updated()).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let body = withdrawal(&id);
    let response = crate::support::send(gateway(&members, dir.path())?, put(&id, &body)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let field = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    assert_eq!(
        Some("W/\"2\"".to_owned()),
        field("etag"),
        "FHIR R4 http.html §3.1.0.4: the member's new version"
    );
    assert_eq!(
        Some("node-a-mpd".to_owned()),
        field("openehr-federation-endpoint")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert!(text.contains("withdrawn at member node-a"), "{text}");
    let expected = body.replacen(
        &format!(r#""id" :  "{id}" ,"#),
        &format!(r#""id" :  "{CREATED}" ,"#),
        1,
    );
    assert_ne!(expected, body);
    assert_eq!(
        vec![(
            format!("/fhir/MedicationDispense/{CREATED}"),
            expected.into_bytes()
        )],
        updates(&members.own_a).await?,
        "§3.1.0.4: one update by the member's own id, every byte but the id's as sent"
    );
    assert_eq!(
        Vec::<(String, Vec<u8>)>::new(),
        updates(&members.own_b).await?
    );
    assert!(
        members.proxy_b.journal().is_empty(),
        "node B is never asked"
    );
    leak::assert_withheld(
        &members.proxy_a.journal(),
        &[PATIENT, PATIENT_TAIL, "SYNTHETIC-RX-WITHHELD-A1"],
        Scope::OutsideBodies,
        "N33: no identifier in the path, the query or a header",
    );
    Ok(())
}

#[tokio::test]
async fn an_update_that_withdraws_nothing_is_refused_asking_no_member() -> TestResult {
    let members = holding(updated()).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let completed =
        withdrawal(&id).replace(r#""status":"entered-in-error""#, r#""status":"completed""#);
    let (status, text) = call(gateway(&members, dir.path())?, put(&id, &completed)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    assert!(members.unasked());
    Ok(())
}

#[tokio::test]
async fn a_body_whose_id_is_not_the_requests_is_refused_asking_no_member() -> TestResult {
    let members = holding(updated()).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let other = face_id("node-a-mpd", EHR_A, "md-2")?;
    let no_id = withdrawal(&id).replacen(&format!(r#" "id" :  "{id}" ,"#), "", 1);
    for body in [withdrawal(&other), no_id] {
        let (status, text) = call(gateway(&members, dir.path())?, put(&id, &body)?).await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            status,
            "FHIR R4 http.html §3.1.0.4: {text}"
        );
        assert_eq!("invalid", issue(&text)?.0);
    }
    assert!(members.unasked());
    Ok(())
}

#[tokio::test]
async fn a_withdrawal_that_breaks_its_profile_asks_no_member() -> TestResult {
    let members = holding(updated()).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let broken = withdrawal(&id).replace(r##""subject":{"reference":"#pat"},"##, "");
    let (status, text) = call(gateway(&members, dir.path())?, put(&id, &broken)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(members.unasked());
    Ok(())
}

#[tokio::test]
async fn the_members_refusal_is_a_conflict_naming_it_with_its_own_issues() -> TestResult {
    let members = holding(
        ResponseTemplate::new(409).set_body_raw(
            br#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"business-rule","diagnostics":"the dispensation is already handed out"}]}"#
                .to_vec(),
            "application/fhir+json",
        ),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let (status, text) = call(gateway(&members, dir.path())?, put(&id, &withdrawal(&id))?).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("conflict", code);
    assert!(
        diagnostics.contains("node-a") && diagnostics.contains("409"),
        "{diagnostics}"
    );
    assert!(
        text.contains("the dispensation is already handed out"),
        "MyHealth@EU Requirements Catalogue v10.0.0 §1.25: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_answer_is_asked_once_and_the_outcome_is_unknown() -> TestResult {
    let members = holding(updated().set_delay(Duration::from_secs(6))).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let (status, text) = call(gateway(&members, dir.path())?, put(&id, &withdrawal(&id))?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "{text}");
    assert!(issue(&text)?.1.contains("unknown"), "{text}");
    assert_eq!(1, updates(&members.own_a).await?.len(), "never sent again");
    Ok(())
}

#[tokio::test]
async fn a_withdrawal_of_a_dispensation_its_member_does_not_hold_writes_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let (status, text) = call(gateway(&members, dir.path())?, put(&id, &withdrawal(&id))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("no dispensation is known by this id", issue(&text)?.1);
    assert_eq!(
        Vec::<(String, Vec<u8>)>::new(),
        updates(&members.own_a).await?
    );
    assert_eq!(
        Vec::<(String, Vec<u8>)>::new(),
        updates(&members.own_b).await?
    );
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_withdrawal_is_recorded_as_an_update_of_dispensations() -> TestResult {
    use crate::feed_audit::SETTLE;
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = holding(updated()).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = super::dispense::recorded::recording(&members, dir.path(), &repository, "")?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let (status, text) = call(app, put(&id, &withdrawal(&id))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    let [record] = records.as_slice() else {
        return Err(format!("one record: {records:#?}").into());
    };
    for expected in [
        "eehrxf-document-priority-category-cs|Electronic-Dispensations",
        "update MedicationDispense",
        "node-a-mpd",
        EHR_A,
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}
