// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member that serves its discharge reports itself, as an HL7 Europe EU
//! Health Data API Document Access Provider: asked by its own `ehr_id`
//! alone, `Patient?identifier=` then `DocumentReference?patient=&type=` with
//! the types of `eehrxf-document-type-discharge-report-vs` the search admits
//! (§5.4.1, N33); each of its `DocumentReference`s listed as it holds it but
//! for the attachment URL, readdressed to a `Binary/{token}` URL the gateway
//! signs; the read asking it alone for its `Binary` and answering the bytes
//! unchanged; its silence failing the list under all-or-nothing and named
//! under `partial` (§11.3, §11.4); and every list and read recorded under
//! `Discharge-Reports` (Annex II 3.2).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use eehrxf::document::discharge::HDR_BUNDLE;
use ferrofed_testkit::proxy::Fault;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::TestResult;
use super::own_member::{
    BINARY, Declared, MEMBER_PATIENT, Members, PATIENTS, answered, documents, listed, url_of,
};
use super::{EHR_SYSTEM, binary, entries, list};
use crate::facade::{EHR_A, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::{call, exchange, field};

/// Both discharge report document types, percent-encoded.
const DISCHARGE_TYPE: &str =
    "&type=http%3A%2F%2Floinc.org%7C18842-5,http%3A%2F%2Floinc.org%7C100719-4";

/// The surgical oncology discharge summary type alone, percent-encoded.
const ONCOLOGY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C100719-4";

/// The laboratory report document type, percent-encoded.
const LABORATORY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C11502-2";

/// The type token the member is asked for when the search admits both
/// discharge report types.
const BOTH: &str = "http://loinc.org|18842-5,http://loinc.org|100719-4";

/// The member's own `DocumentReference`, its members in the order the
/// gateway's serialiser writes them, with a decimal of two places, so the
/// listed bytes can be compared whole.
const DOCUMENT: &str = r#"{"category":[{"coding":[{"code":"Discharge-Reports","system":"http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"}]}],"content":[{"attachment":{"contentType":"application/fhir+json","size":63,"url":"Binary/hdr-1"}}],"extension":[{"url":"urn:oid:2.999.9.2","valueDecimal":2.50}],"id":"doc-hdr-1","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:4d200000-0000-4000-8000-0000000000b1"},"resourceType":"DocumentReference","status":"current","subject":{"reference":"Patient/pat-a"},"type":{"coding":[{"code":"18842-5","system":"http://loinc.org"}]}}"#;

/// Node A's own endpoint, serving discharge reports.
const DISCHARGE: Declared = Declared {
    endpoint: "node-a-hdr",
    category: "Discharge-Reports",
    profile: HDR_BUNDLE,
    binary: "hdr-1",
    document: "doc-hdr-1",
    kind: None,
};

/// The `type` the `DocumentReference` search asked node A's own endpoint
/// for, if it was asked.
fn asked_type(members: &Members) -> Option<String> {
    members
        .asked()
        .into_iter()
        .find(|(path, _)| path == "/fhir/DocumentReference")?
        .1
        .into_iter()
        .find(|(name, _)| name == "type")
        .map(|(_, value)| value)
}

#[tokio::test]
async fn a_members_own_discharge_report_is_listed_unchanged_but_for_its_url_and_read_unchanged()
-> TestResult {
    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (reference, written) = listed(&DISCHARGE, &text)?;
    let url = url_of(&reference)?;
    assert!(url.starts_with("Binary/"), "a URL of the gateway's: {url}");
    assert_eq!(
        written,
        DOCUMENT.replace("\"Binary/hdr-1\"", &format!("\"{url}\"")),
        "every element but the attachment URL byte for byte, as the member holds it"
    );
    let (status, headers, body) = exchange(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(body, BINARY, "the member's Binary byte for byte");
    assert_eq!(
        field(&headers, "content-type"),
        Some("application/fhir+json")
    );
    assert_eq!(
        members.asked(),
        [
            (
                "/fhir/Patient".to_owned(),
                vec![("identifier".to_owned(), format!("{EHR_SYSTEM}|{EHR_A}"))]
            ),
            (
                "/fhir/DocumentReference".to_owned(),
                vec![
                    ("patient".to_owned(), MEMBER_PATIENT.to_owned()),
                    ("type".to_owned(), BOTH.to_owned()),
                ]
            ),
            ("/fhir/Binary/hdr-1".to_owned(), Vec::new()),
        ],
        "the Document Access Provider's SHALL interactions, by node A's ehr_id alone"
    );
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !members.proxy.journal_contains(withheld.as_bytes()),
            "N33: no identifier in the path, the query or a header"
        );
        let captured = wire(&members.own).await?;
        assert!(!captured.contains(withheld), "N33: {captured}");
    }
    assert!(
        received(&members.a).await?.is_empty() && received(&members.b).await?.is_empty(),
        "no query reaches a Query API: node A serves its own reports, and the gateway maps none"
    );
    Ok(())
}

#[tokio::test]
async fn the_member_is_asked_for_the_discharge_types_the_search_admits() -> TestResult {
    for (extra, asked) in [
        (ONCOLOGY_TYPE, "http://loinc.org|100719-4"),
        ("&type=18842-5", "http://loinc.org|18842-5"),
        ("&type=http%3A%2F%2Floinc.org%7C", BOTH),
        ("", BOTH),
    ] {
        let members = Members::holding(DISCHARGE, DOCUMENT).await?;
        let dir = tempfile::tempdir()?;
        let app = members.gateway(dir.path(), ("", ""))?;
        let (status, text) = call(app, list(extra)?).await?;
        assert_eq!(StatusCode::OK, status, "{extra}: {text}");
        assert_eq!(
            asked_type(&members).as_deref(),
            Some(asked),
            "{extra}: the value set's codes the search admits, ORed"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_token_names_the_members_binary_in_the_discharge_category_and_no_patient() -> TestResult
{
    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&DISCHARGE, &text)?.0)?;
    let token = url.strip_prefix("Binary/").ok_or("a Binary URL")?;
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    let mut claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts.get(1).ok_or("a payload")?)?)?;
    assert_eq!(claims["binary"], "hdr-1");
    assert_eq!(claims["endpoint"], "node-a-hdr");
    assert_eq!(claims["category"], "Discharge-Reports");
    assert_eq!(claims["profile"], HDR_BUNDLE);
    assert_eq!(claims["ehr_id"], EHR_A);
    assert!(!claims.to_string().contains(PATIENT_TAIL), "N33: {claims}");
    claims["category"] = Value::from("Laboratory-Reports");
    if let Some(slot) = parts.get_mut(1) {
        *slot = URL_SAFE_NO_PAD.encode(claims.to_string());
    }
    members.proxy.clear_journal();
    let (status, text) = call(app, binary(&format!("Binary/{}", parts.join(".")))?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(members.proxy.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_summary_or_laboratory_type_asks_the_discharge_member_nothing() -> TestResult {
    for extra in ["&type=http%3A%2F%2Floinc.org%7C60591-5", LABORATORY_TYPE] {
        let members = Members::holding(DISCHARGE, DOCUMENT).await?;
        let dir = tempfile::tempdir()?;
        let app = members.gateway(dir.path(), ("", ""))?;
        let (status, text) = call(app, list(extra)?).await?;
        assert_eq!(StatusCode::OK, status, "{extra}: {text}");
        assert!(!text.contains("doc-hdr-1"), "{extra}: {text}");
        assert!(
            members.proxy.journal().is_empty(),
            "{extra}: nothing is sent"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_list_of_every_type_names_the_summary_and_the_members_own_discharge_report() -> TestResult
{
    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let codes: Vec<&str> = entries(&bundle, "match")
        .iter()
        .filter_map(|reference| reference.pointer("/type/coding/0/code")?.as_str())
        .collect();
    assert_eq!(codes.len(), 2, "{bundle}");
    assert!(
        codes.contains(&"60591-5") && codes.contains(&"18842-5"),
        "{codes:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_silent_discharge_member_fails_the_list_and_is_named() -> TestResult {
    let members = Members::start(
        DISCHARGE,
        ResponseTemplate::new(500),
        answered(documents(&DISCHARGE, DOCUMENT).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(DISCHARGE_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        text.contains("the list is incomplete") && text.contains("node-a-hdr (node-error)"),
        "the silent member is named: {text}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_an_unreachable_discharge_member_is_named_in_an_outcome_entry() -> TestResult
{
    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    members.proxy.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let mut request = list(DISCHARGE_TYPE)?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert!(entries(&bundle, "match").is_empty(), "{bundle}");
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("node-a-hdr (offline)"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn an_attachment_outside_the_members_endpoint_is_named_as_unpassable_discharge_reports()
-> TestResult {
    let elsewhere = DOCUMENT.replace(
        "\"Binary/hdr-1\"",
        "\"https://elsewhere.example.org/fhir/Binary/hdr-1\"",
    );
    let members = Members::start(
        DISCHARGE,
        answered(PATIENTS.as_bytes()),
        answered(documents(&DISCHARGE, &elsewhere).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let (status, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-hdr (node-error)"), "{text}");
    let mut request = list(DISCHARGE_TYPE)?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    assert!(
        text.contains(
            "node-a-hdr (node-error) answered discharge reports the list cannot pass through"
        ),
        "{text}"
    );
    assert!(!text.contains("elsewhere.example.org"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_discharge_report_the_member_holds_no_longer_is_not_found() -> TestResult {
    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&DISCHARGE, &text)?.0)?;
    members.own.reset().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Binary/hdr-1"))
        .respond_with(ResponseTemplate::new(410))
        .mount(&members.own)
        .await;
    let (status, text) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(text.contains("holds this document no longer"), "{text}");
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn the_discharge_list_and_read_are_recorded_under_the_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::holding(DISCHARGE, DOCUMENT).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&DISCHARGE, &text)?.0)?;
    let (status, _, _) = exchange(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status);
    let records: Vec<String> = repository
        .wait_for(2, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(
        2,
        records.len(),
        "one record for the list, one for the read: {records:#?}"
    );
    for record in &records {
        for named in ["node-a-hdr", EHR_A, "Discharge-Reports"] {
            assert!(record.contains(named), "{named}: {record}");
        }
        assert!(!record.contains("doc-hdr-1"), "no document in a record");
    }
    Ok(())
}
