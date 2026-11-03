// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record of a dispensation (Annex II 3.2) and its place,
//! reserved before the write.

use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;

use super::{
    BY_IDENTIFIER, EHR_A, EHR_B, Members, RX_A, RX_B, SERVED_BOTH, built, created, creates,
    dispensation, members, own_endpoint, post,
};
use crate::feed_audit::{SETTLE, audit_tables};
use crate::fhir::TestResult;
use crate::support::call;

/// The gateway over `members` recording to `repository` with the
/// `[audit.repository]` keys `extra`.
pub(in crate::fhir) fn recording(
    members: &Members,
    dir: &std::path::Path,
    repository: &FeedRepository,
    extra: &str,
) -> Result<axum::Router, Box<dyn std::error::Error>> {
    let own = format!(
        "{}{}",
        own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
        own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
    );
    built(
        dir,
        (&members.a.uri(), &members.b.uri()),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own,
        (
            "",
            &format!("{SERVED_BOTH}{}", audit_tables(repository, extra)),
        ),
    )
}

#[tokio::test]
async fn a_dispensation_is_recorded_as_a_creation_of_both_categories() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = recording(&members, dir.path(), &repository, "")?;
    let (status, text) = call(app, post(&dispensation(BY_IDENTIFIER))?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
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
        "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
        "eehrxf-document-priority-category-cs|Electronic-Prescriptions:construction",
        "create MedicationDispense",
        "node-a-mpd",
        "node-b-mpd",
        EHR_A,
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}

#[tokio::test]
async fn a_dispensation_read_is_recorded_as_a_read_of_dispensations() -> TestResult {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let held = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationDispense","id":"md-1","status":"completed"},"search":{"mode":"match"}}]}"#;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(held.as_bytes().to_vec(), "application/fhir+json"),
        )
        .mount(&members.own_a)
        .await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = recording(&members, dir.path(), &repository, "")?;
    let id = super::face_id("node-a-mpd", EHR_A, super::CREATED)?;
    let request = http::Request::get(format!("{}/MedicationDispense/{id}", crate::fhir::FHIR))
        .body(axum::body::Body::empty())?;
    let (status, text) = call(app, request).await?;
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
        "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
        "MedicationDispense/md-1",
        "node-a-mpd",
        EHR_A,
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    assert!(
        !record.contains("Electronic-Prescriptions:construction"),
        "a dispensation read reaches no prescription: {record}"
    );
    assert!(record.contains("\"action\":\"R\""), "a read: {record}");
    Ok(())
}

#[tokio::test]
async fn a_dispensation_version_read_is_recorded_as_a_read_of_dispensations() -> TestResult {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};

    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let held = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationDispense","id":"md-1","meta":{"versionId":"1"},"status":"completed"},"search":{"mode":"match"}}]}"#;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(held.as_bytes().to_vec(), "application/fhir+json"),
        )
        .mount(&members.own_a)
        .await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = recording(&members, dir.path(), &repository, "")?;
    let id = super::face_id("node-a-mpd", EHR_A, super::CREATED)?;
    let request = http::Request::get(format!(
        "{}/MedicationDispense/{id}/_history/1",
        crate::fhir::FHIR
    ))
    .body(axum::body::Body::empty())?;
    let (status, text) = call(app, request).await?;
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
        "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
        "MedicationDispense/md-1",
        "node-a-mpd",
        "\"action\":\"R\"",
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}

#[tokio::test]
async fn a_dispensation_whose_record_has_no_place_reaches_no_member() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let app = recording(&members, dir.path(), &repository, "spool_max_events = 1")?;
    let (status, text) = call(app.clone(), post(&dispensation(BY_IDENTIFIER))?).await?;
    assert_eq!(
        StatusCode::CREATED,
        status,
        "the first fills the spool: {text}"
    );
    let (status, text) = call(app, post(&dispensation(BY_IDENTIFIER))?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!(
        1,
        creates(&members.own_a).await?.len(),
        "Annex II 3.2: the refused dispensation never reaches its member"
    );
    Ok(())
}
