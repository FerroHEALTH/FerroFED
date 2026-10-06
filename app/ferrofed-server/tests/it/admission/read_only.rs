// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check without writes, for a member whose governance forbids
//! test data: one query of existing EHRs and no write reach the node, the
//! conditions a read can reach are judged on what it returns, and the
//! report names every condition the run leaves unproven (§12b.1, §12b.2,
//! §5.5; N34, N42a).

use std::error::Error;

use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::read_only::EXISTING_EHRS;
use ferrofed_server::admission::report::{Condition, Mode, Report, Verdict};
use ferrofed_server::federation::Federation;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::unreachable;
use wiremock::matchers::{any, body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{SYSTEM_A, SYSTEM_B, TestResult, V4, dev_federation, verdict};

/// A node holding EHRs whose `ehr_id` and `system_id` cells are `rows`,
/// which answers the query of existing EHRs alone and fails its test on any
/// other request.
async fn holding(rows: &str) -> Server {
    let server = Server::start().await;
    let body = format!(
        r##"{{"q":"{EXISTING_EHRS}","columns":[{{"name":"#0","path":"e/ehr_id/value"}},{{"name":"#1","path":"e/system_id/value"}}],"rows":{rows}}}"##
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(EXISTING_EHRS))
        .and(body_string_contains(r#""fetch":3"#))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json"),
        )
        .expect(1)
        .named("the one query of existing EHRs")
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .with_priority(10)
        .expect(0)
        .named("any write, or any other request, to a node that forbids test data")
        .mount(&server)
        .await;
    server
}

/// The rows of three EHRs with the `ehr_id`s of `ids`, each reporting
/// `system_id`.
fn rows(ids: &[&str], system_id: &str) -> String {
    let rows: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"["{id}","{system_id}"]"#))
        .collect();
    format!("[{}]", rows.join(","))
}

/// The read-only admission check of node A's endpoint, reading three EHRs.
async fn check_a(federation: &Federation) -> Result<Report, Box<dyn Error>> {
    Ok(
        ferrofed_server::admission::read_only::check(
            federation,
            &EndpointId::new("node-a-pub")?,
            3,
        )
        .await?,
    )
}

#[tokio::test]
async fn a_node_holding_version_4_ids_under_its_own_system_id_passes_without_a_write() -> TestResult
{
    let a = holding(&rows(&V4, SYSTEM_A)).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(Mode::ReadOnly, report.mode());
    assert!(report.created().is_empty(), "nothing was created: {report}");
    assert_eq!(V4.len(), report.read(), "{report}");
    for condition in [Condition::EhrIdGeneration, Condition::SystemIdUniqueness] {
        assert_eq!(Verdict::Pass, verdict(&report, condition)?, "{report}");
    }
    assert!(!report.failed(), "{report}");
    a.verify().await;
    Ok(())
}

#[tokio::test]
async fn the_report_names_every_condition_a_run_without_writes_leaves_unproven() -> TestResult {
    let a = holding(&rows(&V4, SYSTEM_A)).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        vec![
            Condition::NoReuse,
            Condition::NoForeignAdoption,
            Condition::EhrIdExchange
        ],
        report.unproven(),
        "{report}"
    );
    let text = report.to_string();
    assert!(
        text.contains("This run made no write to the node"),
        "the header says so: {text}"
    );
    assert!(
        text.contains(
            "Left unproven by a run without writes: no reuse, no adoption of foreign ehr_ids, ehr_id exchange."
        ),
        "{text}"
    );
    assert!(!text.contains("EHRs created"), "{text}");
    Ok(())
}

#[tokio::test]
async fn sequential_ids_the_node_holds_fail_generation() -> TestResult {
    let a = holding(&rows(&["1", "2", "3"], SYSTEM_A)).await;
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
async fn an_ehr_reporting_another_members_system_id_fails() -> TestResult {
    let a = holding(&rows(&V4, SYSTEM_B)).await;
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
async fn an_ehr_from_a_system_the_registry_routes_nowhere_is_left_undecided() -> TestResult {
    let a = holding(&rows(&V4, "legacy.example.org")).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::CannotCheck,
        verdict(&report, Condition::SystemIdUniqueness)?,
        "{report}"
    );
    assert!(!report.failed(), "{report}");
    Ok(())
}

#[tokio::test]
async fn a_node_holding_no_ehr_proves_nothing() -> TestResult {
    let a = holding("[]").await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    for condition in [Condition::EhrIdGeneration, Condition::SystemIdUniqueness] {
        assert_eq!(
            Verdict::CannotCheck,
            verdict(&report, condition)?,
            "{report}"
        );
    }
    assert!(
        report.unproven().contains(&Condition::EhrIdGeneration),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_row_that_is_not_two_strings_fails_and_is_never_dropped() -> TestResult {
    let a = holding(&format!(r#"[["{}","{SYSTEM_A}"],[7,"{SYSTEM_A}"]]"#, V4[0])).await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    assert_eq!(
        Verdict::Fail,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    assert!(report.read() == 0, "no partial read is kept: {report}");
    Ok(())
}

#[tokio::test]
async fn a_node_refusing_the_query_fails_with_its_status_and_never_passes() -> TestResult {
    let a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(403).set_body_raw(
            r#"{"message":"population queries are not served"}"#,
            "application/json",
        ))
        .expect(1)
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;

    for condition in [Condition::EhrIdGeneration, Condition::SystemIdUniqueness] {
        let finding = report.finding(condition).ok_or("a finding")?;
        assert_eq!(Verdict::Fail, finding.verdict(), "{report}");
        assert!(
            finding
                .evidence()
                .iter()
                .any(|line| line.contains("403 Forbidden")),
            "the node's status is named: {report}"
        );
    }
    assert!(
        !report.to_string().contains("population queries"),
        "the node's error body is never printed: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_node_fails_every_condition_the_run_exercises() -> TestResult {
    let dir = tempfile::tempdir()?;
    let report = check_a(&dev_federation(
        dir.path(),
        unreachable::BASE,
        super::UNREACHABLE_B,
    )?)
    .await?;

    for condition in [Condition::EhrIdGeneration, Condition::SystemIdUniqueness] {
        assert_eq!(Verdict::Fail, verdict(&report, condition)?, "{report}");
    }
    assert!(report.failed(), "{report}");
    Ok(())
}

// NOTE: §5.4.1, N33: the ehr_id of an existing EHR is a real patient's pseudonymous identifier,
// so no report line, sequential or repeated or misrouted, ever carries one.
#[tokio::test]
async fn the_report_names_no_ehr_id_it_read() -> TestResult {
    let repeated = [V4[0], V4[0], V4[1]];
    for rows in [
        rows(&V4, SYSTEM_A),
        rows(&V4, SYSTEM_B),
        rows(&["9101", "9102", "9103"], SYSTEM_A),
        rows(&repeated, SYSTEM_A),
    ] {
        let a = holding(&rows).await;
        let dir = tempfile::tempdir()?;
        let report = check_a(&dev_federation(dir.path(), &a.uri(), unreachable::BASE)?).await?;
        let text = format!("{report} {report:?}");
        for id in V4.iter().chain(["9101", "9102", "9103"].iter()) {
            assert!(
                !text.contains(id),
                "the report names the ehr_id {id}: {text}"
            );
        }
        assert!(
            text.contains("row "),
            "the report names its EHRs by row: {text}"
        );
    }
    Ok(())
}
