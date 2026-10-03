// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The cross-reference round trip, and only synthetic subjects on the wire (§5.4.1, N33).

use std::sync::PoisonError;

use ferrofed_server::admission::report::{Condition, Verdict};
use ferrofed_server::admission::subject::{NAMESPACE, VALUE_PREFIX};
use ferrofed_testkit::unreachable;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_its::json::from_canonical_json;
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use wiremock::MockServer;

use crate::facade::registry;

use super::{
    SYSTEM_A, TestResult, V4, check_a, dev_federation, federation, manager, node, pixm, verdict,
};

#[tokio::test]
async fn the_round_trip_passes_when_the_cross_reference_knows_each_new_ehr() -> TestResult {
    let (a, issued) = node(&V4, SYSTEM_A).await;
    let pix = manager(&issued, 0).await;
    let dir = tempfile::tempdir()?;
    let federation = federation(
        dir.path(),
        &registry(&a.uri(), unreachable::BASE, ""),
        "",
        &pixm(&pix.uri()),
    )?;
    let report = check_a(&federation).await?;

    assert_eq!(
        Verdict::Pass,
        verdict(&report, Condition::EhrIdExchange)?,
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn the_round_trip_fails_when_the_cross_reference_names_another_ehr() -> TestResult {
    let (a, issued) = node(&V4, SYSTEM_A).await;
    let pix = manager(&issued, 1).await;
    let dir = tempfile::tempdir()?;
    let federation = federation(
        dir.path(),
        &registry(&a.uri(), unreachable::BASE, ""),
        "",
        &pixm(&pix.uri()),
    )?;
    let report = check_a(&federation).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::EhrIdExchange)?,
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cross_reference_the_gateway_cannot_write_is_cannot_check() -> TestResult {
    let (a, _issued) = node(&V4, SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    let finding = report
        .finding(Condition::EhrIdExchange)
        .ok_or("a finding")?;
    assert_eq!(Verdict::CannotCheck, finding.verdict(), "{report}");
    assert!(
        finding
            .evidence()
            .iter()
            .any(|line| line.contains("writes to no cross-reference")),
        "the reason is given: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_federation_with_no_cross_reference_fails_the_exchange() -> TestResult {
    let (a, _issued) = node(&V4, SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let federation = federation(
        dir.path(),
        &registry(&a.uri(), unreachable::BASE, ""),
        "",
        "",
    )?;
    let report = check_a(&federation).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::EhrIdExchange)?,
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn only_synthetic_subjects_are_sent_and_the_report_prints_none() -> TestResult {
    let (a, issued) = node(&V4, SYSTEM_A).await;
    let b = MockServer::start().await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), &b.uri())?).await?;

    let requests = a.received_requests().await.ok_or("recording is on")?;
    let mut subjects = Vec::new();
    for request in requests
        .iter()
        .filter(|request| request.method.as_str() == "POST")
    {
        let status: EhrStatus = from_canonical_json(std::str::from_utf8(&request.body)?)?;
        let subject = status
            .subject
            .external_ref
            .ok_or("the EHR_STATUS names its subject")?;
        assert!(
            subject.namespace.starts_with("urn:oid:2.999."),
            "the namespace is in the example arc: {}",
            subject.namespace
        );
        assert_eq!(NAMESPACE, subject.namespace);
        let ObjectId::GenericId(id) = subject.id else {
            return Err(format!("the subject is a GENERIC_ID, not {:?}", subject.id).into());
        };
        assert!(id.value.starts_with(VALUE_PREFIX), "a synthetic value");
        subjects.push(id.value);
    }
    assert_eq!(3, subjects.len(), "one subject per test EHR");
    subjects.sort();
    subjects.dedup();
    assert_eq!(3, subjects.len(), "every subject is fresh");
    let recorded = issued.lock().unwrap_or_else(PoisonError::into_inner).len();
    assert_eq!(3, recorded);

    let text = report.to_string();
    for subject in &subjects {
        assert!(
            !text.contains(subject.as_str()),
            "the report prints a subject: {text}"
        );
        for request in &requests {
            assert!(
                !request.url.as_str().contains(subject.as_str()),
                "a subject travelled in the URL (§5.4.1, N33)"
            );
            for (name, value) in &request.headers {
                assert!(
                    !value
                        .as_bytes()
                        .windows(subject.len())
                        .any(|window| window == subject.as_bytes()),
                    "a subject travelled in the {name} header (§5.4.1, N33)"
                );
            }
        }
    }
    assert!(
        b.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "no other member is contacted"
    );
    Ok(())
}
