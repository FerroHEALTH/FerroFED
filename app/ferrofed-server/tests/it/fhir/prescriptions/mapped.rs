// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The map mode of the prescription face (A102 point 1): a member declared
//! with `mode = "map"` is asked the reserved medication order query over
//! ITS-REST by its own `ehr_id` alone (§5.4.1, N33), only the compositions
//! its own system created are its prescriptions, each is mapped into an HL7
//! Europe MPD 1.0.0 `MedicationRequest-eu-mpd` listed beside a native
//! member's with a `Provenance` naming its member, a silent member fails the
//! search under all-or-nothing (§11.3, §11.4), and the read by the id the
//! search names asks that member alone for that composition.

use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_testkit::medication;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{RX_B, built, entries, own, own_endpoint, page, provenance_agent, search, searchset};
use crate::facade::{EHR_A, EHR_B, PATIENT, PATIENT_TAIL, received};
use crate::fhir::{FHIR, TestResult};
use crate::support::call;

/// The versioned object node A's own medication order composition is held
/// under.
const OWN: &str = "5f0c2a9e-0000-4000-8000-0000000000a1";

/// The version id of node A's own order, which node A's system created.
const OWN_UID: &str = "5f0c2a9e-0000-4000-8000-0000000000a1::cdr-a.example.org::1";

/// The version id of an order node A holds that node B's system created.
const FOREIGN_UID: &str = "5f0c2a9e-0000-4000-8000-0000000000b9::cdr-b.example.org::1";

/// The archetype the medication order query selects compositions by.
const ORDER: &str = "openEHR-EHR-INSTRUCTION.medication_order.v3";

/// The synthetic medication order composition of the version id `uid`,
/// holding the prescription `prescription`.
fn order(uid: &str, prescription: &str) -> Result<String, Box<dyn Error>> {
    let medication::Written {
        template,
        archetypes,
    } = medication::template()?;
    let source = Template::Adl2 {
        template: &template,
        archetypes: &archetypes,
    }
    .source()?;
    medication::order_composition(&source, uid, prescription)
}

/// A Query API answering the medication order query with the compositions
/// `held`, each its composition and its version id.
async fn query_api(held: &[(String, &str)]) -> Server {
    let server = Server::start().await;
    let columns = r#"[{"name":"composition","path":"c"},{"name":"uid","path":"c/uid/value"},{"name":"template_id","path":"c/archetype_details/template_id/value"}]"#;
    let rows: Vec<String> = held
        .iter()
        .map(|(composition, uid)| format!(r#"[{composition},"{uid}","{}"]"#, medication::TEMPLATE))
        .collect();
    let body = format!(
        r#"{{"q":"node","columns":{columns},"rows":[{}]}}"#,
        rows.join(",")
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(ORDER))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The `[[fhir.serve]]` table declaring node A in the map mode, with the
/// vendored medication template, and node B on its own endpoint.
fn served() -> String {
    let quoted = |path: &Path| toml::Value::String(path.display().to_string()).to_string();
    let archetypes: Vec<String> = medication::archetype_paths()
        .iter()
        .map(|path| quoted(path))
        .collect();
    format!(
        "\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\n  \
         {{ member = \"node-a\", mode = \"map\", template = {}, archetypes = [{}] }},\n  \"node-b\",\n]\n",
        quoted(&medication::template_path()),
        archetypes.join(", ")
    )
}

/// The gateway over node A's Query API `a`, in the map mode, and node B,
/// whose Query API is `b` and whose own endpoint is `own_b`, the patient
/// known at both.
fn gateway(
    dir: &Path,
    (a, b, own_b): (&Server, &Server, &Server),
) -> Result<Router, Box<dyn Error>> {
    built(
        dir,
        (&a.uri(), &b.uri()),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own_endpoint("node-b-mpd", ("node-b", "org-b"), &own_b.uri()),
        ("", &served()),
    )
}

/// The match entries of the `searchset` `bundle` that claim the MPD
/// `MedicationRequest` profile and come from the mapping.
fn mapped(bundle: &Value) -> Vec<&Value> {
    entries(bundle, "match")
        .into_iter()
        .filter(|resource| resource.pointer("/subject/identifier").is_some())
        .collect()
}

#[tokio::test]
async fn a_map_mode_members_own_orders_are_mapped_beside_a_native_members() -> TestResult {
    let a = query_api(&[
        (order(OWN_UID, "synthetic-rx-own")?, OWN_UID),
        (order(FOREIGN_UID, "synthetic-rx-foreign")?, FOREIGN_UID),
    ])
    .await;
    let b = Server::start().await;
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(dir.path(), (&a, &b, &own_b))?, search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let listed = mapped(&bundle);
    let [prescription] = listed.as_slice() else {
        return Err(format!("A102 point 1: node A's own order alone: {bundle}").into());
    };
    assert_eq!(
        prescription.pointer("/identifier/0/value"),
        Some(&Value::from("synthetic-rx-own"))
    );
    assert_eq!(
        prescription.pointer("/meta/profile/0"),
        Some(&Value::from(MPD_MEDICATION_REQUEST))
    );
    assert_eq!(
        prescription.pointer("/subject/identifier/value"),
        Some(&Value::from(PATIENT)),
        "FHIR R4 Reference.identifier: the patient the search names"
    );
    assert!(
        !text.contains("synthetic-rx-foreign"),
        "an order another system created is not node A's: {text}"
    );
    let id = prescription["id"].as_str().ok_or("an id")?;
    let url = format!("https://gw.example.org{FHIR}/MedicationRequest/{id}");
    assert_eq!(
        provenance_agent(&bundle, &url).as_deref(),
        Some("org-a"),
        "the Provenance names the member it was mapped from"
    );
    let included: Vec<&Value> = entries(&bundle, "include")
        .into_iter()
        .filter(|resource| resource["resourceType"] == "Medication")
        .collect();
    assert_eq!(1, included.len(), "the product the order names: {bundle}");
    assert_eq!(bundle["total"], Value::from(2), "node B's prescription too");
    for asked in received(&a).await? {
        assert!(asked.contains(EHR_A), "N33: node A's ehr_id: {asked}");
        for withheld in [PATIENT, PATIENT_TAIL] {
            assert!(
                !asked.contains(withheld),
                "N33: no patient identifier: {asked}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_silent_map_mode_member_fails_the_search_under_all_or_nothing() -> TestResult {
    let a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&a)
        .await;
    let b = Server::start().await;
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(dir.path(), (&a, &b, &own_b))?, search("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        text.contains("node-a-pub"),
        "§11.4: the silent member is named: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_mapped_prescription_is_read_at_its_member_by_its_composition() -> TestResult {
    let composition = order(OWN_UID, "synthetic-rx-own")?;
    let a = query_api(&[(composition.clone(), OWN_UID)]).await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{OWN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(composition.into_bytes(), "application/json"),
        )
        .mount(&a)
        .await;
    let b = Server::start().await;
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let dir = tempfile::tempdir()?;
    let router = gateway(dir.path(), (&a, &b, &own_b))?;
    let (status, text) = call(router.clone(), search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let id = mapped(&bundle)
        .first()
        .and_then(|resource| resource["id"].as_str())
        .ok_or("a mapped prescription")?
        .to_owned();
    let read = Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())?;
    let (status, text) = call(router, read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let resource: Value = serde_json::from_str(&text)?;
    assert_eq!(
        resource["id"],
        Value::from(id.as_str()),
        "FHIR R4 http.html §3.1.0.2"
    );
    assert_eq!(
        resource.pointer("/identifier/0/value"),
        Some(&Value::from("synthetic-rx-own"))
    );
    assert_eq!(
        resource.pointer("/subject/identifier/value"),
        Some(&Value::from(PATIENT)),
        "the patient the identity binding names behind the ehr_id"
    );
    assert!(
        own_b.received_requests().await.unwrap_or_default().len() == 1,
        "the read asks node A alone"
    );
    Ok(())
}

#[tokio::test]
async fn a_mapped_id_whose_composition_another_system_created_has_no_owner() -> TestResult {
    let own_order = order(OWN_UID, "synthetic-rx-own")?;
    let a = query_api(&[(own_order, OWN_UID)]).await;
    let foreign = order(FOREIGN_UID, "synthetic-rx-foreign")?;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{OWN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(foreign.into_bytes(), "application/json"),
        )
        .mount(&a)
        .await;
    let b = Server::start().await;
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let dir = tempfile::tempdir()?;
    let router = gateway(dir.path(), (&a, &b, &own_b))?;
    let (_, text) = call(router.clone(), search("")?).await?;
    let bundle: Value = serde_json::from_str(&text)?;
    let id = mapped(&bundle)
        .first()
        .and_then(|resource| resource["id"].as_str())
        .ok_or("a mapped prescription")?
        .to_owned();
    let read = Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())?;
    let (status, text) = call(router, read).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "A102 point 1: {text}");
    Ok(())
}
