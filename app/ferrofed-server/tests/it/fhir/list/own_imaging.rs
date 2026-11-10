// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member that serves its imaging reports itself, as an HL7 Europe EU
//! Health Data API Document Access Provider: asked by its own `ehr_id`
//! alone, `Patient?identifier=` then `DocumentReference?patient=&type=` with
//! the types of `eehrxf-document-type-medical-imaging-vs` the search admits
//! (§5.4.1, N33); each of its `DocumentReference`s listed as it holds it but
//! for the attachment URL, readdressed to a `Binary/{token}` URL the gateway
//! signs; the read asking it alone for its `Binary` and answering the bytes
//! unchanged; its silence failing the list under all-or-nothing and named
//! under `partial` (§11.3, §11.4); and every list and read recorded under
//! `Medical-Imaging` (Annex II 3.2).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use eehrxf::document::imaging::IMAGING_BUNDLE;
use ferrofed_testkit::leak::{self, Scope};
use ferrofed_testkit::proxy::Fault;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::TestResult;
use super::own_member::{
    BINARY, Declared, MEMBER_PATIENT, Members, PATIENTS, answered, asked_type, documents, listed,
    url_of,
};
use super::{EHR_SYSTEM, binary, entries, list};
use crate::facade::{EHR_A, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::{call, exchange, field};

/// Both imaging document types, percent-encoded.
const IMAGING_TYPE: &str =
    "&type=http%3A%2F%2Floinc.org%7C85430-7,http%3A%2F%2Floinc.org%7C18748-4";

/// The diagnostic imaging study type alone, percent-encoded.
const STUDY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C18748-4";

/// The laboratory report document type, percent-encoded.
const LABORATORY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C11502-2";

/// The type token the member is asked for when the search admits both
/// imaging report types.
const BOTH: &str = "http://loinc.org|85430-7,http://loinc.org|18748-4";

/// The member's own `DocumentReference`, its members in the order the
/// gateway's serialiser writes them, with a decimal of two places, so the
/// listed bytes can be compared whole.
const DOCUMENT: &str = r#"{"category":[{"coding":[{"code":"Medical-Imaging","system":"http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"}]}],"content":[{"attachment":{"contentType":"application/fhir+json","size":63,"url":"Binary/img-1"}}],"extension":[{"url":"urn:oid:2.999.9.3","valueDecimal":3.50}],"id":"doc-img-1","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:1a900000-0000-4000-8000-0000000000b1"},"resourceType":"DocumentReference","status":"current","subject":{"reference":"Patient/pat-a"},"type":{"coding":[{"code":"85430-7","system":"http://loinc.org"}]}}"#;

/// Node A's own endpoint, serving imaging reports.
const IMAGING: Declared = Declared {
    endpoint: "node-a-img",
    category: "Medical-Imaging",
    profile: IMAGING_BUNDLE,
    binary: "img-1",
    document: "doc-img-1",
    kind: None,
};

#[tokio::test]
async fn a_members_own_imaging_report_is_listed_unchanged_but_for_its_url_and_read_unchanged()
-> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (reference, written) = listed(&IMAGING, &text)?;
    let url = url_of(&reference)?;
    assert!(url.starts_with("Binary/"), "a URL of the gateway's: {url}");
    assert_eq!(
        written,
        DOCUMENT.replace("\"Binary/img-1\"", &format!("\"{url}\"")),
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
            ("/fhir/Binary/img-1".to_owned(), Vec::new()),
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
        "no query reaches a Query API: node A serves its own reports, and the gateway maps none"
    );
    Ok(())
}

#[tokio::test]
async fn the_member_is_asked_for_the_imaging_types_the_search_admits() -> TestResult {
    for (extra, asked) in [
        (STUDY_TYPE, "http://loinc.org|18748-4"),
        ("&type=85430-7", "http://loinc.org|85430-7"),
        ("&type=http%3A%2F%2Floinc.org%7C", BOTH),
        ("", BOTH),
    ] {
        let members = Members::holding(IMAGING, DOCUMENT).await?;
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
async fn the_token_names_the_members_binary_in_the_imaging_category_and_no_patient() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    let url = url_of(&listed(&IMAGING, &text)?.0)?;
    let token = url.strip_prefix("Binary/").ok_or("a Binary URL")?;
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    let mut claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts.get(1).ok_or("a payload")?)?)?;
    assert_eq!(claims["binary"], "img-1");
    assert_eq!(claims["endpoint"], "node-a-img");
    assert_eq!(claims["category"], "Medical-Imaging");
    assert_eq!(claims["profile"], IMAGING_BUNDLE);
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
async fn a_summary_laboratory_or_discharge_type_asks_the_imaging_member_nothing() -> TestResult {
    for extra in [
        "&type=http%3A%2F%2Floinc.org%7C60591-5",
        LABORATORY_TYPE,
        "&type=http%3A%2F%2Floinc.org%7C18842-5",
    ] {
        let members = Members::holding(IMAGING, DOCUMENT).await?;
        let dir = tempfile::tempdir()?;
        let app = members.gateway(dir.path(), ("", ""))?;
        let (status, text) = call(app, list(extra)?).await?;
        assert_eq!(StatusCode::OK, status, "{extra}: {text}");
        assert!(!text.contains("doc-img-1"), "{extra}: {text}");
        assert!(
            members.proxy.journal().is_empty(),
            "{extra}: nothing is sent"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_list_of_every_type_names_the_summary_and_the_members_own_imaging_report() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
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
        codes.contains(&"60591-5") && codes.contains(&"85430-7"),
        "{codes:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_silent_imaging_member_fails_the_list_and_is_named() -> TestResult {
    let members = Members::start(
        IMAGING,
        ResponseTemplate::new(500),
        answered(documents(&IMAGING, DOCUMENT).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(IMAGING_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        text.contains("the list is incomplete") && text.contains("node-a-img (node-error)"),
        "the silent member is named: {text}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_an_unreachable_imaging_member_is_named_in_an_outcome_entry() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    members.proxy.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let mut request = list(IMAGING_TYPE)?;
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
        diagnostics.contains("node-a-img (offline)"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn an_attachment_outside_the_members_endpoint_is_named_as_unpassable_imaging_reports()
-> TestResult {
    let elsewhere = DOCUMENT.replace(
        "\"Binary/img-1\"",
        "\"https://elsewhere.example.org/fhir/Binary/img-1\"",
    );
    let members = Members::start(
        IMAGING,
        answered(PATIENTS.as_bytes()),
        answered(documents(&IMAGING, &elsewhere).as_bytes()),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("best_effort = true\n", ""))?;
    let (status, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-img (node-error)"), "{text}");
    let mut request = list(IMAGING_TYPE)?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    assert!(
        text.contains(
            "node-a-img (node-error) answered imaging reports the list cannot pass through"
        ),
        "{text}"
    );
    assert!(!text.contains("elsewhere.example.org"), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_imaging_report_the_member_holds_no_longer_is_not_found() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    let url = url_of(&listed(&IMAGING, &text)?.0)?;
    members.own.reset().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Binary/img-1"))
        .respond_with(ResponseTemplate::new(410))
        .mount(&members.own)
        .await;
    let (status, text) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(text.contains("holds this document no longer"), "{text}");
    Ok(())
}

/// A synthetic medical image in PNG: the signature and an empty `IEND`
/// chunk.
const ORIGINAL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00IEND\xaeB`\x82";

// NOTE: MyHealth@EU Requirements Catalogue 10.0.0 08 and 08.01: an Original Clinical Document is
// unmodified after its creation, and a medical image is one in JPG or PNG.
#[tokio::test]
async fn a_members_original_image_in_png_is_read_byte_for_byte_with_its_media_type() -> TestResult {
    let document = DOCUMENT
        .replace("application/fhir+json", "image/png")
        .replace("85430-7", "18748-4");
    let members = Members::holding(IMAGING, &document).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let url = url_of(&listed(&IMAGING, &text)?.0)?;
    members.own.reset().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Binary/img-1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ORIGINAL_PNG, "image/png"))
        .mount(&members.own)
        .await;
    let (status, headers, body) = exchange(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(body, ORIGINAL_PNG, "the original byte for byte");
    assert_eq!(field(&headers, "content-type"), Some("image/png"));
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn the_imaging_list_and_read_are_recorded_under_the_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let (_, text) = call(app.clone(), list(IMAGING_TYPE)?).await?;
    let url = url_of(&listed(&IMAGING, &text)?.0)?;
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
        for named in ["node-a-img", EHR_A, "Medical-Imaging"] {
            assert!(record.contains(named), "{named}: {record}");
        }
        assert!(!record.contains("doc-img-1"), "no document in a record");
    }
    Ok(())
}
