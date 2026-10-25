// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient lookup on the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1; Xt-EHR D5.1 §3.4.6 `api-producer-patient`):
//! `GET {fhir-base}/Patient?identifier=system|value` answers a `searchset`
//! with the one `Patient` the PDQm Supplier holds when a member holds the
//! patient; an identifier no member holds, one the Supplier does not know and
//! a patient every member restricts answer alike, with the empty
//! `searchset` (§11.3, Art 8); no member is asked anything, so no patient
//! identifier reaches a node (§5.4.1, N33); a search by demographics is
//! refused naming the parameter the face takes; the lookup is authenticated
//! and recorded (Annex II 3.2); and `metadata` declares it.

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use http::{Request, StatusCode};
use serde_json::Value;

use super::{
    BIRTH_DATE, FAMILY, FHIR, GIVEN, TestResult, UID_A, UID_B, gateway, gateway_over, node_holding,
    supplier,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received};
use crate::support::{call, send_as_is};

/// `GET {fhir-base}/Patient` with the query `query`.
fn lookup(query: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/Patient?{query}")).body(Body::empty())
}

/// The lookup of the test patient by its identifier.
fn of_patient() -> Result<Request<Body>, http::Error> {
    lookup(&format!(
        "identifier={}%7C{PATIENT}",
        NAMESPACE.replace(':', "%3A")
    ))
}

/// A `[[dev.consent_denied]]` row denying the test patient at `member`.
fn denied(member: &str) -> String {
    format!(
        "\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"{member}\"\n"
    )
}

/// A `[[dev.crossref]]` row knowing another synthetic patient at node A, so
/// the cross-reference is configured and holds the test patient nowhere.
fn another_patient() -> String {
    format!(
        "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"SYNTHETIC-OTHER-2\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    )
}

/// The two members, the Supplier, and a gateway over them knowing the patient
/// at the members `rows` name, with the tables `tables`.
async fn over(
    dir: &std::path::Path,
    rows: &[(&str, &str)],
    tables: &str,
) -> Result<(Router, Server, Server, Server), Box<dyn std::error::Error>> {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let pdq = supplier().await;
    let (app, _) = gateway_over(
        dir,
        (&a.uri(), &b.uri(), &pdqm_stub::base_url(&pdq)),
        rows,
        ("", tables),
    )?;
    Ok((app, a, b, pdq))
}

/// Whether neither member was asked anything.
async fn nothing_sent(a: &Server, b: &Server) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(received(a).await?.is_empty() && received(b).await?.is_empty())
}

#[tokio::test]
async fn a_patient_a_member_holds_is_found_as_the_supplier_holds_it_and_no_member_is_asked()
-> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app, of_patient()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(bundle["type"], "searchset", "{bundle}");
    assert_eq!(bundle["total"], Value::from(1), "{bundle}");
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    let [entry] = entries.as_slice() else {
        return Err(format!("one entry: {bundle}").into());
    };
    assert_eq!(entry.pointer("/search/mode"), Some(&Value::from("match")));
    let patient = &entry["resource"];
    assert_eq!(patient["resourceType"], "Patient", "{patient}");
    assert_eq!(
        patient.pointer("/identifier/0/system"),
        Some(&Value::from(NAMESPACE))
    );
    assert_eq!(
        patient.pointer("/identifier/0/value"),
        Some(&Value::from(PATIENT))
    );
    assert_eq!(
        patient.pointer("/name/0/family"),
        Some(&Value::from(FAMILY)),
        "{patient}"
    );
    assert_eq!(
        patient.pointer("/name/0/given/0"),
        Some(&Value::from(GIVEN))
    );
    assert_eq!(patient["birthDate"], BIRTH_DATE);
    let id = patient["id"].as_str().ok_or("an id")?;
    assert_eq!(
        entry["fullUrl"].as_str(),
        Some(format!("urn:uuid:{id}").as_str())
    );
    assert!(
        nothing_sent(&a, &b).await?,
        "§5.4.1, N33: the lookup asks no member"
    );
    Ok(())
}

#[tokio::test]
async fn an_identifier_no_member_holds_and_a_restricted_patient_answer_alike() -> TestResult {
    let unknown_dir = tempfile::tempdir()?;
    let (unknown, a, b, _pdq) = over(unknown_dir.path(), &[], &another_patient()).await?;
    let (status, held_nowhere) = call(unknown, of_patient()?).await?;
    assert_eq!(StatusCode::OK, status, "{held_nowhere}");
    assert!(nothing_sent(&a, &b).await?);

    let restricted_dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let tables = format!("{}{}", denied("node-a"), denied("node-b"));
    let (restricted, a, b, _pdq) = over(restricted_dir.path(), &rows, &tables).await?;
    let (status, every_member_restricts) = call(restricted, of_patient()?).await?;
    assert_eq!(StatusCode::OK, status, "{every_member_restricts}");
    assert!(nothing_sent(&a, &b).await?);

    let other_dir = tempfile::tempdir()?;
    let (unknown_to_supplier, a, b, _pdq) = over(other_dir.path(), &rows, "").await?;
    let (status, supplier_knows_none) = call(
        unknown_to_supplier,
        lookup("identifier=urn%3Aoid%3A2.999.1%7CSYNTHETIC-OTHER-1")?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{supplier_knows_none}");
    assert!(nothing_sent(&a, &b).await?);

    let empty: Value = serde_json::from_str(&held_nowhere)?;
    assert_eq!(empty["type"], "searchset");
    assert_eq!(empty["total"], Value::from(0));
    assert!(empty.get("entry").is_none(), "{empty}");
    assert_eq!(
        held_nowhere, every_member_restricts,
        "Art 8, §11.3: a restricted patient answers as one no member holds"
    );
    assert_eq!(held_nowhere, supplier_knows_none);
    for trace in ["consent", "restrict", "denied", "withheld"] {
        assert!(
            !every_member_restricts.to_ascii_lowercase().contains(trace),
            "{every_member_restricts}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_search_by_demographics_or_without_an_identifier_is_refused_naming_identifier()
-> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    for query in [
        String::new(),
        format!("family={FAMILY}"),
        format!("family={FAMILY}&birthdate={BIRTH_DATE}"),
        format!("name={GIVEN}&gender=female"),
        format!("identifier=urn%3Aoid%3A2.999.1%7C{PATIENT}&family={FAMILY}"),
        format!("identifier={PATIENT}"),
        String::from("_id=synthetic-1"),
    ] {
        let (status, text) = call(app.clone(), lookup(&query)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        let outcome: Value = serde_json::from_str(&text)?;
        assert_eq!(outcome["resourceType"], "OperationOutcome", "{text}");
        let diagnostics = outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .ok_or("diagnostics")?;
        assert!(
            diagnostics.contains("identifier"),
            "{query}: names the parameter the face takes: {diagnostics}"
        );
        assert!(!text.contains(PATIENT_TAIL), "{text}");
        assert!(!text.contains(FAMILY), "{text}");
    }
    assert!(nothing_sent(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_lookup_with_no_credential_is_refused() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let response = send_as_is(app, of_patient()?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "§13.1, N25");
    assert!(nothing_sent(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_the_lookup_by_identifier() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(
        app,
        Request::get(format!("{FHIR}/metadata")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let statement: Value = serde_json::from_str(&text)?;
    let patient = statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|resource| resource["type"] == "Patient")
        .ok_or("the Patient resource")?;
    assert_eq!(
        patient.pointer("/interaction/0/code"),
        Some(&Value::from("search-type"))
    );
    let parameters: Vec<(&str, &str)> = patient["searchParam"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|parameter| {
            Some((
                parameter["name"].as_str()?,
                parameter["definition"].as_str()?,
            ))
        })
        .collect();
    assert_eq!(
        parameters,
        [(
            "identifier",
            "http://hl7.org/fhir/SearchParameter/Patient-identifier"
        )]
    );
    assert_eq!(
        patient.pointer("/operation/0/name"),
        Some(&Value::from("summary")),
        "the summary operation stays declared"
    );
    Ok(())
}

/// Annex II 3.2, "every access event": a lookup that finds the patient tells
/// the caller that members hold the patient's record, so it is recorded once,
/// naming the patient's `ehr_id` at each member it resolved at, though no
/// member is asked.
#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_lookup_that_finds_the_patient_is_recorded_once_naming_where_it_resolved() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let (app, a, b, _pdq) = over(dir.path(), &rows, &tables).await?;
    let (status, text) = call(app, of_patient()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    // The PDQm exchange's own audit record can arrive first, so the wait is for the
    // access record itself.
    let until = std::time::Instant::now() + SETTLE;
    let mut records: Vec<String> = Vec::new();
    while records.is_empty() && std::time::Instant::now() < until {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        records = repository
            .records()
            .into_iter()
            .filter(|record| record.contains("ehds-categories"))
            .collect();
    }
    assert_eq!(1, records.len(), "one access record: {records:#?}");
    let record = records.first().ok_or("a record")?;
    for named in ["node-a-pub", "node-b-pub", EHR_A, EHR_B, "ehds-no-category"] {
        assert!(record.contains(named), "{named}: {record}");
    }
    assert!(nothing_sent(&a, &b).await?, "no member was asked");
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_lookup_that_finds_no_patient_writes_no_record() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let (app, _a, _b, _pdq) =
        over(dir.path(), &[], &format!("{}{tables}", another_patient())).await?;
    let (status, text) = call(app, of_patient()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(usize::MAX, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert!(
        records.is_empty(),
        "nothing about the patient was disclosed: {records:#?}"
    );
    Ok(())
}
