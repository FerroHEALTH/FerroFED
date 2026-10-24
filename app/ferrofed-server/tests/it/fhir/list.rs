// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document list on the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1): `GET {fhir-base}/DocumentReference?patient.identifier=system|value`
//! answers a `searchset` naming the summary the gateway writes on request
//! and each `native` member's own document, carried inline and unchanged,
//! each entry held to the HL7 Europe EU Health Data API
//! `EehrxfMhdDocumentReference` profile; no section query is run; no patient
//! identifier reaches a member (§5.4.1, N33); a silent member fails the list
//! under all-or-nothing and is named in an `outcome` entry under `partial`
//! (§11.3, §11.4); the search is authenticated and recorded (Annex II 3.2);
//! and `metadata` declares it.

use std::collections::BTreeMap;
use std::sync::Arc;

use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use axum::body::Body;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::health_data_api;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{FHIR, TestResult, UID_A, UID_B, gateway, node_holding, supplier, text_registered};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire,
};
use crate::support::{call, exchange, field, send_as_is};

/// The identifier system the members' `ehr_id`s are written in, in the
/// example arc.
pub(super) const EHR_SYSTEM: &str = "urn:oid:2.999.4.1";

/// Node A's own document, whose whitespace no serialiser would keep.
pub(super) const DOCUMENT_A: &[u8] = b"{ \"resourceType\" :\"Bundle\",\n \"identifier\": {\"system\": \"urn:ietf:rfc:3986\", \"value\": \"urn:uuid:5a1e0000-0000-4000-8000-00000000000a\"},\n \"type\": \"document\" , \"entry\":[ ] }\n";

/// Node B's own document.
pub(super) const DOCUMENT_B: &[u8] = b"{\"resourceType\":\"Bundle\",\"identifier\":{\"system\":\"urn:oid:2.999.4.2\",\"value\":\"synthetic-doc-b\"},\"type\":\"document\",\"entry\":[]}";

/// The LOINC token of the patient summary document type, percent-encoded.
const SUMMARY_TYPE: &str = "http%3A%2F%2Floinc.org%7C60591-5";

/// The native endpoint `id` of `node`, of the organisation `organisation`,
/// at `base`.
pub(super) fn native_endpoint(id: &str, (node, organisation): (&str, &str), base: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"{id}\"\nnode = \"{node}\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"{organisation}\"\n\
         categories = [\"Patient-Summaries\"]\nprofiles = [\"{EPS_COMPOSITION}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// A member's own endpoint, answering every summary request with
/// `response`.
async fn own_endpoint(response: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$summary"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

/// Both members serving the patient summary themselves: each Query API
/// endpoint, which must stay unasked, and each own endpoint behind a
/// capturing proxy.
pub(super) struct Members {
    a: Server,
    b: Server,
    pub(super) own_a: Server,
    pub(super) own_b: Server,
    pub(super) proxy_a: CapturingProxy,
    pub(super) proxy_b: CapturingProxy,
    pdq: PdqSupplier,
}

impl Members {
    /// The members, node A's own endpoint answering with `a` and node B's
    /// with `b`.
    pub(super) async fn start(
        a: ResponseTemplate,
        b: ResponseTemplate,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (own_a, own_b) = (own_endpoint(a).await, own_endpoint(b).await);
        let proxy_a = CapturingProxy::start(own_a.uri()).await?;
        let proxy_b = CapturingProxy::start(own_b.uri()).await?;
        Ok(Self {
            a: node_holding(UID_A).await,
            b: node_holding(UID_B).await,
            own_a,
            own_b,
            proxy_a,
            proxy_b,
            pdq: supplier().await?,
        })
    }

    /// The gateway over the members, with the `[federation]` keys
    /// `federation` and the tables `tables`.
    pub(super) fn gateway(
        &self,
        dir: &std::path::Path,
        tables: (&str, &str),
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        self.gateway_keyed(dir, tables, "")
    }

    /// The gateway of [`Members::gateway`], with the keys `keys` added to
    /// `[fhir]`.
    pub(super) fn gateway_keyed(
        &self,
        dir: &std::path::Path,
        (federation, tables): (&str, &str),
        keys: &str,
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
        let members = format!(
            "{}{}",
            native_endpoint("node-a-eps", ("node-a", "org-a"), self.proxy_a.origin()),
            native_endpoint("node-b-eps", ("node-b", "org-b"), self.proxy_b.origin())
        );
        let table = format!("[fhir]\nbase = \"{FHIR}\"\n");
        let text = text_registered(
            dir,
            (&self.a.uri(), &self.b.uri(), &self.pdq.base_url()),
            &rows,
            &members,
            (federation, tables),
        )?
        .replacen(&table, &format!("{table}{keys}\n"), 1);
        let settings =
            Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
                .resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        Ok(ferrofed_server::router(state, &settings_with_room()))
    }

    /// Whether no section query reached either member's Query API.
    async fn no_section_query(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(received(&self.a).await?.is_empty() && received(&self.b).await?.is_empty())
    }
}

/// A served document, as `document` names it.
pub(super) fn fhir_document(document: &[u8]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(document.to_vec(), "application/fhir+json")
}

/// `GET {fhir-base}/DocumentReference` for the test patient, with the query
/// `extra` added.
pub(super) fn list(extra: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "{FHIR}/DocumentReference?patient.identifier={}%7C{PATIENT}{extra}",
        NAMESPACE.replace(':', "%3A")
    ))
    .body(Body::empty())
}

/// The resources of the entries of `bundle` whose search mode is `mode`.
pub(super) fn entries<'a>(bundle: &'a Value, mode: &str) -> Vec<&'a Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry.pointer("/search/mode") == Some(&Value::from(mode)))
        .map(|entry| &entry["resource"])
        .collect()
}

/// The entry of `matches` whose custodian is shown as `custodian`.
pub(super) fn by_custodian<'a>(matches: &[&'a Value], custodian: &str) -> Option<&'a Value> {
    matches
        .iter()
        .find(|reference| reference.pointer("/custodian/display") == Some(&Value::from(custodian)))
        .copied()
}

/// Holds the entry `reference` to name `document` at a URL of the gateway's,
/// with its hash, size and media type, and returns that URL.
pub(super) fn names(
    reference: &Value,
    document: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    let attachment = reference
        .pointer("/content/0/attachment")
        .ok_or("an attachment")?;
    assert!(
        attachment.get("data").is_none(),
        "no inline data: {attachment}"
    );
    let url = attachment["url"].as_str().ok_or("a URL")?;
    assert!(url.starts_with("Binary/"), "a URL of the gateway's: {url}");
    let hash = STANDARD.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, document).as_ref());
    assert_eq!(
        attachment["hash"],
        Value::from(hash),
        "R4 Attachment.hash, SHA-1"
    );
    assert_eq!(
        attachment["size"],
        Value::from(document.len()),
        "the number of bytes"
    );
    assert_eq!(
        attachment["contentType"], "application/fhir+json",
        "as the member answered"
    );
    Ok(url.to_owned())
}

/// `GET {fhir-base}/{url}`, with the default credential.
pub(super) fn binary(url: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/{url}")).body(Body::empty())
}

/// Holds `app` to answer `url` with `document`, byte for byte, as the
/// member typed it.
pub(super) async fn reads(app: axum::Router, url: &str, document: &[u8]) -> TestResult {
    let (status, headers, body) = exchange(app, binary(url)?).await?;
    assert_eq!(StatusCode::OK, status, "{}", String::from_utf8_lossy(&body));
    assert_eq!(body, document, "the member's exact bytes");
    assert_eq!(
        field(&headers, "content-type"),
        Some("application/fhir+json"),
        "as the member typed them"
    );
    Ok(())
}

/// The findings of `reference` against `EehrxfMhdDocumentReference`.
pub(super) fn findings(reference: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    health_data_api::check(&reference.to_string())
}

#[tokio::test]
async fn the_list_names_the_summary_written_on_request_and_each_members_own_document() -> TestResult
{
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(&format!("&type={SUMMARY_TYPE}"))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(bundle["resourceType"], "Bundle");
    assert_eq!(bundle["type"], "searchset");
    let matches = entries(&bundle, "match");
    assert_eq!(3, matches.len(), "{bundle}");
    assert_eq!(bundle["total"], Value::from(3));
    assert!(entries(&bundle, "outcome").is_empty(), "{bundle}");
    let written = by_custodian(&matches, "Synthetic Operator").ok_or("the summary written")?;
    let findings_written = findings(written)?;
    assert!(findings_written.is_empty(), "{findings_written:#?}");
    let attachment = written
        .pointer("/content/0/attachment")
        .ok_or("an attachment")?;
    assert_eq!(
        attachment["url"],
        Value::from(format!(
            "Patient/$summary?identifier={}%7C{PATIENT}",
            NAMESPACE.replace(':', "%3A")
        )),
        "retrievable through Patient/$summary, relative to the FHIR base"
    );
    assert_eq!(
        written.pointer("/content/0/format/code"),
        Some(&Value::from(
            "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps"
        ))
    );
    for (custodian, document, identifier) in [
        (
            "org-a",
            DOCUMENT_A,
            "urn:uuid:5a1e0000-0000-4000-8000-00000000000a",
        ),
        ("org-b", DOCUMENT_B, "synthetic-doc-b"),
    ] {
        let own = by_custodian(&matches, custodian).ok_or("a member's own document")?;
        let url = names(own, document)?;
        reads(app.clone(), &url, document).await?;
        assert_eq!(
            own.pointer("/masterIdentifier/value"),
            Some(&Value::from(identifier)),
            "the document's own identifier"
        );
        assert_eq!(
            own.pointer("/subject/identifier/value"),
            Some(&Value::from(PATIENT)),
            "the caller's own identifier, returned to the caller"
        );
        let findings_own = findings(own)?;
        assert!(findings_own.is_empty(), "{findings_own:#?}");
    }
    assert!(
        members.no_section_query().await?,
        "the list runs no section query"
    );
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_native_member_is_asked_for_the_list_by_its_ehr_id_alone() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (proxy, own, ehr_id) in [
        (&members.proxy_a, &members.own_a, EHR_A),
        (&members.proxy_b, &members.own_b, EHR_B),
    ] {
        let journal = proxy.journal();
        let [request] = journal.as_slice() else {
            return Err(format!("one request at the own endpoint, got {}", journal.len()).into());
        };
        assert_eq!(request.path, "/fhir/Patient/$summary");
        let query = request.query.as_deref().unwrap_or_default();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("identifier".to_owned(), format!("{EHR_SYSTEM}|{ehr_id}")),
                ("profile".to_owned(), EPS_COMPOSITION.to_owned()),
            ],
            "N33: the member's ehr_id alone"
        );
        for withheld in [PATIENT, PATIENT_TAIL] {
            assert!(
                !proxy.journal_contains(withheld.as_bytes()),
                "N33: no identifier in the path, the query or a header"
            );
            let captured = wire(own).await?;
            assert!(
                !captured.contains(withheld),
                "N33: none in the caller claims either: {captured}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn with_no_native_member_the_list_holds_the_summary_written_on_request_alone() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    assert_eq!(1, matches.len(), "{bundle}");
    let written = matches.first().ok_or("an entry")?;
    let findings_written = findings(written)?;
    assert!(findings_written.is_empty(), "{findings_written:#?}");
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "the list runs no section query"
    );
    Ok(())
}

#[tokio::test]
async fn a_silent_native_member_fails_the_list_and_is_named() -> TestResult {
    let members = Members::start(ResponseTemplate::new(500), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("the list is incomplete")
            && diagnostics.contains("node-a-eps (node-error)"),
        "the silent member is named: {diagnostics}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_unreachable_native_member_fails_the_list_with_a_gateway_time_out() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    members.proxy_b.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "§11.4: {text}");
    assert!(text.contains("node-b-eps (offline)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_silent_native_member_is_named_in_an_outcome_entry() -> TestResult {
    let members = Members::start(ResponseTemplate::new(500), fhir_document(DOCUMENT_B)).await?;
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
    let own = by_custodian(&matches, "org-b").ok_or("node B's document")?;
    names(own, DOCUMENT_B)?;
    assert!(by_custodian(&matches, "org-a").is_none(), "{bundle}");
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("incomplete"))
    );
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("node-a-eps (node-error)"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn a_search_with_no_credential_is_refused_and_nothing_is_sent() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let response = send_as_is(app, list("")?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "§13.1, N25");
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    assert!(members.no_section_query().await?);
    Ok(())
}

#[tokio::test]
async fn a_type_other_than_the_patient_summary_matches_nothing_and_nothing_is_sent() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list("&type=http%3A%2F%2Floinc.org%7C11502-2")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(bundle["type"], "searchset");
    assert!(bundle.get("entry").is_none(), "{bundle}");
    assert_eq!(bundle["total"], Value::from(0));
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_search_naming_no_patient_or_another_parameter_is_refused() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    for query in [
        format!("type={SUMMARY_TYPE}"),
        format!("patient.identifier={PATIENT}"),
        format!("patient.identifier=urn%3Aoid%3A2.999.1%7C{PATIENT}&status=current"),
    ] {
        let request =
            Request::get(format!("{FHIR}/DocumentReference?{query}")).body(Body::empty())?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        let outcome: Value = serde_json::from_str(&text)?;
        assert_eq!(outcome["resourceType"], "OperationOutcome", "{text}");
        assert!(!text.contains(PATIENT_TAIL), "{text}");
    }
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_patient_no_member_holds_matches_nothing_and_nothing_is_sent() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let request = Request::get(format!(
        "{FHIR}/DocumentReference?patient.identifier=urn%3Aoid%3A2.999.1%7CSYNTHETIC-OTHER-1"
    ))
    .body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert!(bundle.get("entry").is_none(), "{bundle}");
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    assert!(members.no_section_query().await?);
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_list_is_recorded_once_naming_every_member_asked() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(
        1,
        records.len(),
        "one access record per request: {records:#?}"
    );
    let record = records.first().ok_or("a record")?;
    assert!(
        record.contains("eehrxf-document-priority-category-cs|Patient-Summaries"),
        "Art 14(1)(a): the category of a summary: {record}"
    );
    for endpoint in ["node-a-eps", "node-b-eps", EHR_A, EHR_B] {
        assert!(record.contains(endpoint), "{endpoint}: {record}");
    }
    assert!(
        !record.contains("node-a-pub"),
        "no section query was asked: {record}"
    );
    Ok(())
}

/// Annex II 3.2, "every access event": a list that names the gateway's
/// summary tells the caller that members hold the patient's record, so it
/// is recorded once, naming the patient's `ehr_id` at each member it resolved
/// at, even where no member serves its own summary and none is asked.
#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_list_that_asked_no_member_is_recorded_once_naming_where_the_patient_resolved()
-> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let pdq = supplier().await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let (app, _) = super::gateway_over(
        dir.path(),
        (&a.uri(), &b.uri(), &pdq.base_url()),
        &rows,
        ("", &tables),
    )?;
    let (status, text) = call(app, list("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert!(
        bundle.get("entry").is_some(),
        "the gateway's summary is listed: {bundle}"
    );
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(1, records.len(), "one access record: {records:#?}");
    let record = records.first().ok_or("a record")?;
    for named in ["node-a-pub", "node-b-pub", EHR_A, EHR_B] {
        assert!(record.contains(named), "{named}: {record}");
    }
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no section query was asked"
    );
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_the_document_search() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(
        app,
        Request::get(format!("{FHIR}/metadata")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let statement: Value = serde_json::from_str(&text)?;
    let resources = statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .ok_or("resources")?;
    let documents = resources
        .iter()
        .find(|resource| resource["type"] == "DocumentReference")
        .ok_or("the DocumentReference resource")?;
    assert_eq!(
        documents.pointer("/interaction/0/code"),
        Some(&Value::from("search-type"))
    );
    assert_eq!(
        documents.pointer("/supportedProfile/0"),
        Some(&Value::from(health_data_api::PROFILE))
    );
    let names: Vec<&str> = documents["searchParam"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|parameter| parameter["name"].as_str())
        .collect();
    assert_eq!(names, ["patient.identifier", "type"]);
    let binary = resources
        .iter()
        .find(|resource| resource["type"] == "Binary")
        .ok_or("the Binary resource")?;
    assert_eq!(
        binary.pointer("/interaction/0/code"),
        Some(&Value::from("read")),
        "the read of a listed document"
    );
    Ok(())
}

/// The list for a patient the PIX Manager resolves at node A, while the
/// Manager node B is resolved through answers `500`, with the
/// `[federation]` keys `federation` and the completeness header
/// `completeness` when given: its status and its text.
async fn listed_with_a_failed_resolution(
    federation: &str,
    completeness: Option<&str>,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    use ferrofed_testkit::pix::PixManager;
    use ferrofed_testkit::seed::{self, CrossReferenceSeed, EhrDomain, PatientId};
    use wiremock::matchers::any;

    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let pix = PixManager::start().await?;
    let (domain, patient) = (EhrDomain::new(1), PatientId::new(1, 46));
    let feed = CrossReferenceSeed {
        patient,
        ehrs: vec![(
            domain,
            uuid::Uuid::from_u128(0x3a3a_3a3a_3a3a_4a3a_8a3a_3a3a_3a3a_3a3a),
        )],
    };
    assert_eq!(
        StatusCode::CREATED,
        seed::feed(&pix.base_url(), &feed).await?,
        "ITI-104 creates the Patient"
    );
    let down = Server::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&down)
        .await;
    let resolvers = format!(
        "\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\n\
         [[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\"node-b\" = \"urn:oid:2.999.32\"\n",
        pix.origin(),
        domain.system(),
        down.uri()
    );
    let own = format!(
        "{}{}",
        native_endpoint("node-a-eps", ("node-a", "org-a"), members.proxy_a.origin()),
        native_endpoint("node-b-eps", ("node-b", "org-b"), members.proxy_b.origin())
    );
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (&members.a.uri(), &members.b.uri(), &members.pdq.base_url()),
        &[],
        &own,
        (federation, &resolvers),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let mut request = Request::get(format!(
        "{FHIR}/DocumentReference?patient.identifier={}%7C{}",
        patient.namespace().replace(':', "%3A"),
        patient.value()
    ))
    .body(Body::empty())?;
    if let Some(completeness) = completeness {
        request
            .headers_mut()
            .insert("openehr-federation-completeness", completeness.parse()?);
    }
    call(app, request).await
}

#[tokio::test]
async fn a_resolution_the_cross_reference_could_not_answer_fails_the_list() -> TestResult {
    let (status, text) = listed_with_a_failed_resolution("", None).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "all-or-nothing, as the summary and the prescription search: {text}"
    );
    assert!(text.contains("node-b-pub (not-resolved)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_member_whose_resolution_failed_is_named_in_an_outcome_entry() -> TestResult
{
    let (status, text) =
        listed_with_a_failed_resolution("best_effort = true\n", Some("partial")).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let own = by_custodian(&entries(&bundle, "match"), "org-a").ok_or("node A's document")?;
    names(own, DOCUMENT_A)?;
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    assert!(
        outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("node-b-pub (not-resolved)")),
        "{outcome}"
    );
    Ok(())
}
