// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member's own document read through the gateway at the URL the document
//! list names, `{fhir-base}/Binary/{token}`: the member asked again by its
//! own `ehr_id` alone (§5.4.1, N33), its bytes answered unchanged, the URL
//! taken only from the caller it was listed for while it is valid, and taken
//! for no other token the gateway signs, and the read recorded (Annex II
//! 3.2).

use std::time::Duration;

use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use http::{StatusCode, header};
use serde_json::Value;
use wiremock::ResponseTemplate;

use super::TestResult;
use super::list::{
    DOCUMENT_A, DOCUMENT_B, EHR_SYSTEM, Members, binary, by_custodian, entries, fhir_document,
    list, names, reads,
};
use crate::facade::{EHR_B, PATIENT, PATIENT_TAIL, wire};
use crate::support::{call, claims, exchange, issuer};

/// The URL the list names node B's document at, listed by the default
/// caller through `app`.
async fn listed_b(app: axum::Router) -> Result<String, Box<dyn std::error::Error>> {
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let own = by_custodian(&matches, "org-b").ok_or("node B's document")?;
    names(own, DOCUMENT_B)
}

/// Holds `app` to refuse `url` as a URL that names no document, with no
/// byte of a document.
async fn refuses(app: axum::Router, request: http::Request<axum::body::Body>) -> TestResult {
    let (status, headers, body) = exchange(app, request).await?;
    let text = String::from_utf8_lossy(&body);
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/fhir+json"),
        "an OperationOutcome"
    );
    assert!(!text.contains("synthetic-doc-b"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_listed_document_is_read_unchanged_and_its_hash_matches() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let url = listed_b(app.clone()).await?;
    let (status, _, body) = exchange(app.clone(), binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status);
    let hash = STANDARD.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, &body).as_ref());
    let expected = STANDARD.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, DOCUMENT_B).as_ref());
    assert_eq!(hash, expected, "R4 Attachment.hash of the bytes read");
    reads(app, &url, DOCUMENT_B).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_read_asks_the_member_by_its_ehr_id_alone() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let url = listed_b(app.clone()).await?;
    members.proxy_b.clear_journal();
    reads(app, &url, DOCUMENT_B).await?;
    let journal = members.proxy_b.journal();
    let [request] = journal.as_slice() else {
        return Err(format!(
            "one request at node B's own endpoint, got {}",
            journal.len()
        )
        .into());
    };
    assert_eq!(request.path, "/fhir/Patient/$summary");
    let query = request.query.as_deref().unwrap_or_default();
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("identifier".to_owned(), format!("{EHR_SYSTEM}|{EHR_B}")),
            ("profile".to_owned(), EPS_COMPOSITION.to_owned()),
        ],
        "N33: node B's ehr_id alone"
    );
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !members.proxy_b.journal_contains(withheld.as_bytes()),
            "N33: no identifier in the path, the query or a header"
        );
        let captured = wire(&members.own_b).await?;
        assert!(!captured.contains(withheld), "N33: {captured}");
    }
    assert!(
        !url.contains(PATIENT_TAIL),
        "no patient identifier in the URL"
    );
    Ok(())
}

#[tokio::test]
async fn an_expired_url_is_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway_keyed(dir.path(), ("", ""), "document_url_lifetime_secs = 1")?;
    let url = listed_b(app.clone()).await?;
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    members.proxy_b.clear_journal();
    refuses(app, binary(&url)?).await?;
    assert!(members.proxy_b.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_tampered_url_is_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let url = listed_b(app.clone()).await?;
    let token = url.strip_prefix("Binary/").ok_or("a Binary URL")?;
    let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
    let payload = parts.get(1).ok_or("a payload")?;
    let mut claims: Value =
        serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?)?;
    claims["endpoint"] = Value::from("node-a-eps");
    let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    if let Some(slot) = parts.get_mut(1) {
        *slot = forged;
    }
    members.proxy_a.clear_journal();
    members.proxy_b.clear_journal();
    refuses(app, binary(&format!("Binary/{}", parts.join(".")))?).await?;
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    Ok(())
}

#[tokio::test]
async fn another_callers_url_is_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let url = listed_b(app.clone()).await?;
    let mut other = claims();
    other.sub = String::from("synthetic-other-caller");
    let token = issuer().mint(&other)?;
    let mut request = binary(&url)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    members.proxy_b.clear_journal();
    refuses(app, request).await?;
    assert!(members.proxy_b.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_token_of_another_type_is_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    listed_b(app.clone()).await?;
    let requests = members
        .own_b
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let conveyed = requests
        .iter()
        .find_map(|request| request.headers.get("openehr-federation-client"))
        .and_then(|value| value.to_str().ok())
        .ok_or("the conveyance the list sent")?
        .to_owned();
    members.proxy_b.clear_journal();
    refuses(app, binary(&format!("Binary/{conveyed}"))?).await?;
    assert!(members.proxy_b.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn a_member_that_fails_the_read_is_named() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let url = listed_b(app.clone()).await?;
    members.own_b.reset().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&members.own_b)
        .await;
    let (status, text) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-b-eps (node-error)"), "{text}");
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_read_is_recorded_naming_the_member_and_its_ehr_id() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let url = listed_b(app.clone()).await?;
    reads(app, &url, DOCUMENT_B).await?;
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
    let read = records
        .iter()
        .find(|record| !record.contains("node-a-eps"))
        .ok_or("the read's record, which names node B alone")?;
    for named in ["node-b-eps", EHR_B, "Patient-Summaries"] {
        assert!(read.contains(named), "{named}: {read}");
    }
    assert!(!read.contains(PATIENT_TAIL), "{read}");
    Ok(())
}
