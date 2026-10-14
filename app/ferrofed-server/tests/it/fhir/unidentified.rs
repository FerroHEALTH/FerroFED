// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member whose own answer is no FHIR document `Bundle` with an
//! identifier that has a system and a value (FHIR R4 `bdl-9`), which the
//! HL7 Europe EU Health Data API `EehrxfMhdDocumentReference` takes as its
//! required `masterIdentifier`: the list never names its answer as a
//! document, fails under all-or-nothing as for a member that did not answer,
//! and names the member in an `outcome` entry under `partial` (§11.1,
//! §11.3, §11.4); and `Binary` refuses such an answer alike.

use http::StatusCode;
use serde_json::Value;
use wiremock::ResponseTemplate;

use super::TestResult;
use super::list::{
    DOCUMENT_A, DOCUMENT_B, Members, binary, by_custodian, entries, fhir_document, findings, list,
    names,
};
use crate::facade::PATIENT_TAIL;
use crate::support::call;

/// A document with no `Bundle.identifier`.
const UNIDENTIFIED: &[u8] = b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"entry\":[]}";

/// A document whose identifier has a value and no system.
const NO_SYSTEM: &[u8] = b"{\"resourceType\":\"Bundle\",\"identifier\":{\"value\":\"synthetic-doc-x\"},\"type\":\"document\",\"entry\":[]}";

/// A document whose identifier has a system and no value.
const NO_VALUE: &[u8] = b"{\"resourceType\":\"Bundle\",\"identifier\":{\"system\":\"urn:oid:2.999.4.3\"},\"type\":\"document\",\"entry\":[]}";

/// Answers that are no FHIR document at all: no JSON, another resource, and
/// a `Bundle` of another type, each with an identifier.
const NOT_DOCUMENTS: [&[u8]; 3] = [
    b"synthetic text that is no FHIR resource",
    b"{\"resourceType\":\"Patient\",\"identifier\":[{\"system\":\"urn:oid:2.999.4.3\",\"value\":\"synthetic-doc-x\"}]}",
    b"{\"resourceType\":\"Bundle\",\"identifier\":{\"system\":\"urn:oid:2.999.4.3\",\"value\":\"synthetic-doc-x\"},\"type\":\"collection\",\"entry\":[]}",
];

/// Holds the outcome `outcome` to name node `endpoint` in `node-error`, its
/// diagnostics saying `why`.
fn names_the_member(outcome: &Value, endpoint: &str, why: &str) -> TestResult {
    assert_eq!(outcome["resourceType"], "OperationOutcome", "{outcome}");
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("incomplete")),
        "{outcome}"
    );
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains(&format!("{endpoint} (node-error)")) && diagnostics.contains(why),
        "the member is named, {why}: {diagnostics}"
    );
    Ok(())
}

/// Holds the list to fail under all-or-nothing when node A answers with
/// `answer`, naming node A and listing nothing.
async fn fails_the_list(answer: ResponseTemplate) -> TestResult {
    let members = Members::start(answer, fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    names_the_member(&outcome, "node-a-eps", "the list is incomplete")?;
    assert!(
        !text.contains("synthetic-doc") && !text.contains("DocumentReference"),
        "no document is listed: {text}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

/// Holds the list under `partial` to name node A in an `outcome` entry when
/// it answers with `answer`, and to list the summary written on request and
/// node B's document alone, each held to the profile with no finding.
async fn names_it_under_partial(answer: ResponseTemplate) -> TestResult {
    let members = Members::start(answer, fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let mut request = list("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    assert_eq!(
        2,
        matches.len(),
        "the summary written and node B's: {bundle}"
    );
    assert_eq!(bundle.get("total"), Some(&Value::from(2)), "{bundle}");
    assert!(by_custodian(&matches, "org-a").is_none(), "{bundle}");
    let own = by_custodian(&matches, "org-b").ok_or("node B's document")?;
    names(own, DOCUMENT_B)?;
    for reference in &matches {
        assert!(
            reference.pointer("/masterIdentifier/value").is_some(),
            "bdl-9: {reference}"
        );
        let found = findings(reference)?;
        assert!(found.is_empty(), "{found:#?}");
    }
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    names_the_member(outcome, "node-a-eps", "bdl-9")
}

#[tokio::test]
async fn a_document_with_no_identifier_fails_the_list() -> TestResult {
    fails_the_list(fhir_document(UNIDENTIFIED)).await
}

#[tokio::test]
async fn a_document_whose_identifier_has_no_system_fails_the_list() -> TestResult {
    fails_the_list(fhir_document(NO_SYSTEM)).await
}

#[tokio::test]
async fn a_document_whose_identifier_has_no_value_fails_the_list() -> TestResult {
    fails_the_list(fhir_document(NO_VALUE)).await
}

#[tokio::test]
async fn an_answer_that_is_no_fhir_document_fails_the_list() -> TestResult {
    for answer in NOT_DOCUMENTS {
        fails_the_list(fhir_document(answer)).await?;
    }
    fails_the_list(ResponseTemplate::new(200).set_body_raw(NOT_DOCUMENTS[0].to_vec(), "text/plain"))
        .await
}

#[tokio::test]
async fn under_partial_a_document_with_no_identifier_is_named_in_an_outcome_entry() -> TestResult {
    names_it_under_partial(fhir_document(UNIDENTIFIED)).await
}

#[tokio::test]
async fn under_partial_a_document_whose_identifier_has_no_system_is_named() -> TestResult {
    names_it_under_partial(fhir_document(NO_SYSTEM)).await
}

#[tokio::test]
async fn under_partial_a_document_whose_identifier_has_no_value_is_named() -> TestResult {
    names_it_under_partial(fhir_document(NO_VALUE)).await
}

#[tokio::test]
async fn under_partial_an_answer_that_is_no_fhir_document_is_named() -> TestResult {
    for answer in NOT_DOCUMENTS {
        names_it_under_partial(fhir_document(answer)).await?;
    }
    Ok(())
}

/// Holds `Binary` to refuse node B's document, listed while it was whole,
/// once node B answers with `answer`, with none of its bytes.
async fn refuses_the_read(answer: &[u8]) -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let own = by_custodian(&matches, "org-b").ok_or("node B's document")?;
    let url = names(own, DOCUMENT_B)?;
    members.own_b.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(fhir_document(answer))
        .mount(&members.own_b)
        .await;
    let (status, text) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    names_the_member(&outcome, "node-b-eps", "bdl-9")?;
    assert!(
        !text.contains("synthetic-doc") && !text.contains("synthetic text"),
        "none of the member's bytes: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_read_of_a_document_with_no_identifier_is_refused() -> TestResult {
    refuses_the_read(UNIDENTIFIED).await
}

#[tokio::test]
async fn a_read_of_a_document_whose_identifier_has_no_system_or_value_is_refused() -> TestResult {
    refuses_the_read(NO_SYSTEM).await?;
    refuses_the_read(NO_VALUE).await
}

#[tokio::test]
async fn a_read_of_an_answer_that_is_no_fhir_document_is_refused() -> TestResult {
    for answer in NOT_DOCUMENTS {
        refuses_the_read(answer).await?;
    }
    Ok(())
}

/// The `kind` details of the entities of `record` named `name`, of the one
/// whose identifier is `id` when it is given.
fn details(record: &Value, (name, id): (&str, Option<&str>), kind: &str) -> Vec<String> {
    record
        .get("entity")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entity| entity.get("name") == Some(&Value::from(name)))
        .filter(|entity| {
            id.is_none_or(|id| entity.pointer("/what/identifier/value") == Some(&Value::from(id)))
        })
        .flat_map(|entity| {
            entity
                .get("detail")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|detail| detail.get("type") == Some(&Value::from(kind)))
                .filter_map(|detail| detail.get("valueString").and_then(Value::as_str))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn under_partial_the_list_record_counts_no_document_it_left_out() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::start(fhir_document(UNIDENTIFIED), fhir_document(DOCUMENT_B)).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("best_effort = true\n", &tables))?;
    let mut request = list("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let records: Vec<Value> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .map(|record| serde_json::from_str(&record))
        .collect::<Result<_, _>>()?;
    let [record] = records.as_slice() else {
        return Err(format!("one access record of the list, got {records:#?}").into());
    };
    assert_eq!(
        vec!["1"],
        details(record, ("ehds-categories", None), "delivered"),
        "Annex II 3.2: node B's document alone was delivered: {record}"
    );
    assert_eq!(
        vec!["node-error"],
        details(record, ("origin", Some("node-a-eps")), "status"),
        "{record}"
    );
    assert_eq!(
        vec!["active"],
        details(record, ("origin", Some("node-b-eps")), "status"),
        "{record}"
    );
    Ok(())
}
