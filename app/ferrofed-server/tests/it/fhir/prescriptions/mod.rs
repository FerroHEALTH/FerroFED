// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The prescription search on the FHIR face (Regulation (EU) 2025/327 Art
//! 14(1)(b), Annex II 2.1): `GET {fhir-base}/MedicationRequest?patient.identifier=`
//! asks every member that serves prescriptions and holds the patient, at its
//! own endpoint and by its own `ehr_id` alone (§5.4.1, N33), and answers one
//! `searchset` with each member's prescription as the member sent it and one
//! `Provenance` naming that member; a member's pages are followed under its
//! own base, and the gateway pages its answer by `_count`; a silent member
//! fails the search under all-or-nothing and is named under `partial`
//! (§11.3, §11.4); an unknown patient and a restricted one answer alike; the
//! search takes the `composition-` search scope; and the identifier reaches
//! neither a member nor the log.

mod paging;
mod restricted;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::list::EHR_SYSTEM;
use super::{FHIR, TestResult, UNREACHED_SUPPLIER, text_registered};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire,
};
use crate::support::call;

/// The `[[fhir.serve]]` table declaring both members for prescriptions.
const SERVED: &str = "\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\"node-a\", \"node-b\"]\n";

/// Node A's prescription, whose whitespace and unknown extension no
/// serialiser would keep.
const RX_A: &str = r#"{ "resourceType" : "MedicationRequest", "id":"rx-a1", "extension":[{"url":"https://example.org/fhir/StructureDefinition/synthetic","valueString":"kept as sent"}],
  "identifier":[{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}], "status":"active","intent":"order",
  "subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A"} }"#;

/// Node A's second prescription, on its second page.
const RX_A2: &str = r#"{"resourceType":"MedicationRequest","id":"rx-a2","identifier":[{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A2"}],"status":"completed","intent":"order","subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A2"}}"#;

/// Node B's prescription.
const RX_B: &str = r#"{"resourceType":"MedicationRequest","id":"rx-b1","identifier":[{"system":"urn:oid:2.999.5.2","value":"SYNTHETIC-RX-B1"}],"status":"active","intent":"order","subject":{"reference":"Patient/p-b"},"medicationCodeableConcept":{"text":"Synthetic medicine B"}}"#;

/// Node B's prescription under the identifier node A's carries.
const RX_B_SAME: &str = r#"{"resourceType":"MedicationRequest","id":"rx-b9","identifier":[{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}],"status":"active","intent":"order","subject":{"reference":"Patient/p-b"},"medicationCodeableConcept":{"text":"Synthetic medicine A"}}"#;

/// A `searchset` of the prescriptions `resources`, each under a `fullUrl`
/// at `base`, with the `next` link `next` when given.
fn searchset(base: &str, resources: &[&str], next: Option<&str>) -> String {
    let entries: Vec<String> = resources
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            format!(
                r#"{{"fullUrl":"{base}/fhir/MedicationRequest/entry-{index}","resource":{resource},"search":{{"mode":"match"}}}}"#
            )
        })
        .collect();
    let link = next.map_or_else(String::new, |url| {
        format!(r#","link":[{{"relation":"next","url":"{url}"}}]"#)
    });
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","total":{}{link},"entry":[{}]}}"#,
        resources.len(),
        entries.join(",")
    )
}

/// A member's answer of the `searchset` `body`.
fn page(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/fhir+json")
}

/// The own endpoint `id` of `node`, at `base`, serving prescriptions.
fn own_endpoint(id: &str, (node, organisation): (&str, &str), base: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"{id}\"\nnode = \"{node}\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"{organisation}\"\n\
         categories = [\"Electronic-Prescriptions\"]\nprofiles = [\"{MPD_MEDICATION_REQUEST}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// A member's own endpoint answering the first page of a search with
/// `first`.
async fn own(first: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationRequest"))
        .respond_with(first)
        .mount(&server)
        .await;
    server
}

/// Both members: each Query API endpoint, which must stay unasked, and each
/// own endpoint behind a capturing proxy.
struct Members {
    a: Server,
    b: Server,
    own_a: Server,
    own_b: Server,
    proxy_a: CapturingProxy,
    proxy_b: CapturingProxy,
}

impl Members {
    /// The members, node A's own endpoint answering with `a` and node B's
    /// with `b`.
    async fn start(a: ResponseTemplate, b: ResponseTemplate) -> Result<Self, Box<dyn Error>> {
        let (own_a, own_b) = (own(a).await, own(b).await);
        Self::over(own_a, own_b).await
    }

    /// The members whose own endpoints are `own_a` and `own_b`.
    async fn over(own_a: Server, own_b: Server) -> Result<Self, Box<dyn Error>> {
        let proxy_a = CapturingProxy::start(own_a.uri()).await?;
        let proxy_b = CapturingProxy::start(own_b.uri()).await?;
        Ok(Self {
            a: Server::start().await,
            b: Server::start().await,
            own_a,
            own_b,
            proxy_a,
            proxy_b,
        })
    }

    /// The gateway over the members, the patient known at the members
    /// `rows` names, with the `[federation]` keys `federation` and the
    /// tables `tables` after the serving table.
    fn gateway(
        &self,
        dir: &Path,
        rows: &[(&str, &str)],
        (federation, tables): (&str, &str),
    ) -> Result<Router, Box<dyn Error>> {
        let members = format!(
            "{}{}",
            own_endpoint("node-a-mpd", ("node-a", "org-a"), self.proxy_a.origin()),
            own_endpoint("node-b-mpd", ("node-b", "org-b"), self.proxy_b.origin())
        );
        built(
            dir,
            (&self.a.uri(), &self.b.uri()),
            rows,
            &members,
            (federation, &format!("{SERVED}{tables}")),
        )
    }

    /// The gateway over the members, the patient known at both.
    fn both(&self, dir: &Path) -> Result<Router, Box<dyn Error>> {
        self.gateway(dir, &[("node-a", EHR_A), ("node-b", EHR_B)], ("", ""))
    }

    /// Whether neither member's own endpoint was asked.
    fn unasked(&self) -> bool {
        self.proxy_a.journal().is_empty() && self.proxy_b.journal().is_empty()
    }

    /// Whether nothing reached either member's Query API.
    async fn no_query(&self) -> Result<bool, Box<dyn Error>> {
        Ok(received(&self.a).await?.is_empty() && received(&self.b).await?.is_empty())
    }
}

/// The gateway over node A at `a` and node B at `b`, with `members` added
/// to the registry, the patient known at the members `rows` names, and the
/// `[federation]` keys and the tables given.
fn built(
    dir: &Path,
    (a, b): (&str, &str),
    rows: &[(&str, &str)],
    members: &str,
    (federation, tables): (&str, &str),
) -> Result<Router, Box<dyn Error>> {
    let text = text_registered(
        dir,
        (a, b, UNREACHED_SUPPLIER),
        rows,
        members,
        (federation, tables),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, &settings_with_room()))
}

/// `GET {fhir-base}/MedicationRequest` for the test patient, with the query
/// `extra` added.
fn search(extra: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "{FHIR}/MedicationRequest?patient.identifier={}%7C{PATIENT}{extra}",
        NAMESPACE.replace(':', "%3A")
    ))
    .body(Body::empty())
}

/// The resources of the entries of `bundle` whose search mode is `mode`.
fn entries<'a>(bundle: &'a Value, mode: &str) -> Vec<&'a Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry.pointer("/search/mode") == Some(&Value::from(mode)))
        .map(|entry| &entry["resource"])
        .collect()
}

/// The `fullUrl` of the entry of `bundle` whose resource has the id `id`.
fn full_url<'a>(bundle: &'a Value, id: &str) -> Option<&'a str> {
    bundle["entry"]
        .as_array()?
        .iter()
        .find(|entry| entry.pointer("/resource/id") == Some(&Value::from(id)))?
        .get("fullUrl")?
        .as_str()
}

/// The name of the `Organization` the `Provenance` of the entry at
/// `target` in `bundle` names as its agent.
fn provenance_agent(bundle: &Value, target: &str) -> Option<String> {
    let entries = bundle["entry"].as_array()?;
    let provenance = entries.iter().find(|entry| {
        entry.pointer("/resource/resourceType") == Some(&Value::from("Provenance"))
            && entry.pointer("/resource/target/0/reference") == Some(&Value::from(target))
    })?;
    let agent = provenance.pointer("/resource/agent/0/who/reference")?;
    let organization = entries
        .iter()
        .find(|entry| entry.get("fullUrl") == Some(agent))?;
    organization
        .pointer("/resource/name")?
        .as_str()
        .map(str::to_owned)
}

#[tokio::test]
async fn each_members_prescriptions_are_answered_as_sent_with_a_provenance_naming_it() -> TestResult
{
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(
        text.contains(RX_A),
        "Annex II 2.1: node A's prescription byte for byte: {text}"
    );
    assert!(text.contains(RX_B), "node B's prescription: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(bundle["type"], "searchset");
    assert_eq!(bundle["total"], Value::from(2));
    let ids: Vec<&Value> = entries(&bundle, "match")
        .into_iter()
        .map(|resource| &resource["id"])
        .collect();
    assert_eq!(ids, [&Value::from("rx-a1"), &Value::from("rx-b1")]);
    for (id, organisation) in [("rx-a1", "org-a"), ("rx-b1", "org-b")] {
        let url = full_url(&bundle, id).ok_or("a fullUrl")?;
        assert_eq!(
            provenance_agent(&bundle, url).as_deref(),
            Some(organisation),
            "one Provenance per prescription names its member"
        );
    }
    assert!(entries(&bundle, "outcome").is_empty(), "{bundle}");
    assert!(
        bundle
            .pointer("/link/0/url")
            .and_then(Value::as_str)
            .is_some_and(|url| url.ends_with(&format!(
                "/fhir/MedicationRequest?patient.identifier={}%7C{PATIENT}",
                NAMESPACE.replace(':', "%3A")
            ))),
        "FHIR R4 search §3.1.1.6: the self link names what the search used: {bundle}"
    );
    assert!(members.no_query().await?, "no query reaches a Query API");
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn each_member_is_asked_by_its_ehr_id_alone() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (proxy, own, ehr_id) in [
        (&members.proxy_a, &members.own_a, EHR_A),
        (&members.proxy_b, &members.own_b, EHR_B),
    ] {
        let journal = proxy.journal();
        let [request] = journal.as_slice() else {
            return Err(format!("one request at the own endpoint, got {}", journal.len()).into());
        };
        assert_eq!(request.path, "/fhir/MedicationRequest");
        let query = request.query.as_deref().unwrap_or_default();
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            [(
                "patient.identifier".to_owned(),
                format!("{EHR_SYSTEM}|{ehr_id}")
            )],
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
async fn a_silent_member_fails_the_search_and_is_named() -> TestResult {
    let members = Members::start(
        ResponseTemplate::new(500),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    assert!(
        text.contains("the search is incomplete") && text.contains("node-a-mpd (node-error)"),
        "the silent member is named: {text}"
    );
    assert!(!text.contains(RX_B), "no partial list under all-or-nothing");
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_unreachable_member_fails_the_search_with_a_gateway_time_out() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    members.proxy_b.set_fault(Fault::Refuse);
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "§11.4: {text}");
    assert!(text.contains("node-b-mpd (offline)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_silent_member_is_named_in_an_outcome_entry() -> TestResult {
    let members = Members::start(
        ResponseTemplate::new(500),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        ("best_effort = true\n", ""),
    )?;
    let mut request = search("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(1, entries(&bundle, "match").len(), "node B's: {bundle}");
    let outcomes = entries(&bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    assert_eq!(
        outcome.pointer("/issue/0/severity"),
        Some(&Value::from("warning")),
        "FHIR R4 http.html §3.1.0.9: no error in a searchset"
    );
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("incomplete"))
    );
    assert!(
        outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("node-a-mpd (node-error)")),
        "{outcome}"
    );
    Ok(())
}

#[tokio::test]
async fn a_prescription_identifier_two_members_hold_is_listed_once_per_member_and_warned()
-> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B_SAME], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(members.both(dir.path())?, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(
        2,
        entries(&bundle, "match").len(),
        "one per member: {bundle}"
    );
    let outcomes = entries(&bundle, "outcome");
    let [warning] = outcomes.as_slice() else {
        return Err(format!("one warning: {bundle}").into());
    };
    assert_eq!(
        warning.pointer("/issue/0/severity"),
        Some(&Value::from("warning"))
    );
    let diagnostics = warning
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.contains("node-a-mpd") && diagnostics.contains("node-b-mpd"),
        "both members are named: {diagnostics}"
    );
    assert!(
        !diagnostics.contains("SYNTHETIC-RX-A1"),
        "the warning never quotes the identifier: {diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_hold_the_patient_is_not_asked() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), &[("node-a", EHR_A)], ("", ""))?;
    let (status, text) = call(app, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(1, entries(&bundle, "match").len(), "{bundle}");
    assert!(
        members.proxy_b.journal().is_empty(),
        "node B was never asked"
    );
    Ok(())
}
