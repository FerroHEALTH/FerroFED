// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifier-generation and `system_id` conditions of §12b.2 at each node (N42a).

use ferrofed_server::admission::report::{Condition, Verdict};
use ferrofed_testkit::unreachable;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    SYSTEM_A, SYSTEM_B, TestResult, UNREACHABLE_B, V4, check_a, dev_federation, node, verdict,
};

#[tokio::test]
async fn a_node_minting_version_4_ids_passes_generation_and_system_id() -> TestResult {
    let (a, _issued) = node(&V4, SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(V4.to_vec(), report.created(), "{report}");
    assert_eq!(
        Verdict::Pass,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    assert_eq!(
        Verdict::Pass,
        verdict(&report, Condition::SystemIdUniqueness)?,
        "{report}"
    );
    assert!(!report.failed(), "{report}");
    let text = report.to_string();
    assert!(
        text.contains("This check creates test EHRs on the node"),
        "the header says so: {text}"
    );
    assert!(text.contains("EHRs created (3)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn reuse_and_foreign_adoption_are_never_claimed() -> TestResult {
    let (a, _issued) = node(&V4, SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    for condition in [Condition::NoReuse, Condition::NoForeignAdoption] {
        let finding = report.finding(condition).ok_or("a finding")?;
        assert_eq!(Verdict::CannotCheck, finding.verdict(), "{report}");
        assert!(
            !finding.evidence().is_empty(),
            "a reason is given: {report}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_node_minting_sequential_ids_fails_generation() -> TestResult {
    let (a, _issued) = node(&["1", "2", "3"], SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    assert!(report.failed(), "{report}");
    Ok(())
}

#[tokio::test]
async fn a_node_minting_another_uuid_version_is_left_to_the_operator() -> TestResult {
    // NOTE: RFC 9562 Appendix A.1 gives this version-1 UUID; §12b.2 admits
    // another scheme only on equivalence the operator judges.
    let v1 = [
        "c232ab00-9414-11ec-b3c8-9f6bdeced846",
        "c232ab01-9414-11ec-b3c8-9f6bdeced846",
        "c232ab02-9414-11ec-b3c8-9f6bdeced846",
    ];
    let (a, _issued) = node(&v1, SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::CannotCheck,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_issuing_one_ehr_id_twice_fails_generation() -> TestResult {
    let (a, _issued) = node(&[V4[0]], SYSTEM_A).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    let finding = report
        .finding(Condition::EhrIdGeneration)
        .ok_or("a finding")?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{report}");
    assert!(
        finding
            .evidence()
            .iter()
            .any(|line| line.contains(V4[0]) && line.contains("3 different subjects")),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_reporting_another_members_system_id_fails() -> TestResult {
    let (a, _issued) = node(&V4, SYSTEM_B).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    let finding = report
        .finding(Condition::SystemIdUniqueness)
        .ok_or("a finding")?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{report}");
    assert!(
        finding
            .evidence()
            .iter()
            .any(|line| line.contains("node-b")),
        "the other member is named: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_reporting_a_system_id_the_registry_does_not_record_fails() -> TestResult {
    let (a, _issued) = node(&V4, "cdr-elsewhere.example.org").await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::SystemIdUniqueness)?,
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_node_fails_every_condition_it_exercises() -> TestResult {
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(
        dir.path(),
        unreachable::BASE,
        UNREACHABLE_B,
    )?)
    .await?;

    assert!(report.created().is_empty(), "{report}");
    for condition in [
        Condition::EhrIdGeneration,
        Condition::SystemIdUniqueness,
        Condition::EhrIdExchange,
    ] {
        let finding = report.finding(condition).ok_or("a finding")?;
        assert_eq!(Verdict::Fail, finding.verdict(), "{condition:?}: {report}");
    }
    assert!(
        report
            .to_string()
            .contains("endpoint node-a-pub could not be reached"),
        "the typed cause is reported: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_node_refusing_the_create_fails_with_its_status_and_never_its_body() -> TestResult {
    let a = MockServer::start().await;
    let echo = "ffd-admission-echoed-by-the-node";
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(ResponseTemplate::new(409).set_body_raw(
            format!(r#"{{"message":"{echo}"}}"#).into_bytes(),
            "application/json",
        ))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    let text = report.to_string();
    assert!(text.contains("409"), "the node's status is named: {text}");
    assert!(
        !text.contains(echo),
        "the node's body is never printed: {text}"
    );
    Ok(())
}
