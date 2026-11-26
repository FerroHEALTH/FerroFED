// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vaccination history the FHIR face writes (MyHealth@EU Requirements
//! Catalogue v10.0.0 §1.24), end to end over FerroBRIDGE's ADL 2 patient
//! summary template declared in `[fhir.vaccination]`: a member's attested
//! immunisation list listed under LOINC 11369-6 and read as a MyHealth@EU
//! `bundle-euvac-myhealtheu` whose legal authenticator is the person who
//! attested the record, the member asked by its own `ehr_id` alone (§5.4.1,
//! N33), and a history that cannot be written listed unavailable with its
//! reason.

use std::error::Error;
use std::path::Path;

use eehrxf::document::vaccination::EUVAC_BUNDLE;
use ferrofed_testkit::ehdsi_language;
use ferrofed_testkit::medication;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub::{self, Entry};
use ferrofed_testkit::vaccination;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::fhir_tables_with;
use super::{gateway_with, rows};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, settings_with_room, wire};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::support::call;

/// The version of node A's immunisation list, created at node A.
const VERSION: &str = "4f5c1d1e-0000-4000-8000-000000001980::cdr-a.example.org::1";

/// The versioned object of [`VERSION`].
const OBJECT: &str = "4f5c1d1e-0000-4000-8000-000000001980";

/// The `[fhir.vaccination]` table of the vendored patient summary template.
fn declared() -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string()).to_string();
    let archetypes = medication::patient_summary_archetype_paths()
        .iter()
        .map(|file| path(file))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n[fhir.vaccination]\ntemplate = {}\narchetypes = [{archetypes}]\n",
        path(&medication::patient_summary_template_path()),
    )
}

/// A node answering the vaccination history query with `rows` and every
/// other query with none, and the read of its version with `version`.
async fn holding(rows: &str, version: Option<&str>) -> Server {
    let server = Server::start().await;
    let columns = r#"[{"name":"composition","path":"c"},{"name":"uid","path":"c/uid/value"},{"name":"template_id","path":"c/archetype_details/template_id/value"},{"name":"attestations","path":"v/attestations"}]"#;
    let answer = |rows: &str| {
        ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"q":"node","columns":{columns},"rows":[{rows}]}}"#).into_bytes(),
            "application/json",
        )
    };
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(ferrofed_eehrxf::vaccination::SECTION))
        .respond_with(answer(rows))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(answer(""))
        .mount(&server)
        .await;
    if let Some(version) = version {
        Mock::given(method("GET"))
            .and(path(format!(
                "/v1/ehr/{EHR_A}/versioned_composition/{OBJECT}/version/{VERSION}"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(version.as_bytes().to_vec(), "application/json"),
            )
            .mount(&server)
            .await;
    }
    server
}

/// The row node A answers the query with, its version attested by
/// `attestations`, a JSON array.
fn row(attestations: &str) -> String {
    format!(
        r#"[{},"{VERSION}","{}",{attestations}]"#,
        vaccination::composition(VERSION),
        medication::PATIENT_SUMMARY_TEMPLATE
    )
}

/// The gateway writing vaccination histories over nodes `a` and `b`, the
/// patient known at both, the header asked of the PDQm Supplier `pdq`.
async fn gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    pdq: &Server,
) -> Result<(axum::Router, Server), Box<dyn Error>> {
    let terminology = ehdsi_language::start().await;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    let tables = fhir_tables_with(
        &pdqm_stub::base_url(pdq),
        &format!("receive_namespaces = [\"{NAMESPACE}\"]"),
        &format!("{}{}", declared(), ehdsi_language::table(&terminology)),
    );
    let app = gateway_with(dir, (a, b), (&crossref, &tables), &settings_with_room())?;
    Ok((app, terminology))
}

/// The test patient as the Supplier answers with it, named, with the birth
/// date `birth_date`.
async fn supplier(birth_date: Option<&str>) -> Server {
    let known = Entry::new(&[(NAMESPACE, PATIENT)])
        .named(("SENTINEL-FAMILY-v4c9", "SENTINEL-GIVEN-v4c9"), birth_date);
    pdqm_stub::answering(std::slice::from_ref(&known), std::slice::from_ref(&known)).await
}

#[tokio::test]
async fn an_attested_immunisation_list_is_listed_and_read_as_a_vaccination_history() -> TestResult {
    let attested = format!("[{}]", vaccination::attestation(false));
    let version = vaccination::original_version(VERSION, &attested);
    let (a, b) = (
        holding(&row(&attested), Some(&version)).await,
        holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let pdq = supplier(Some("1970-01-01")).await;
    let (app, _terminology) = gateway(dir.path(), (&a, &b), &pdq).await?;
    let (status, text) = call(app.clone(), list("&type=11369-6")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one vaccination history listed: {bundle}").into());
    };
    assert_eq!(
        reference.pointer("/type/coding/0/code"),
        Some(&Value::from("11369-6"))
    );
    assert_eq!(
        reference.pointer("/content/0/format/code"),
        Some(&Value::from(EUVAC_BUNDLE))
    );
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, text) = call(app, binary(url)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let document: Value = serde_json::from_str(&text)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{OBJECT}"))),
        "the history is named as the list names it"
    );
    let composition = &document["entry"][0]["resource"];
    assert_eq!(composition["attester"][0]["mode"], "legal", "{composition}");
    assert_eq!(composition["attester"][0]["time"], vaccination::ATTESTED);
    let kinds: Vec<&str> = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.pointer("/resource/resourceType")?.as_str())
        .collect();
    for kind in [
        "Composition",
        "Patient",
        "Immunization",
        "Practitioner",
        "Provenance",
    ] {
        assert!(kinds.contains(&kind), "{kind}: {kinds:?}");
    }
    wire(&a).await?.assert_withholds(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier reaches node A",
    );
    Ok(())
}

/// The outcomes the list of `type=11369-6` names over node A answering
/// `row`, the header asked of `pdq`, and the matches it names.
async fn listed(row: &str, pdq: &Server) -> Result<(Vec<Value>, Vec<Value>), Box<dyn Error>> {
    let (a, b) = (holding(row, None).await, holding("", None).await);
    let dir = tempfile::tempdir()?;
    let (app, _terminology) = gateway(dir.path(), (&a, &b), pdq).await?;
    let (status, text) = call(app, list("&type=11369-6")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    Ok((
        entries(&bundle, "match").into_iter().cloned().collect(),
        entries(&bundle, "outcome").into_iter().cloned().collect(),
    ))
}

/// Whether one of `outcomes` names a vaccination history unavailable for
/// `reason`.
fn unavailable(outcomes: &[Value], reason: &str) -> bool {
    outcomes.iter().any(|outcome| {
        outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("vaccination history") && text.contains(reason))
    })
}

#[tokio::test]
async fn a_version_no_person_attested_is_listed_unavailable_with_its_reason() -> TestResult {
    let pdq = supplier(Some("1970-01-01")).await;
    for attestations in [
        String::from("[]"),
        format!("[{}]", vaccination::attestation(true)),
    ] {
        let (matches, outcomes) = listed(&row(&attestations), &pdq).await?;
        assert!(matches.is_empty(), "{matches:?}");
        assert!(
            unavailable(&outcomes, "no attestation"),
            "{attestations}: {outcomes:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_patient_with_no_birth_date_gets_the_history_listed_unavailable() -> TestResult {
    let pdq = supplier(None).await;
    let attested = format!("[{}]", vaccination::attestation(false));
    let (matches, outcomes) = listed(&row(&attested), &pdq).await?;
    assert!(matches.is_empty(), "{matches:?}");
    assert!(unavailable(&outcomes, "birth date"), "{outcomes:?}");
    Ok(())
}
