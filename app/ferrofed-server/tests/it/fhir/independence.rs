// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Neither harmonised component changes the other's output on the running
//! gateway, so neither is "adversely affected by other software components
//! of the same EHR system" (Regulation (EU) 2025/327 Art 30(1)(b)).
//!
//! The logging component's record of an access the FHIR face serves, with
//! the interoperability component's work in its path, names what the record
//! of the same access over the ITS-REST face names: the patient summary
//! against the ITS-REST query that reaches the same compositions, and a
//! receipt against the ITS-REST write of the composition it became. The
//! fields compared are who, whose data, which member, which data and how it
//! ended; the request's own text and operation differ by construction, and
//! so does what only the FHIR request carries (the patient summary category
//! it serves, Art 14(1)(a), and for a receipt the patient and the version
//! its document and member name). The document list reaches each member's
//! own FHIR endpoint, which no ITS-REST request reaches, so its record is
//! compared with itself across two lists whose entries the interoperability
//! component writes differently of the same data.
//!
//! The documents the interoperability component writes, the patient summary
//! to the caller and the composition a received document becomes to its
//! member, are the same bytes, the values minted per request aside, with no
//! access log, and whether the Audit Record Repository takes the record,
//! refuses it into the spool or holds its answer. A sink that refuses the
//! record refuses the access (Annex II 3.2): the caller gets `503
//! access-unrecorded` and none of the document, and a composition already
//! written is the same one. No specification governs the test design: our
//! own design.

use std::error::Error;

use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use axum::Router;
use axum::body::Body;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::list::{DOCUMENT_A, DOCUMENT_B, Members, fhir_document, list};
use super::receive::{VERSION_A, creating, document, gateway_with, post, receiving, rows};
use super::{
    ALLERGIES, TestResult, UID_A, UID_B, gateway_over, node_holding, resources, summary, supplier,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, body, settings_with_room};
use crate::feed_audit::{SETTLE, audit_tables};
use crate::support::call;

/// Node A's own document of [`DOCUMENT_A`], the same JSON value written
/// with no whitespace, so the entry the face writes of it differs and the
/// data does not.
const DOCUMENT_A_COMPACT: &[u8] = b"{\"resourceType\":\"Bundle\",\"identifier\":{\"system\":\"urn:ietf:rfc:3986\",\"value\":\"urn:uuid:5a1e0000-0000-4000-8000-00000000000a\"},\"type\":\"document\",\"entry\":[]}";

/// The `detail` types that name the data an access reached and how the
/// record classifies and keeps it, the request's own text left out.
const DATA: [&str; 8] = [
    "status",
    "node",
    "system-id",
    "rows",
    "template-id",
    "version-uid",
    "delivered",
    "category-map-digest",
];

/// The `detail` types that state the retention of a record.
const RETENTION: [&str; 2] = ["ehds-retention-years", "ehds-retention-ground"];

/// What the logging component's sink does with a record: the four states a
/// deployment's sink can be in.
#[derive(Debug, Clone, Copy)]
enum Sink {
    /// The deployment keeps no access log: no logging component is in the
    /// path at all.
    Absent,
    /// The Audit Record Repository takes every record.
    Accepting,
    /// The repository answers `503`, and the spool keeps the record.
    RepositoryRefusing,
    /// The repository takes each record and holds its answer past the test.
    Slow,
}

impl Sink {
    /// Every state in which the sink takes the record.
    const TAKING: [Self; 4] = [
        Self::Absent,
        Self::Accepting,
        Self::RepositoryRefusing,
        Self::Slow,
    ];

    /// The `[audit]` tables of this state over `repository`.
    fn tables(self, repository: &FeedRepository) -> String {
        match self {
            Self::Absent => String::new(),
            Self::Accepting => audit_tables(repository, ""),
            Self::RepositoryRefusing => {
                repository.set_up(false);
                audit_tables(repository, "")
            }
            Self::Slow => {
                repository.go_silent();
                audit_tables(repository, "")
            }
        }
    }
}

/// The access records among `records`, as JSON, in arrival order.
fn accesses(records: &[String]) -> Result<Vec<Value>, Box<dyn Error>> {
    records
        .iter()
        .filter(|record| record.contains("ehds-categories"))
        .map(|record| Ok(serde_json::from_str(record)?))
        .collect()
}

/// Waits until `repository` holds `count` access records, and returns the
/// last.
async fn nth_access(repository: &FeedRepository, count: usize) -> Result<Value, Box<dyn Error>> {
    let until = std::time::Instant::now() + SETTLE;
    loop {
        let records = accesses(&repository.records())?;
        if records.len() >= count {
            return records
                .into_iter()
                .nth(count.saturating_sub(1))
                .ok_or_else(|| "the access record".into());
        }
        if std::time::Instant::now() >= until {
            return Err(format!("{count} access records, got {records:#?}").into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// The entities of `record` named `name`, each as its identifier and the
/// `detail` entries of the types `kinds`.
fn entities(record: &Value, name: &str, kinds: &[&str]) -> Vec<(Value, Vec<Value>)> {
    record["entity"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entity| entity["name"] == name)
        .map(|entity| {
            let details = entity["detail"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|detail| kinds.iter().any(|kind| detail["type"] == *kind))
                .cloned()
                .collect();
            (
                entity
                    .pointer("/what/identifier")
                    .cloned()
                    .unwrap_or_default(),
                details,
            )
        })
        .collect()
}

/// The patient entity of `record` (BALP `entity:patient`).
fn patient(record: &Value) -> Option<&Value> {
    record["entity"]
        .as_array()?
        .iter()
        .find(|entity| entity.pointer("/role/code") == Some(&Value::from("1")))
}

/// The categories of `record`, each with its basis, sorted.
fn categories(record: &Value) -> Vec<Value> {
    let mut categories: Vec<Value> = entities(
        record,
        "ehds-categories",
        &["ehds-category", "ehds-category-basis", "ehds-unclassified"],
    )
    .into_iter()
    .flat_map(|(_, details)| details)
    .collect();
    categories.sort_by_key(Value::to_string);
    categories
}

/// The category entries the patient summary category adds to a record
/// whose request serves it by construction (Art 14(1)(a)).
fn by_construction() -> Vec<Value> {
    let system =
        "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";
    vec![
        serde_json::json!({"type": "ehds-category", "valueString": format!("{system}|Patient-Summaries")}),
        serde_json::json!({"type": "ehds-category-basis", "valueString": format!("{system}|Patient-Summaries:construction")}),
    ]
}

/// `record` with what differs between two records of one access left out:
/// the time it was recorded and the gateway's request id.
fn timeless(mut record: Value) -> Value {
    if let Some(object) = record.as_object_mut() {
        object.remove("recorded");
    }
    if let Some(entities) = record.get_mut("entity").and_then(Value::as_array_mut) {
        entities.retain(|entity| entity.pointer("/type/code") != Some(&Value::from("XrequestId")));
    }
    record
}

/// Fails unless `fhir` and `its_rest` name one caller, one action and one
/// outcome.
fn same_caller_action_and_outcome(fhir: &Value, its_rest: &Value) {
    assert_eq!(fhir["agent"], its_rest["agent"], "Annex II 3.2(b): who");
    assert_eq!(fhir["action"], its_rest["action"], "what was done");
    assert_eq!(fhir["outcome"], its_rest["outcome"], "how it ended");
}

/// The ITS-REST query that reaches what the allergies section query of
/// the summary reaches, for the test patient.
fn allergies_query() -> String {
    format!(
        "SELECT c, c/uid/value, c/archetype_details/template_id/value \
         FROM EHR e CONTAINS COMPOSITION c CONTAINS EVALUATION v[{ALLERGIES}] \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The patient summary gateway over node A and node B, the patient known at
/// both, the header asked of `pdq`, with the `[audit]` tables `tables`.
fn summarising(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    pdq: &PdqSupplier,
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let (app, _) = gateway_over(
        dir,
        (&a.uri(), &b.uri(), &pdq.base_url()),
        &rows,
        ("", tables),
    )?;
    Ok(app)
}

/// The patient summary `text`, the three values the face mints per request
/// (the document's identifier, the `Patient` id and the timestamp) replaced
/// by fixed names, so two summaries of the same data compare as bytes.
fn unminted(text: &str) -> Result<String, Box<dyn Error>> {
    let document: Value = serde_json::from_str(text)?;
    let identifier = document
        .pointer("/identifier/value")
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("urn:uuid:"))
        .ok_or("the document's identifier")?;
    let patient = resources(&document, "Patient")
        .first()
        .and_then(|patient| patient["id"].as_str())
        .ok_or("the Patient id")?;
    let timestamp = document
        .get("timestamp")
        .and_then(Value::as_str)
        .ok_or("the timestamp")?;
    Ok(text
        .replace(identifier, "<identifier>")
        .replace(patient, "<patient>")
        .replace(timestamp, "<timestamp>"))
}

/// The bodies of every composition `node` was asked to create at
/// [`EHR_A`], in arrival order.
async fn written_to(node: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let target = format!("/v1/ehr/{EHR_A}/composition");
    requests
        .into_iter()
        .filter(|request| request.method == http::Method::POST && request.url.path() == target)
        .map(|request| Ok(String::from_utf8(request.body)?))
        .collect()
}

/// The composition `text` a received document became, the start time the
/// face sets to the time of receipt replaced by a fixed name.
fn received_at_any_time(text: &str) -> Result<String, Box<dyn Error>> {
    let composition: Value = serde_json::from_str(text)?;
    let start = composition
        .pointer("/context/start_time/value")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the context start time: {text}"))?;
    Ok(text.replace(start, "<received>"))
}

/// The gateway over node A and node B receiving patient summaries at node A,
/// the patient known there, with the `[audit]` tables `tables`.
fn receiving_at_a(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!("{}{tables}", receiving("node-a", &[NAMESPACE]));
    gateway_with(dir, (a, b), (&crossref, &fhir), &settings_with_room())
}

/// Fails unless the answer `(status, text)` is the refusal of an access
/// whose record could not be stored, with none of `data` in it.
fn withheld((status, text): &(StatusCode, String), data: &[&str]) {
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, *status, "{text}");
    assert!(text.contains("access-unrecorded"), "{text}");
    for value in data {
        assert!(
            !text.contains(value),
            "no data leaves without its record: {text}"
        );
    }
}

#[tokio::test]
async fn a_summary_is_recorded_as_the_same_query_over_its_rest_is() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let tables = Sink::Accepting.tables(&repository);
    let app = summarising(dir.path(), (&a, &b), &pdq, &tables)?;
    let (status, text) = call(app.clone(), summary("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let fhir = nth_access(&repository, 1).await?;
    let (status, text) = call(app, crate::facade::post(body(&allergies_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let its_rest = nth_access(&repository, 2).await?;
    same_caller_action_and_outcome(&fhir, &its_rest);
    assert_eq!(fhir["meta"], its_rest["meta"], "one BALP pattern");
    assert!(patient(&fhir).is_some(), "the summary names its patient");
    assert_eq!(patient(&fhir), patient(&its_rest), "Annex II 3.2(a): whose");
    assert_eq!(
        entities(&fhir, "ehr", &["endpoint", "patient-lookup"]),
        entities(&its_rest, "ehr", &["endpoint", "patient-lookup"]),
        "every ehr_id at its endpoint"
    );
    let origins = entities(&fhir, "origin", &DATA);
    assert_eq!(
        vec![7, 7],
        origins
            .iter()
            .map(|(_, details)| details.len())
            .collect::<Vec<_>>(),
        "both members, each with what it answered: {origins:?}"
    );
    assert_eq!(
        origins,
        entities(&its_rest, "origin", &DATA),
        "Annex II 3.2(e): every origin, how it answered and what it gave"
    );
    let kept = [DATA.as_slice(), RETENTION.as_slice()].concat();
    assert_eq!(
        entities(&fhir, "ehds-categories", &kept),
        entities(&its_rest, "ehds-categories", &kept),
        "Art 9(2)(c): which data, under which map, kept how long"
    );
    let mut expected = categories(&its_rest);
    expected.extend(by_construction());
    expected.sort_by_key(Value::to_string);
    assert_eq!(
        expected,
        categories(&fhir),
        "Annex II 3.2(c): the categories of the data, and the category the request serves"
    );
    Ok(())
}

#[tokio::test]
async fn a_receipt_is_recorded_as_the_same_write_over_its_rest_is() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = receiving_at_a(dir.path(), (&a, &b), &audit_tables(&repository, ""))?;
    let (status, text) = call(app.clone(), post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let fhir = nth_access(&repository, 1).await?;
    let composition = written_to(&a).await?.first().ok_or("the write")?.clone();
    let write = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header("openEHR-federation-endpoint", "node-a-pub")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(composition))?;
    let (status, text) = call(app, write).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let its_rest = nth_access(&repository, 2).await?;
    same_caller_action_and_outcome(&fhir, &its_rest);
    assert_eq!(
        entities(&fhir, "ehr", &["endpoint"]),
        entities(&its_rest, "ehr", &["endpoint"]),
        "the ehr_id written to, at its endpoint"
    );
    let written = [
        "node",
        "system-id",
        "template-id",
        "archetype-id",
        "category-map-digest",
    ];
    let origins = entities(&fhir, "origin", &written);
    assert_eq!(
        Some(written.len()),
        origins.first().map(|(_, details)| details.len()),
        "one origin naming each: {origins:?}"
    );
    assert_eq!(
        origins,
        entities(&its_rest, "origin", &written),
        "Annex II 3.2(e): the member written to, and what was written"
    );
    let kept = [
        written.as_slice(),
        RETENTION.as_slice(),
        &["ehds-unclassified"],
    ]
    .concat();
    assert_eq!(
        entities(&fhir, "ehds-categories", &kept),
        entities(&its_rest, "ehds-categories", &kept),
        "which data, under which map, kept how long"
    );
    // The receipt reads its patient from the document and the version from the member's
    // answer, which the record of an ITS-REST write addressing an ehr_id leaves unnamed.
    let named = fhir.to_string();
    for receipt_only in [PATIENT, VERSION_A, "Patient-Summaries:construction"] {
        assert!(named.contains(receipt_only), "{receipt_only}: {named}");
    }
    Ok(())
}

#[tokio::test]
async fn a_list_is_recorded_alike_whatever_entries_the_face_writes_of_the_documents() -> TestResult
{
    let hash =
        |document: &[u8]| STANDARD.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, document).as_ref());
    let mut records = Vec::new();
    let mut running = Vec::new();
    for (own, other) in [
        (DOCUMENT_A, DOCUMENT_A_COMPACT),
        (DOCUMENT_A_COMPACT, DOCUMENT_A),
    ] {
        let members = Members::start(fhir_document(own), fhir_document(DOCUMENT_B)).await?;
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = members.gateway(dir.path(), ("", &audit_tables(&repository, "")))?;
        let (status, text) = call(app, list("")?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        assert!(
            text.contains(&hash(own)) && !text.contains(&hash(other)),
            "the interoperability component describes the bytes served: {text}"
        );
        records.push(timeless(nth_access(&repository, 1).await?));
        running.push((members, repository));
    }
    assert_eq!(
        records.first(),
        records.get(1),
        "the record of the access is unchanged by them"
    );
    Ok(())
}

#[tokio::test]
async fn the_summary_is_the_same_document_whatever_the_access_log_sink_does() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let pdq = supplier().await?;
    let mut documents = Vec::new();
    // NOTE: no specification governs this: our own design; a spool is keyed by its repository,
    // so each repository lives to the end and no later one reuses its port.
    let mut repositories = Vec::new();
    for sink in Sink::TAKING {
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = summarising(dir.path(), (&a, &b), &pdq, &sink.tables(&repository))?;
        let (status, text) = call(app, summary("")?).await?;
        assert_eq!(StatusCode::OK, status, "{sink:?}: {text}");
        documents.push((sink, unminted(&text)?));
        repositories.push(repository);
    }
    let (_, expected) = documents.first().ok_or("a document")?;
    for (sink, document) in &documents {
        assert_eq!(expected, document, "{sink:?}: the same bytes");
    }
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "spool_max_events = 1");
    let app = summarising(dir.path(), (&a, &b), &pdq, &tables)?;
    // NOTE: PDQm §2:3.78.5.1; the header's ITI-78 record takes the spool's one place first.
    let refused = call(app, summary("")?).await?;
    withheld(&refused, &[UID_A, UID_B, "Bundle", "AllergyIntolerance"]);
    Ok(())
}

#[tokio::test]
async fn a_received_document_is_written_the_same_whatever_the_access_log_sink_does() -> TestResult {
    let mut written = Vec::new();
    // NOTE: no specification governs this: our own design; a spool is keyed by its repository,
    // so each repository lives to the end and no later one reuses its port.
    let mut repositories = Vec::new();
    for sink in Sink::TAKING {
        let (a, b) = (creating().await, Server::start().await);
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = receiving_at_a(dir.path(), (&a, &b), &sink.tables(&repository))?;
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::OK, status, "{sink:?}: {text}");
        let composition = written_to(&a).await?.first().ok_or("the write")?.clone();
        written.push((format!("{sink:?}"), received_at_any_time(&composition)?));
        repositories.push(repository);
    }
    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "spool_max_events = 1");
    let app = receiving_at_a(dir.path(), (&a, &b), &tables)?;
    let (status, text) = call(app.clone(), post(&document())?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the first record fills the spool: {text}"
    );
    let refused = call(app, post(&document())?).await?;
    withheld(&refused, &[VERSION_A, "node-a"]);
    let compositions = written_to(&a).await?;
    let [_, refused_write] = compositions.as_slice() else {
        return Err(format!("two writes, got {}", compositions.len()).into());
    };
    // NOTE: Annex II 3.2; the record follows the write, so the member holds a composition
    // whose answer the caller was refused, and it is the same composition.
    written.push((
        String::from("Refusing"),
        received_at_any_time(refused_write)?,
    ));
    let (_, expected) = written.first().ok_or("a composition")?;
    for (sink, composition) in &written {
        assert_eq!(expected, composition, "{sink}: the same bytes");
    }
    Ok(())
}
