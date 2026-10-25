// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member that serves the patient summary itself, at its own
//! `hl7-fhir-rest` endpoint (the registry's `native` mode): it is asked
//! there by its own `ehr_id` alone, through a capturing proxy that shows no
//! patient identifier reaches it in the path, the query or a header
//! (§5.4.1, N33); it is left out of the section queries and named in every
//! section of the gateway's document; its silence makes the summary
//! incomplete (§11.3, §11.4); its endpoint is absent from `OPTIONS {base}/`
//! (§7a.2) and a directive or a targeting header naming it is `400` (§8,
//! N35); and configuration that cannot locate it refuses the start.

use std::collections::BTreeMap;

use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::{Request, StatusCode, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    TestResult, UID_B, gateway_registered, node_holding, section, summary, supplier,
    text_registered,
};
use crate::facade::{EHR_A, EHR_B, PATIENT, PATIENT_TAIL, body, post, wire};
use crate::support::call;

/// The LOINC code of the allergies section.
const ALLERGIES_SECTION: &str = "48765-2";

/// The identifier system node A's `ehr_id` is written in, in the example
/// arc.
const EHR_SYSTEM: &str = "urn:oid:2.999.4.1";

/// The member's own document, whose whitespace no serialiser would keep.
const DOCUMENT: &[u8] = b"{ \"resourceType\" :\"Bundle\",\n \"identifier\": {\"system\": \"urn:oid:2.999.4.2\", \"value\": \"synthetic-doc-a\"},\n \"type\": \"document\" , \"entry\":[ ] }\n";

/// The native endpoint of node A at `base`, serving patient summaries in
/// the profiles `profiles`.
fn native_endpoint(base: &str, profiles: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"node-a-eps\"\nnode = \"node-a\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [\"Patient-Summaries\"]\nprofiles = [{profiles}]\nehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// Node A's own endpoint, answering every summary request with `response`.
async fn own_endpoint(response: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$summary"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

/// The members: node A's Query API endpoint, which must stay unasked, node A's
/// own endpoint behind a capturing proxy, and node B holding one allergy.
struct Members {
    a: Server,
    own: Server,
    proxy: CapturingProxy,
    b: Server,
    pdq: Server,
}

impl Members {
    /// The members, node A's own endpoint answering with `response`.
    async fn start(response: ResponseTemplate) -> Result<Self, Box<dyn std::error::Error>> {
        let own = own_endpoint(response).await;
        let proxy = CapturingProxy::start(own.uri()).await?;
        Ok(Self {
            a: Server::start().await,
            own,
            proxy,
            b: node_holding(UID_B).await,
            pdq: supplier().await,
        })
    }

    /// The gateway over the members, with the `[federation]` keys
    /// `federation`.
    fn gateway(
        &self,
        dir: &std::path::Path,
        federation: &str,
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
        let members = native_endpoint(self.proxy.origin(), &format!("\"{EPS_COMPOSITION}\""));
        let (app, _) = gateway_registered(
            dir,
            (
                &self.a.uri(),
                &self.b.uri(),
                &pdqm_stub::base_url(&self.pdq),
            ),
            &rows,
            &members,
            (federation, ""),
        )?;
        Ok(app)
    }
}

/// The number of requests `server` received.
async fn asked(server: &Server) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_native_member_is_asked_by_its_ehr_id_alone_and_left_out_of_the_mapped_document()
-> TestResult {
    let members = Members::start(
        ResponseTemplate::new(200).set_body_raw(DOCUMENT.to_vec(), "application/fhir+json"),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    ferrofed_testkit::emit::document("ferrofed-server-summary-beside-native-member", &text)?;
    let document: Value = serde_json::from_str(&text)?;
    let allergies = section(&document, ALLERGIES_SECTION).ok_or("the allergies section")?;
    assert_eq!(
        allergies["entry"].as_array().map(Vec::len),
        Some(1),
        "node B's allergy alone is mapped: {allergies}"
    );
    let div = allergies
        .pointer("/text/div")
        .and_then(Value::as_str)
        .ok_or("a narrative")?;
    assert!(
        div.contains(
            "Not mapped here: the records of org-a, which serves its own patient summary."
        ),
        "the member is named as covered elsewhere: {div}"
    );
    assert_eq!(
        0,
        asked(&members.a).await?,
        "no section query reaches node A"
    );
    let journal = members.proxy.journal();
    let [request] = journal.as_slice() else {
        return Err(format!(
            "one request at node A's own endpoint, got {}",
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
            ("identifier".to_owned(), format!("{EHR_SYSTEM}|{EHR_A}")),
            ("profile".to_owned(), EPS_COMPOSITION.to_owned()),
        ],
        "N33: node A's ehr_id alone"
    );
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !members.proxy.journal_contains(withheld.as_bytes()),
            "N33: no identifier in the path, the query or a header"
        );
        let captured = wire(&members.own).await?;
        assert!(
            !captured.contains(withheld),
            "N33: none in the caller claims either: {captured}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_native_member_that_fails_makes_the_summary_incomplete_and_is_named() -> TestResult {
    let members = Members::start(ResponseTemplate::new(500)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("node-a-eps (node-error)"),
        "the silent member is named: {diagnostics}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_unreachable_native_member_is_a_gateway_time_out() -> TestResult {
    let members = Members::start(ResponseTemplate::new(200)).await?;
    members.proxy.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "§11.4: {text}");
    assert!(text.contains("node-a-eps (offline)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_silent_native_member_is_named_in_every_section() -> TestResult {
    let members = Members::start(ResponseTemplate::new(500)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "best_effort = true\n")?;
    let mut request = summary("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let document: Value = serde_json::from_str(&text)?;
    let allergies = section(&document, ALLERGIES_SECTION).ok_or("the allergies section")?;
    let div = allergies
        .pointer("/text/div")
        .and_then(Value::as_str)
        .ok_or("a narrative")?;
    assert!(div.contains("No answer from: org-a (node-error)"), "{div}");
    assert!(!div.contains("Not mapped here"), "{div}");
    Ok(())
}

/// A member's own document with no `Bundle.identifier` (FHIR R4 `bdl-9`).
const UNIDENTIFIED: &[u8] = b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"entry\":[]}";

#[tokio::test]
async fn a_native_member_answering_a_document_with_no_identifier_fails_the_summary() -> TestResult {
    let members = Members::start(
        ResponseTemplate::new(200).set_body_raw(UNIDENTIFIED.to_vec(), "application/fhir+json"),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("node-a-eps (node-error)"),
        "the member is named in node-error: {diagnostics}"
    );
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_native_member_answering_a_document_with_no_identifier_is_not_covered()
-> TestResult {
    let members = Members::start(
        ResponseTemplate::new(200).set_body_raw(UNIDENTIFIED.to_vec(), "application/fhir+json"),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "best_effort = true\n")?;
    let mut request = summary("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let document: Value = serde_json::from_str(&text)?;
    let allergies = section(&document, ALLERGIES_SECTION).ok_or("the allergies section")?;
    let div = allergies
        .pointer("/text/div")
        .and_then(Value::as_str)
        .ok_or("a narrative")?;
    assert!(div.contains("No answer from: org-a (node-error)"), "{div}");
    assert!(
        !div.contains("Not mapped here"),
        "never marked as covered by its own document: {div}"
    );
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn the_native_endpoint_is_absent_from_the_self_description() -> TestResult {
    let members = Members::start(ResponseTemplate::new(200)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let (status, text) = call(app, Request::options("/").body(axum::body::Body::empty())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let described: Value = serde_json::from_str(&text)?;
    let ids: Vec<&str> = described["endpoints"]
        .as_array()
        .ok_or("endpoints")?
        .iter()
        .filter_map(|endpoint| endpoint["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        ["node-a-pub", "node-b-pub"],
        "§7a.2: the identifiers usable in a directive"
    );
    Ok(())
}

// conformance: CP-6
#[tokio::test]
async fn a_directive_naming_the_native_endpoint_is_refused_400() -> TestResult {
    let members = Members::start(ResponseTemplate::new(200)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let query = format!(
        "SELECT c/uid/value FROM ENDPOINT p [\"node-a-eps\"] CONTAINS EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}'"
    );
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8: {text}");
    assert!(text.contains("endpoint-unknown"), "{text}");
    assert!(members.proxy.journal().is_empty(), "nothing reaches it");
    Ok(())
}

// conformance: CP-6
#[tokio::test]
async fn a_targeting_header_naming_the_native_endpoint_is_refused_400() -> TestResult {
    let members = Members::start(ResponseTemplate::new(200)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), "")?;
    let query = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}'"
    );
    let mut request = post(body(&query)?)?;
    request
        .headers_mut()
        .insert("openEHR-federation-endpoint", "node-a-eps".parse()?);
    request
        .headers_mut()
        .insert(header::ACCEPT, "application/json".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "§8.4, N35: {text}");
    assert!(text.contains("endpoint-unknown"), "{text}");
    assert!(members.proxy.journal().is_empty(), "nothing reaches it");
    Ok(())
}

/// What loading the federation whose registry declares `members` beside
/// node A and node B gives.
fn loaded(
    members: &str,
) -> Result<Result<Option<Federation>, FederationError>, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (
            "http://a.example.org",
            "http://b.example.org",
            super::UNREACHED_SUPPLIER,
        ),
        &[("node-a", EHR_A)],
        members,
        ("", ""),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings))
}

#[test]
fn a_native_endpoint_with_no_identifier_system_refuses_the_start() -> TestResult {
    let members = native_endpoint("http://a.example.org", &format!("\"{EPS_COMPOSITION}\""))
        .replace(&format!("ehr_id_system = \"{EHR_SYSTEM}\"\n"), "");
    let refused = loaded(&members)?;
    let Err(FederationError::Registry { source, .. }) = &refused else {
        return Err(format!("the registry must refuse to load: {refused:?}").into());
    };
    assert!(
        matches!(
            **source,
            ferrofed_registry::error::LoadError::NativeWithoutIdentifierSystem(_)
        ),
        "N33: {source:?}"
    );
    Ok(())
}

#[test]
fn a_native_summary_endpoint_without_the_eps_profile_refuses_the_start() -> TestResult {
    let members = native_endpoint(
        "http://a.example.org",
        "\"http://example.org/StructureDefinition/other\"",
    );
    let refused = loaded(&members)?;
    assert!(
        matches!(
            refused,
            Err(FederationError::NativeProfileMissing { ref category, .. })
                if category == "Patient-Summaries"
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_native_endpoint_naming_no_priority_category_refuses_the_start() -> TestResult {
    let members = native_endpoint("http://a.example.org", &format!("\"{EPS_COMPOSITION}\""))
        .replace("\"Patient-Summaries\"", "\"Patient-Summary\"");
    let refused = loaded(&members)?;
    assert!(
        matches!(
            refused,
            Err(FederationError::NativeCategoryUnknown { ref category, .. })
                if category == "Patient-Summary"
        ),
        "{refused:?}"
    );
    Ok(())
}
