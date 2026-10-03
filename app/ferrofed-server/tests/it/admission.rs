// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check against mock nodes: each identifier-integrity
//! condition of §12b.2 reported as pass, fail or cannot-check with its
//! evidence, a node that cannot be reached failing and never passing, and the
//! only subjects on the wire being fresh synthetic ones that the report never
//! prints (§12b.1, §12b.2, §5.4.1; N33, N42a).
//!
//! CP-33a is an Operator point (§17), so these tests carry no conformance
//! marker: they test the evidence the gateway hands the operator.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::report::{Condition, Report, Verdict};
use ferrofed_server::admission::subject::{NAMESPACE, VALUE_PREFIX};
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::{EXIT_CONFIG, EXIT_USAGE};
use ferrofed_testkit::unreachable;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_its::json::from_canonical_json;
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Match, Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::facade::{crossref, registry};
use crate::run::binary;

type TestResult = Result<(), Box<dyn Error>>;

/// The `system_id`s the registry records for node A and node B.
const SYSTEM_A: &str = "cdr-a.example.org";
const SYSTEM_B: &str = "cdr-b.example.org";

/// Three version-4 UUIDs, as a conformant node mints them.
const V4: [&str; 3] = [
    "0b6f4f1e-6a55-4c2b-9d0e-1f2a3b4c5d6e",
    "1c7a5e2f-7b66-4d3c-8e1f-2a3b4c5d6e7f",
    "2d8b6f3a-8c77-4e4d-9f2a-3b4c5d6e7f80",
];

/// A second base URL no connection reaches, for node B beside an unreachable
/// node A: the registry refuses two endpoints at one URL.
const UNREACHABLE_B: &str = "http://127.0.0.1:0/node-b";

/// The `ehr_id` domain of node A and node B at the PIX Manager.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The subject of each test EHR a node created, by the `ehr_id` it issued.
type Issued = Arc<Mutex<BTreeMap<String, String>>>;

/// Returns the subject value of the `EHR_STATUS` a create carries, read
/// through the strict canonical reader, or `None` when the body is not an
/// `EHR_STATUS` whose subject is a `GENERIC_ID`.
fn subject(request: &Request) -> Option<String> {
    let text = std::str::from_utf8(&request.body).ok()?;
    let status: EhrStatus = from_canonical_json(text).ok()?;
    match status.subject.external_ref?.id {
        ObjectId::GenericId(id) => Some(id.value),
        _ => None,
    }
}

/// Matches a create whose body [`subject`] can read.
struct ReadableSubject;

impl Match for ReadableSubject {
    fn matches(&self, request: &Request) -> bool {
        subject(request).is_some()
    }
}

/// A node answering `POST /v1/ehr` with the next `ehr_id` of a list,
/// recording the subject each one was issued for.
struct Minting {
    ids: Mutex<VecDeque<String>>,
    issued: Issued,
}

impl Respond for Minting {
    #[expect(
        clippy::expect_used,
        reason = "the mock is mounted behind ReadableSubject, so every request it answers has a subject"
    )]
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut ids = self.ids.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(id) = ids.pop_front() else {
            return ResponseTemplate::new(500);
        };
        ids.push_back(id.clone());
        let subject = subject(request)
            .expect("the ReadableSubject matcher should admit only readable bodies");
        self.issued
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.clone(), subject);
        ResponseTemplate::new(201)
            .insert_header("ETag", format!("\"{id}\"").as_str())
            .insert_header("Location", format!("{}/{id}", request.url).as_str())
    }
}

/// A node answering `GET /v1/ehr/{ehr_id}` with an `EHR` whose
/// `system_id` is fixed.
struct Reading {
    system_id: String,
}

impl Respond for Reading {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let ehr_id = request
            .url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or_default();
        let system_id = &self.system_id;
        let body = format!(
            r#"{{"_type":"EHR","system_id":{{"_type":"HIER_OBJECT_ID","value":"{system_id}"}},"ehr_id":{{"_type":"HIER_OBJECT_ID","value":"{ehr_id}"}},"ehr_status":{{"_type":"OBJECT_REF","namespace":"local","type":"EHR_STATUS","id":{{"_type":"HIER_OBJECT_ID","value":"7f1e2d3c-4b5a-4968-8776-655443322110"}}}},"ehr_access":{{"_type":"OBJECT_REF","namespace":"local","type":"EHR_ACCESS","id":{{"_type":"HIER_OBJECT_ID","value":"8a2f3e4d-5c6b-4a79-9887-766554433221"}}}},"time_created":{{"_type":"DV_DATE_TIME","value":"2026-10-03T09:00:00Z"}}}}"#
        );
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json")
    }
}

/// A node that issues `ids` in turn and reports `system_id` in every EHR.
///
/// A create whose body is not a readable `EHR_STATUS` falls through to a
/// mock that expects no request, so the node fails its test when dropped.
async fn node(ids: &[&str], system_id: &str) -> (MockServer, Issued) {
    let server = MockServer::start().await;
    let issued = Issued::default();
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .and(ReadableSubject)
        .respond_with(Minting {
            ids: Mutex::new(ids.iter().map(|id| (*id).to_owned()).collect()),
            issued: Arc::clone(&issued),
        })
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(ResponseTemplate::new(400))
        .with_priority(2)
        .expect(0)
        .named("a create whose EHR_STATUS the strict canonical reader refuses")
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/ehr/[^/]+$"))
        .respond_with(Reading {
            system_id: system_id.to_owned(),
        })
        .mount(&server)
        .await;
    (server, issued)
}

/// A PIX Manager that maps each subject to the `ehr_id` `issued` holds for
/// it at node A, shifted by `shift` places when it is to answer wrongly.
struct Crossref {
    issued: Issued,
    shift: usize,
}

impl Respond for Crossref {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let subject = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "sourceIdentifier")
            .and_then(|(_, value)| value.split_once('|').map(|(_, id)| id.to_owned()));
        let issued = self.issued.lock().unwrap_or_else(PoisonError::into_inner);
        let ids: Vec<&String> = issued.keys().collect();
        let found = ids
            .iter()
            .position(|id| issued.get(*id) == subject.as_ref())
            .and_then(|at| ids.get((at + self.shift) % ids.len()));
        let body = match found {
            Some(ehr_id) => format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{ehr_id}"}}}}]}}"#
            ),
            None => r#"{"resourceType":"Parameters"}"#.to_owned(),
        };
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/fhir+json")
    }
}

/// A PIX Manager over `issued`, answering wrongly when `shift` is not zero.
async fn manager(issued: &Issued, shift: usize) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .respond_with(Crossref {
            issued: Arc::clone(issued),
            shift,
        })
        .mount(&server)
        .await;
    server
}

/// The `[pixm]` table of one Manager at `pix` serving node A and node B.
fn pixm(pix: &str) -> String {
    format!(
        "[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n"
    )
}

/// The configuration text over the registry document `document`.
fn configuration(document: &Path, top: &str, tables: &str) -> String {
    let document = toml::Value::String(document.display().to_string());
    format!(
        "{top}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{tables}"
    )
}

/// The federation over `registry`, with the top-level keys `top` and the
/// tables `tables`.
fn federation(
    dir: &Path,
    registry: &str,
    top: &str,
    tables: &str,
) -> Result<Federation, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let text = configuration(&document, top, tables);
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings)?.ok_or("a registry is configured")?)
}

/// A development federation over node A at `a` and node B at `b`, with the
/// static cross-reference, which knows no synthetic subject.
fn dev_federation(dir: &Path, a: &str, b: &str) -> Result<Federation, Box<dyn Error>> {
    federation(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &crossref(&[("node-a", V4[0])]),
    )
}

/// The admission check of node A's endpoint with three test EHRs.
async fn check_a(federation: &Federation) -> Result<Report, Box<dyn Error>> {
    Ok(ferrofed_server::admission::check(federation, &EndpointId::new("node-a-pub")?, 3).await?)
}

/// The verdict on `condition`.
fn verdict(report: &Report, condition: Condition) -> Result<Verdict, Box<dyn Error>> {
    Ok(report
        .finding(condition)
        .ok_or("every condition has a finding")?
        .verdict())
}

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

#[test]
fn a_registry_with_a_shared_system_id_refuses_the_check() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    let shared =
        registry("http://127.0.0.1:9", "http://127.0.0.1:9", "").replace(SYSTEM_B, SYSTEM_A);
    std::fs::write(&document, shared)?;
    let output = binary(
        &["admission", "check", "--endpoint", "node-a-pub"],
        &configuration(&document, "", ""),
    )?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(SYSTEM_A),
        "the shared system_id is named: {stderr}"
    );
    assert!(output.stdout.is_empty(), "nothing was checked");
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
async fn the_binary_exits_one_on_a_failed_condition() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(unreachable::BASE, UNREACHABLE_B, ""))?;
    let text = configuration(&document, "", "");
    let output = tokio::task::spawn_blocking(move || {
        binary(&["admission", "check", "--endpoint", "node-a-pub"], &text)
            .map_err(|error| error.to_string())
    })
    .await??;
    assert_eq!(Some(1), output.status.code());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[fail] ehr_id generation"), "{stdout}");
    Ok(())
}

#[test]
fn an_endpoint_the_registry_does_not_hold_is_a_usage_refusal() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(unreachable::BASE, UNREACHABLE_B, ""))?;
    let output = binary(
        &["admission", "check", "--endpoint", "node-c-pub"],
        &configuration(&document, "", ""),
    )?;
    assert_eq!(Some(i32::from(EXIT_USAGE)), output.status.code());
    assert!(String::from_utf8_lossy(&output.stderr).contains("node-c-pub"));
    Ok(())
}

#[test]
fn a_configuration_without_a_registry_cannot_check_admission() -> TestResult {
    let output = binary(&["admission", "check", "--endpoint", "node-a-pub"], "")?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
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
