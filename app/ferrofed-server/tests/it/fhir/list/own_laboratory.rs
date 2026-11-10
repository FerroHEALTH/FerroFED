// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member that serves its laboratory reports itself, as an HL7 Europe EU
//! Health Data API Document Access Provider: asked by its own
//! `ehr_id` alone, `Patient?identifier=` then
//! `DocumentReference?patient=&type=` (§5.4.1, N33); each of its
//! `DocumentReference`s listed as it holds it but for the attachment URL,
//! readdressed to a `Binary/{token}` URL the gateway signs; the read asking
//! it alone for its `Binary` and answering the bytes unchanged; its silence
//! failing the list under all-or-nothing and named under `partial` (§11.3,
//! §11.4); and every list and read recorded (Annex II 3.2).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use eehrxf::document::laboratory::LAB_BUNDLE;
use ferrofed_testkit::leak::{self, Scope};
use ferrofed_testkit::proxy::Fault;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::TestResult;
use super::own_member::{
    BINARY, Declared, MEMBER_PATIENT, Members, NO_PATIENT, PATIENTS, answered, documents, listed,
    url_of,
};
use super::{EHR_SYSTEM, binary, entries, list};
use crate::facade::{EHR_A, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::{call, exchange, field};

/// The laboratory report document type, percent-encoded.
const LABORATORY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C11502-2";

/// The patient summary document type, percent-encoded.
const SUMMARY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C60591-5";

/// The member's own `DocumentReference`, its members in the order the
/// gateway's serialiser writes them, with a decimal of two places, so the
/// listed bytes can be compared whole.
const DOCUMENT: &str = r#"{"category":[{"coding":[{"code":"Laboratory-Reports","system":"http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"}]}],"content":[{"attachment":{"contentType":"application/fhir+json","size":63,"url":"Binary/lab-1"}}],"extension":[{"url":"urn:oid:2.999.9.1","valueDecimal":1.50}],"id":"doc-lab-1","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:1ab00000-0000-4000-8000-0000000000a1"},"resourceType":"DocumentReference","status":"current","subject":{"reference":"Patient/pat-a"},"type":{"coding":[{"code":"11502-2","system":"http://loinc.org"}]}}"#;

/// Node A's own endpoint, serving laboratory reports.
const LABORATORY: Declared = Declared {
    endpoint: "node-a-lab",
    category: "Laboratory-Reports",
    profile: LAB_BUNDLE,
    binary: "lab-1",
    document: "doc-lab-1",
    kind: Some("http://loinc.org|11502-2"),
};

// conformance: CP-26 track-10
#[tokio::test]
async fn a_members_own_report_is_listed_unchanged_but_for_its_url_and_read_unchanged() -> TestResult
{
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(LABORATORY_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (reference, written) = listed(&LABORATORY, &text)?;
    let url = url_of(&reference)?;
    assert!(url.starts_with("Binary/"), "a URL of the gateway's: {url}");
    assert_eq!(
        written,
        DOCUMENT.replace("\"Binary/lab-1\"", &format!("\"{url}\"")),
        "every element but the attachment URL byte for byte, as the member holds it"
    );
    let (status, headers, body) = exchange(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(body, BINARY, "the member's Binary byte for byte");
    assert_eq!(
        field(&headers, "content-type"),
        Some("application/fhir+json")
    );
    let kind = "http://loinc.org|11502-2".to_owned();
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
                    ("type".to_owned(), kind),
                ]
            ),
            ("/fhir/Binary/lab-1".to_owned(), Vec::new()),
        ],
        "the Document Access Provider's SHALL interactions, by node A's ehr_id alone"
    );
    for withheld in [PATIENT, PATIENT_TAIL] {
        leak::assert_withheld(
            &members.proxy.journal(),
            &[withheld],
            Scope::WholeRequest,
            "N33: no identifier in the path, the query or a header",
        );
        let captured = wire(&members.own).await?;
        captured.assert_withholds(&[withheld], "N33");
    }
    assert!(
        received(&members.a).await?.is_empty() && received(&members.b).await?.is_empty(),
        "no laboratory query reaches a Query API: node A serves its own, and the gateway maps none"
    );
    Ok(())
}

#[tokio::test]
async fn the_token_names_the_members_binary_and_no_patient() -> TestResult {
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(LABORATORY_TYPE)?).await?;
    let url = url_of(&listed(&LABORATORY, &text)?.0)?;
    let token = url.strip_prefix("Binary/").ok_or("a Binary URL")?;
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    let mut claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts.get(1).ok_or("a payload")?)?)?;
    assert_eq!(claims["binary"], "lab-1");
    assert_eq!(claims["endpoint"], "node-a-lab");
    assert_eq!(claims["category"], "Laboratory-Reports");
    assert_eq!(claims["ehr_id"], EHR_A);
    assert!(!claims.to_string().contains(PATIENT_TAIL), "N33: {claims}");
    claims["binary"] = Value::from("lab-2");
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
async fn a_summary_type_asks_the_laboratory_member_nothing() -> TestResult {
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(SUMMARY_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!text.contains("doc-lab-1"), "{text}");
    assert!(members.proxy.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_list_of_every_type_names_the_summary_and_the_members_own_report() -> TestResult {
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
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
        codes.contains(&"60591-5") && codes.contains(&"11502-2"),
        "{codes:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_holding_no_patient_lists_no_report_and_is_asked_no_more() -> TestResult {
    let members = Members::start(
        LABORATORY,
        answered(NO_PATIENT.as_bytes()),
        answered(documents(&LABORATORY, DOCUMENT).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(LABORATORY_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert!(entries(&bundle, "match").is_empty(), "{bundle}");
    assert!(entries(&bundle, "outcome").is_empty(), "{bundle}");
    let asked = members.asked();
    assert_eq!(1, asked.len(), "the Patient search alone: {asked:?}");
    Ok(())
}

#[tokio::test]
async fn a_silent_laboratory_member_fails_the_list_and_is_named() -> TestResult {
    let members = Members::start(
        LABORATORY,
        ResponseTemplate::new(500),
        answered(documents(&LABORATORY, DOCUMENT).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(LABORATORY_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        text.contains("the list is incomplete") && text.contains("node-a-lab (node-error)"),
        "the silent member is named: {text}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_an_unreachable_laboratory_member_is_named_in_an_outcome_entry() -> TestResult
{
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    members.proxy.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let mut request = list(LABORATORY_TYPE)?;
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
        diagnostics.contains("node-a-lab (offline)"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn an_attachment_outside_the_members_endpoint_fails_the_list() -> TestResult {
    let elsewhere = DOCUMENT.replace(
        "\"Binary/lab-1\"",
        "\"https://elsewhere.example.org/fhir/Binary/lab-1\"",
    );
    let members = Members::start(
        LABORATORY,
        answered(PATIENTS.as_bytes()),
        answered(documents(&LABORATORY, &elsewhere).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(LABORATORY_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-lab (node-error)"), "{text}");
    assert!(!text.contains("elsewhere.example.org"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_binary_the_member_holds_no_longer_is_not_found_and_a_failed_read_is_named() -> TestResult
{
    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(LABORATORY_TYPE)?).await?;
    let url = url_of(&listed(&LABORATORY, &text)?.0)?;
    for (answer, expected, named) in [
        (404, StatusCode::NOT_FOUND, "holds this document no longer"),
        (
            500,
            StatusCode::FAILED_DEPENDENCY,
            "node-a-lab (node-error)",
        ),
    ] {
        members.own.reset().await;
        Mock::given(method("GET"))
            .and(path("/fhir/Binary/lab-1"))
            .respond_with(ResponseTemplate::new(answer))
            .mount(&members.own)
            .await;
        let (status, text) = call(app.clone(), binary(&url)?).await?;
        assert_eq!(expected, status, "{text}");
        assert!(text.contains(named), "{text}");
    }
    Ok(())
}

/// A synthetic original report in PDF, with bytes that are no UTF-8.
const ORIGINAL_PDF: &[u8] =
    b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n";

// NOTE: MyHealth@EU Requirements Catalogue 10.0.0 08 and 08.01: an Original Clinical Document is
// unmodified after its creation, in PDF, JPG or PNG.
#[tokio::test]
async fn a_members_original_report_in_pdf_is_read_byte_for_byte_with_its_media_type() -> TestResult
{
    let document = DOCUMENT.replace("application/fhir+json", "application/pdf");
    let members = Members::holding(LABORATORY, &document).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(LABORATORY_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (reference, written) = listed(&LABORATORY, &text)?;
    let url = url_of(&reference)?;
    assert_eq!(
        written,
        document.replace("\"Binary/lab-1\"", &format!("\"{url}\"")),
        "the reference keeps the member's media type"
    );
    members.own.reset().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Binary/lab-1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ORIGINAL_PDF, "application/pdf"))
        .mount(&members.own)
        .await;
    let (status, headers, body) = exchange(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(body, ORIGINAL_PDF, "the original byte for byte");
    assert_eq!(field(&headers, "content-type"), Some("application/pdf"));
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn the_list_and_the_read_are_recorded_under_the_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::holding(LABORATORY, DOCUMENT).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let (_, text) = call(app.clone(), list(LABORATORY_TYPE)?).await?;
    let url = url_of(&listed(&LABORATORY, &text)?.0)?;
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
        for named in ["node-a-lab", EHR_A, "Laboratory-Reports"] {
            assert!(record.contains(named), "{named}: {record}");
        }
        assert!(!record.contains("doc-lab-1"), "no document in a record");
    }
    Ok(())
}
