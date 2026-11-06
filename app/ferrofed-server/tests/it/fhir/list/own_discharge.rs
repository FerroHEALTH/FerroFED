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

use std::collections::BTreeMap;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use eehrxf::document::discharge::HDR_BUNDLE;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use ferrofed_testkit::proxy::{Bind, CapturingProxy, Fault};
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use super::super::{TestResult, UID_B, node_holding, supplier, text_registered};
use super::{EHR_SYSTEM, binary, entries, list};
use crate::facade::{EHR_A, EHR_B, PATIENT, PATIENT_TAIL, received, settings_with_room, wire};
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

/// The member's own logical id of its patient.
const MEMBER_PATIENT: &str = "pat-a";

/// The member's `Patient` search, naming [`MEMBER_PATIENT`].
const PATIENTS: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"fullUrl":"Patient/pat-a","resource":{"resourceType":"Patient","id":"pat-a"},"search":{"mode":"match"}}]}"#;

/// The member's own `DocumentReference`, its members in the order the
/// gateway's serialiser writes them, with a decimal of two places, so the
/// listed bytes can be compared whole.
const DOCUMENT: &str = r#"{"category":[{"coding":[{"code":"Discharge-Reports","system":"http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"}]}],"content":[{"attachment":{"contentType":"application/fhir+json","size":63,"url":"Binary/hdr-1"}}],"extension":[{"url":"urn:oid:2.999.9.2","valueDecimal":2.50}],"id":"doc-hdr-1","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:4d200000-0000-4000-8000-0000000000b1"},"resourceType":"DocumentReference","status":"current","subject":{"reference":"Patient/pat-a"},"type":{"coding":[{"code":"18842-5","system":"http://loinc.org"}]}}"#;

/// The member's `Binary`, whose whitespace no serialiser would keep.
const BINARY: &[u8] =
    b"{ \"resourceType\" :\"Binary\", \"contentType\": \"application/fhir+json\" }\n";

/// The member's `DocumentReference` search, holding `document`.
fn documents(document: &str) -> String {
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","entry":[{{"fullUrl":"DocumentReference/doc-hdr-1","resource":{document},"search":{{"mode":"match"}}}}]}}"#
    )
}

/// A FHIR JSON answer of `body`.
fn answered(body: &[u8]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_vec(), "application/fhir+json")
}

/// The native endpoint of node A at `base`, serving discharge reports.
fn own_endpoint(base: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"node-a-hdr\"\nnode = \"node-a\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [\"Discharge-Reports\"]\nprofiles = [\"{HDR_BUNDLE}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// Node A serving its discharge reports itself, behind a capturing proxy,
/// beside its Query API, and node B, which serves none.
struct Members {
    a: Server,
    b: Server,
    own: Server,
    proxy: CapturingProxy,
    pdq: Server,
}

impl Members {
    /// The members, node A's own endpoint answering the `Patient` search
    /// with `patients`, every `DocumentReference` search of its patient with
    /// `documents` and the read of `Binary/hdr-1` with [`BINARY`].
    async fn start(
        patients: ResponseTemplate,
        documents: ResponseTemplate,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let own = Server::start().await;
        Mock::given(method("GET"))
            .and(path("/fhir/Patient"))
            .and(query_param("identifier", format!("{EHR_SYSTEM}|{EHR_A}")))
            .respond_with(patients)
            .mount(&own)
            .await;
        Mock::given(method("GET"))
            .and(path("/fhir/DocumentReference"))
            .and(query_param("patient", MEMBER_PATIENT))
            .respond_with(documents)
            .mount(&own)
            .await;
        Mock::given(method("GET"))
            .and(path("/fhir/Binary/hdr-1"))
            .respond_with(answered(BINARY))
            .mount(&own)
            .await;
        let proxy = CapturingProxy::start(own.uri(), Bind::Loopback).await?;
        Ok(Self {
            a: Server::start().await,
            b: node_holding(UID_B).await,
            own,
            proxy,
            pdq: supplier().await,
        })
    }

    /// The members, node A holding the one discharge report [`DOCUMENT`].
    async fn holding() -> Result<Self, Box<dyn std::error::Error>> {
        Self::start(
            answered(PATIENTS.as_bytes()),
            answered(documents(DOCUMENT).as_bytes()),
        )
        .await
    }

    /// The gateway over the members, with the `[federation]` keys
    /// `federation` and the tables `tables`.
    fn gateway(
        &self,
        dir: &std::path::Path,
        (federation, tables): (&str, &str),
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
        let text = text_registered(
            dir,
            (
                &self.a.uri(),
                &self.b.uri(),
                &pdqm_stub::base_url(&self.pdq),
            ),
            &rows,
            &own_endpoint(self.proxy.origin()),
            (federation, tables),
        )?;
        let settings =
            Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
                .resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        Ok(ferrofed_server::router(state, &settings_with_room()))
    }

    /// The path and the query pairs of every request node A's own endpoint
    /// received, in order.
    fn asked(&self) -> Vec<(String, Vec<(String, String)>)> {
        self.proxy
            .journal()
            .iter()
            .map(|request| {
                let query = request.query.as_deref().unwrap_or_default();
                let pairs = url::form_urlencoded::parse(query.as_bytes())
                    .map(|(name, value)| (name.into_owned(), value.into_owned()))
                    .collect();
                (request.path.clone(), pairs)
            })
            .collect()
    }
}

/// One entry of a `searchset`, its resource as the gateway wrote it.
#[derive(Deserialize)]
struct RawEntry {
    /// The resource's text.
    resource: Box<RawValue>,
}

/// A `searchset`, each resource as the gateway wrote it.
#[derive(Deserialize)]
struct RawSearchset {
    /// The entries.
    #[serde(default)]
    entry: Vec<RawEntry>,
}

/// The one discharge report `text` lists, with its text as the gateway
/// wrote it.
fn listed(text: &str) -> Result<(Value, String), Box<dyn std::error::Error>> {
    let bundle: Value = serde_json::from_str(text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one discharge report listed: {bundle}").into());
    };
    let raw: RawSearchset = serde_json::from_str(text)?;
    let written = raw
        .entry
        .iter()
        .map(|entry| entry.resource.get())
        .find(|resource| resource.contains("\"doc-hdr-1\""))
        .ok_or("the member's document")?
        .to_owned();
    Ok(((*reference).clone(), written))
}

/// The attachment URL of the listed `reference`.
fn url_of(reference: &Value) -> Result<String, Box<dyn std::error::Error>> {
    Ok(reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?
        .to_owned())
}

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
    let members = Members::holding().await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (reference, written) = listed(&text)?;
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
        let members = Members::holding().await?;
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
    let members = Members::holding().await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&text)?.0)?;
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
        let members = Members::holding().await?;
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
    let members = Members::holding().await?;
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
        ResponseTemplate::new(500),
        answered(documents(DOCUMENT).as_bytes()),
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
    let members = Members::holding().await?;
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
        answered(PATIENTS.as_bytes()),
        answered(documents(&elsewhere).as_bytes()),
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
    let members = Members::holding().await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&text)?.0)?;
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

    let members = Members::holding().await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(dir.path(), ("", &tables))?;
    let (_, text) = call(app.clone(), list(DISCHARGE_TYPE)?).await?;
    let url = url_of(&listed(&text)?.0)?;
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
