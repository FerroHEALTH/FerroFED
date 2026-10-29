// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The receipt of a dispensation (Regulation (EU) 2025/327 Art 14(1)(c)):
//! `POST {fhir-base}/MedicationDispense` is held to the HL7 Europe MPD
//! profile, its prescription is looked up at every member holding the
//! patient, by each member's own `ehr_id` alone, and matched at the gateway,
//! and the dispensation is created once at the one member that owns it, its
//! body as the client sent it (§5.4 scope note, N22, N33); no owner is
//! `404` as an unknown patient is, two owners `409` with an integrity
//! incident, the owner's refusal `409` and its silence `504`.

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_registry::incident::Kind;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::list::EHR_SYSTEM;
use super::super::receive::issue;
use super::{Members, RX_A, RX_B, RX_B_SAME, built, face_id, page, searchset};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL};
use crate::fhir::{FHIR, TestResult};
use crate::support::{call, send};

/// The `[[fhir.serve]]` tables declaring both members for both categories.
const SERVED_BOTH: &str = "\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\"node-a\", \"node-b\"]\n\n\
     [[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [\"node-a\", \"node-b\"]\n";

/// The system of the business identifier of every test dispensation.
pub(in crate::fhir) const DISPENSATION_SYSTEM: &str = "urn:oid:2.999.6.1";

/// The value of the business identifier of every test dispensation.
pub(in crate::fhir) const DISPENSATION_VALUE: &str = "SYNTHETIC-MD-1";

/// The member's own id of the dispensation it creates.
pub(super) const CREATED: &str = "md-1";

/// A dispensation of the prescription `authorizing`, its patient the test
/// patient in a contained `Patient`.
pub(in crate::fhir) fn dispensation(authorizing: &str) -> String {
    format!(
        r##"{{"resourceType":"MedicationDispense","meta":{{"profile":["{MPD_MEDICATION_DISPENSE}"]}},
  "extension":[{{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-10-16T10:00:00Z"}}],
  "contained":[{{"resourceType":"Patient","id":"pat","identifier":[{{"system":"{NAMESPACE}","value":"{PATIENT}"}}]}}],
  "identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}],
  "status":"completed","medicationCodeableConcept":{{"text":"Synthetic medicine A"}},
  "subject":{{"reference":"#pat"}},
  "performer":[{{"actor":{{"display":"Synthetic pharmacist"}}}}],
  "authorizingPrescription":[{authorizing}],
  "quantity":{{"value":30,"unit":"tablet"}}}}"##
    )
}

/// The `authorizingPrescription` naming node A's prescription by its
/// identifier.
pub(in crate::fhir) const BY_IDENTIFIER: &str =
    r#"{"identifier":{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}}"#;

/// `POST {fhir-base}/MedicationDispense` of `body`.
pub(super) fn post(body: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("{FHIR}/MedicationDispense"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_owned()))
}

/// The own endpoint `id` of `node`, at `base`, serving both categories.
fn own_endpoint(id: &str, (node, organisation): (&str, &str), base: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"{id}\"\nnode = \"{node}\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"{organisation}\"\n\
         categories = [\"Electronic-Prescriptions\", \"Electronic-Dispensations\"]\n\
         profiles = [\"{MPD_MEDICATION_REQUEST}\", \"{MPD_MEDICATION_DISPENSE}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// The gateway over `members`, both serving both categories, the patient
/// known at both.
pub(super) fn gateway(members: &Members, dir: &Path) -> Result<Router, Box<dyn Error>> {
    let own = format!(
        "{}{}",
        own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
        own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
    );
    built(
        dir,
        (&members.a.uri(), &members.b.uri()),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own,
        ("", SERVED_BOTH),
    )
}

/// Members holding `a` and `b`, each creating a dispensation with `created`.
pub(super) async fn members(
    (a, b): (&[&str], &[&str]),
    created: ResponseTemplate,
) -> Result<Members, Box<dyn Error>> {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", a, None)),
        page(searchset("https://cdr-b.example.org", b, None)),
    )
    .await?;
    for own in [&members.own_a, &members.own_b] {
        Mock::given(method("POST"))
            .and(path("/fhir/MedicationDispense"))
            .respond_with(created.clone())
            .mount(own)
            .await;
        // The patient's dispensations the duplicate check reads: none, below
        // the priority of any read a test mounts.
        Mock::given(method("GET"))
            .and(path("/fhir/MedicationDispense"))
            .respond_with(page(searchset("https://cdr.example.org", &[], None)))
            .with_priority(10)
            .mount(own)
            .await;
    }
    Ok(members)
}

/// A member's `201 Created` of the dispensation [`CREATED`].
pub(super) fn created() -> ResponseTemplate {
    ResponseTemplate::new(201)
        .insert_header(
            "location",
            format!("https://cdr.example.org/fhir/MedicationDispense/{CREATED}/_history/1"),
        )
        .insert_header("etag", "W/\"1\"")
}

/// The creates that reached `server`, each body as it arrived.
async fn creates(server: &Server) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .filter(|request| request.method == http::Method::POST)
        .map(|request| request.body)
        .collect())
}

/// Whether neither member was asked to create anything.
async fn none_written(members: &Members) -> Result<bool, Box<dyn Error>> {
    Ok(creates(&members.own_a).await?.is_empty() && creates(&members.own_b).await?.is_empty())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_dispensation_reaches_the_owner_alone_by_its_ehr_id_with_its_body_unchanged() -> TestResult
{
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let body = dispensation(BY_IDENTIFIER);
    let response = send(gateway(&members, dir.path())?, post(&body)?).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or("a Location")?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    assert!(
        location.ends_with(&format!("/fhir/MedicationDispense/{id}/_history/1")),
        "FHIR R4 http.html §3.1.0.8: the face's id and the member's version: {location}"
    );
    assert_eq!(
        Some("node-a-mpd"),
        response
            .headers()
            .get("openehr-federation-endpoint")
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert!(
        text.contains("node-a"),
        "the outcome names the member: {text}"
    );
    assert_eq!(
        vec![body.clone().into_bytes()],
        creates(&members.own_a).await?,
        "§5.4 scope note, N22: one create, its body byte for byte"
    );
    assert!(
        creates(&members.own_b).await?.is_empty(),
        "never a member that does not own it"
    );
    for proxy in [&members.proxy_a, &members.proxy_b] {
        for capture in proxy.journal() {
            let outside = format!(
                "{} {} {:?}",
                capture.path,
                capture.query.as_deref().unwrap_or_default(),
                capture.headers
            );
            for withheld in [PATIENT, PATIENT_TAIL, "SYNTHETIC-RX-A1"] {
                assert!(
                    !outside.contains(withheld),
                    "N33: no identifier in the path, the query or a header: {outside}"
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_dispensation_by_identifier_and_the_faces_reference_reaches_its_owner() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let both = format!(
        r#"{{"reference":"MedicationRequest/{}","identifier":{{"system":"urn:oid:2.999.5.2","value":"SYNTHETIC-RX-B1"}}}}"#,
        face_id("node-b-mpd", EHR_B, "rx-b1")?
    );
    let (status, text) = call(gateway(&members, dir.path())?, post(&dispensation(&both))?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(1, creates(&members.own_b).await?.len());
    assert_eq!(creates(&members.own_a).await?, Vec::<Vec<u8>>::new());
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_dispensation_naming_its_prescription_by_the_faces_reference_alone_is_refused()
-> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let reference = format!(
        r#"{{"reference":"MedicationRequest/{}"}}"#,
        face_id("node-b-mpd", EHR_B, "rx-b1")?
    );
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(&reference))?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("business-rule", code);
    assert_eq!(
        "name the prescription by its identifier; the member cannot resolve the gateway's reference",
        diagnostics,
        "§5.4 scope note, N33: the body reaches its member unchanged, so it must name what the member holds"
    );
    assert!(members.unasked(), "nothing reaches a member");
    Ok(())
}

#[tokio::test]
async fn a_reference_and_an_identifier_that_disagree_write_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let both = format!(
        r#"{{"reference":"MedicationRequest/{}","identifier":{{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}}}}"#,
        face_id("node-b-mpd", EHR_B, "rx-b1")?
    );
    let (status, text) = call(gateway(&members, dir.path())?, post(&dispensation(&both))?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn no_owner_answers_as_an_unknown_patient_does() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let unknown = r#"{"identifier":{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-NONE"}}"#;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(unknown))?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let unowned = issue(&text)?;
    let stranger = dispensation(BY_IDENTIFIER).replace(PATIENT, "SENTINEL-STRANGER-1");
    let (status, text) = call(gateway(&members, dir.path())?, post(&stranger)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!(
        unowned,
        issue(&text)?,
        "RFC 9110 §15.5.5: one answer for no prescription and no patient"
    );
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn a_prescription_two_members_hold_is_refused_with_an_integrity_incident() -> TestResult {
    let members = members((&[RX_A], &[RX_B_SAME]), created()).await?;
    let dir = tempfile::tempdir()?;
    let before = Kind::PrescriptionIdentifierCollision.emitted();
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("conflict", code);
    assert!(
        diagnostics.contains("node-a-mpd") && diagnostics.contains("node-b-mpd"),
        "N42 by analogy: both claimants are named: {diagnostics}"
    );
    assert!(!diagnostics.contains("SYNTHETIC-RX-A1"), "{diagnostics}");
    assert_eq!(
        before + 1,
        Kind::PrescriptionIdentifierCollision.emitted(),
        "one integrity incident"
    );
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn two_matches_at_one_member_are_refused_with_no_incident() -> TestResult {
    let twin = RX_A.replace("\"rx-a1\"", "\"rx-a9\"");
    let members = members((&[RX_A, &twin], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let before = Kind::PrescriptionIdentifierCollision.emitted();
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert!(issue(&text)?.1.contains("node-a-mpd"), "{text}");
    assert_eq!(before, Kind::PrescriptionIdentifierCollision.emitted());
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn prescriptions_held_by_different_members_write_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let spanning = format!(
        r#"{BY_IDENTIFIER},{{"identifier":{{"system":"urn:oid:2.999.5.2","value":"SYNTHETIC-RX-B1"}}}}"#
    );
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(&spanning))?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "§2.3: {text}");
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn a_member_silent_at_the_lookup_refuses_the_dispensation() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        ResponseTemplate::new(500),
    )
    .await?;
    Mock::given(method("POST"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(created())
        .mount(&members.own_a)
        .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(
        issue(&text)?.1.contains("node-b-mpd (node-error)"),
        "§11.3: the silent member is named: {text}"
    );
    assert!(none_written(&members).await?, "it could be the owner");
    Ok(())
}

#[tokio::test]
async fn a_header_naming_another_member_writes_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let mut request = post(&dispensation(BY_IDENTIFIER))?;
    request
        .headers_mut()
        .insert("openehr-federation-endpoint", "node-b-mpd".parse()?);
    let (status, text) = call(gateway(&members, dir.path())?, request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(none_written(&members).await?);
    let mut request = post(&dispensation(BY_IDENTIFIER))?;
    request
        .headers_mut()
        .insert("openehr-federation-endpoint", "node-a-mpd".parse()?);
    let (status, text) = call(gateway(&members, dir.path())?, request).await?;
    assert_eq!(
        StatusCode::CREATED,
        status,
        "the owner's own endpoint: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_owners_refusal_is_a_conflict_naming_it() -> TestResult {
    for refusal in [StatusCode::CONFLICT, StatusCode::UNPROCESSABLE_ENTITY] {
        let members = members(
            (&[RX_A], &[RX_B]),
            ResponseTemplate::new(refusal.as_u16()).set_body_raw(
                br#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"business-rule"}]}"#
                    .to_vec(),
                "application/fhir+json",
            ),
        )
        .await?;
        let dir = tempfile::tempdir()?;
        let (status, text) = call(
            gateway(&members, dir.path())?,
            post(&dispensation(BY_IDENTIFIER))?,
        )
        .await?;
        assert_eq!(StatusCode::CONFLICT, status, "{refusal}: {text}");
        let (code, diagnostics) = issue(&text)?;
        assert_eq!("conflict", code);
        assert!(
            diagnostics.contains("node-a") && diagnostics.contains(refusal.as_str()),
            "{diagnostics}"
        );
    }
    Ok(())
}

/// The member's issues of `text`, after the gateway's first one.
fn relayed_issues(text: &str) -> Result<Vec<serde_json::Value>, serde_json::Error> {
    let outcome: serde_json::Value = serde_json::from_str(text)?;
    let issues = outcome
        .get("issue")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(issues.into_iter().skip(1).collect())
}

/// MyHealth@EU Requirements Catalogue v10.0.0 §1.25: the owner's own
/// reason travels after the gateway's issue, unaltered, so the contact point
/// can relay why the prescription was not dispensable.
#[tokio::test]
async fn the_owners_own_issues_follow_the_gateways_on_a_refusal_and_an_error() -> TestResult {
    let reason = br#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"business-rule","diagnostics":"the prescription is already fully dispensed"}]}"#;
    for (answered, expected) in [
        (StatusCode::CONFLICT, StatusCode::CONFLICT),
        (StatusCode::INTERNAL_SERVER_ERROR, StatusCode::BAD_GATEWAY),
    ] {
        let members = members(
            (&[RX_A], &[RX_B]),
            ResponseTemplate::new(answered.as_u16())
                .set_body_raw(reason.to_vec(), "application/fhir+json"),
        )
        .await?;
        let dir = tempfile::tempdir()?;
        let (status, text) = call(
            gateway(&members, dir.path())?,
            post(&dispensation(BY_IDENTIFIER))?,
        )
        .await?;
        assert_eq!(expected, status, "{answered}: {text}");
        let (_, diagnostics) = issue(&text)?;
        assert!(
            diagnostics.contains("node-a") && diagnostics.contains("own issues follow"),
            "{diagnostics}"
        );
        let relayed = relayed_issues(&text)?;
        assert_eq!(1, relayed.len(), "{text}");
        assert_eq!(
            Some("the prescription is already fully dispensed"),
            relayed[0]["diagnostics"].as_str(),
            "{text}"
        );
        assert_eq!(Some("business-rule"), relayed[0]["code"].as_str(), "{text}");
    }
    Ok(())
}

/// A member body that is no `OperationOutcome` is left out.
#[tokio::test]
async fn a_member_body_that_is_no_outcome_is_not_relayed() -> TestResult {
    for body in [
        &br#"{"resourceType":"Patient","id":"x"}"#[..],
        &b"not json"[..],
    ] {
        let members = members(
            (&[RX_A], &[RX_B]),
            ResponseTemplate::new(409).set_body_raw(body.to_vec(), "application/fhir+json"),
        )
        .await?;
        let dir = tempfile::tempdir()?;
        let (status, text) = call(
            gateway(&members, dir.path())?,
            post(&dispensation(BY_IDENTIFIER))?,
        )
        .await?;
        assert_eq!(StatusCode::CONFLICT, status, "{text}");
        assert!(relayed_issues(&text)?.is_empty(), "{text}");
        assert!(!issue(&text)?.1.contains("own issues follow"), "{text}");
    }
    Ok(())
}

#[tokio::test]
async fn an_owner_that_does_not_answer_is_asked_once_and_the_outcome_is_unknown() -> TestResult {
    let members = members(
        (&[RX_A], &[RX_B]),
        created().set_delay(Duration::from_secs(6)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "{text}");
    assert!(issue(&text)?.1.contains("unknown"), "{text}");
    assert_eq!(1, creates(&members.own_a).await?.len(), "never sent again");
    Ok(())
}

#[tokio::test]
async fn a_dispensation_that_breaks_its_profile_asks_no_member() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let broken = dispensation(BY_IDENTIFIER).replace(
        r#""performer":[{"actor":{"display":"Synthetic pharmacist"}}],"#,
        "",
    );
    let (status, text) = call(gateway(&members, dir.path())?, post(&broken)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(members.unasked(), "nothing reaches a member");
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(r#"{"resourceType":"MedicationRequest"}"#)?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(members.unasked());
    Ok(())
}

/// The count of the dispensations that ended as `outcome`, scraped from a
/// fresh surface: the counts are the process's, so any surface reads them.
fn counted(outcome: &str) -> Result<u64, Box<dyn Error>> {
    let samples = crate::metrics::parse(&ferrofed_server::metrics::Metrics::default().render()?)?;
    for sample in samples
        .iter()
        .filter(|sample| sample.name == "ferrofed_fhir_dispensations_total")
    {
        assert_eq!(
            vec!["outcome"],
            sample.labels.keys().collect::<Vec<_>>(),
            "outcome is the only label: {sample:?}"
        );
    }
    Ok(crate::metrics::count(
        &samples,
        "ferrofed_fhir_dispensations_total",
        &[("outcome", outcome)],
    )
    .ok_or("every outcome is exposed")?
    .parse()?)
}

#[tokio::test]
async fn a_dispensation_with_no_owner_is_counted_with_no_value() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let (unowned, created_before) = (counted("unowned")?, counted("created")?);
    let unknown = r#"{"identifier":{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-NONE"}}"#;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(unknown))?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!(
        unowned + 1,
        counted("unowned")?,
        "one dispensation with no owner"
    );
    assert_eq!(created_before, counted("created")?);
    let exposed = ferrofed_server::metrics::Metrics::default().render()?;
    for value in ["SYNTHETIC-RX-NONE", PATIENT, "node-a"] {
        assert!(
            !exposed
                .lines()
                .filter(|line| line.contains("ferrofed_fhir_dispensations_total"))
                .any(|line| line.contains(value)),
            "no value reaches the series: {value}"
        );
    }
    Ok(())
}
#[cfg(feature = "binding-ihe")]
mod recorded {
    //! The access record of a dispensation (Annex II 3.2) and its place,
    //! reserved before the write.

    use ferrofed_testkit::atna_feed::FeedRepository;
    use http::StatusCode;

    use super::{
        BY_IDENTIFIER, EHR_A, EHR_B, Members, RX_A, RX_B, SERVED_BOTH, built, created, creates,
        dispensation, members, own_endpoint, post,
    };
    use crate::feed_audit::{SETTLE, audit_tables};
    use crate::fhir::TestResult;
    use crate::support::call;

    /// The gateway over `members` recording to `repository` with the
    /// `[audit.repository]` keys `extra`.
    fn recording(
        members: &Members,
        dir: &std::path::Path,
        repository: &FeedRepository,
        extra: &str,
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let own = format!(
            "{}{}",
            own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
            own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
        );
        built(
            dir,
            (&members.a.uri(), &members.b.uri()),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
            &own,
            (
                "",
                &format!("{SERVED_BOTH}{}", audit_tables(repository, extra)),
            ),
        )
    }

    #[tokio::test]
    async fn a_dispensation_is_recorded_as_a_creation_of_both_categories() -> TestResult {
        let members = members((&[RX_A], &[RX_B]), created()).await?;
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = recording(&members, dir.path(), &repository, "")?;
        let (status, text) = call(app, post(&dispensation(BY_IDENTIFIER))?).await?;
        assert_eq!(StatusCode::CREATED, status, "{text}");
        let records: Vec<String> = repository
            .wait_for(1, SETTLE)
            .await
            .into_iter()
            .filter(|record| record.contains("ehds-categories"))
            .collect();
        let [record] = records.as_slice() else {
            return Err(format!("one record: {records:#?}").into());
        };
        for expected in [
            "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
            "eehrxf-document-priority-category-cs|Electronic-Prescriptions:construction",
            "create MedicationDispense",
            "node-a-mpd",
            "node-b-mpd",
            EHR_A,
        ] {
            assert!(record.contains(expected), "{expected}: {record}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_dispensation_read_is_recorded_as_a_read_of_dispensations() -> TestResult {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let members = members((&[RX_A], &[RX_B]), created()).await?;
        let held = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationDispense","id":"md-1","status":"completed"},"search":{"mode":"match"}}]}"#;
        Mock::given(method("GET"))
            .and(path("/fhir/MedicationDispense"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(held.as_bytes().to_vec(), "application/fhir+json"),
            )
            .mount(&members.own_a)
            .await;
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = recording(&members, dir.path(), &repository, "")?;
        let id = super::face_id("node-a-mpd", EHR_A, super::CREATED)?;
        let request = http::Request::get(format!("{}/MedicationDispense/{id}", crate::fhir::FHIR))
            .body(axum::body::Body::empty())?;
        let (status, text) = call(app, request).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        let records: Vec<String> = repository
            .wait_for(1, SETTLE)
            .await
            .into_iter()
            .filter(|record| record.contains("ehds-categories"))
            .collect();
        let [record] = records.as_slice() else {
            return Err(format!("one record: {records:#?}").into());
        };
        for expected in [
            "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
            "MedicationDispense/md-1",
            "node-a-mpd",
            EHR_A,
        ] {
            assert!(record.contains(expected), "{expected}: {record}");
        }
        assert!(
            !record.contains("Electronic-Prescriptions:construction"),
            "a dispensation read reaches no prescription: {record}"
        );
        assert!(record.contains("\"action\":\"R\""), "a read: {record}");
        Ok(())
    }

    #[tokio::test]
    async fn a_dispensation_version_read_is_recorded_as_a_read_of_dispensations() -> TestResult {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let members = members((&[RX_A], &[RX_B]), created()).await?;
        let held = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationDispense","id":"md-1","meta":{"versionId":"1"},"status":"completed"},"search":{"mode":"match"}}]}"#;
        Mock::given(method("GET"))
            .and(path("/fhir/MedicationDispense"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(held.as_bytes().to_vec(), "application/fhir+json"),
            )
            .mount(&members.own_a)
            .await;
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = recording(&members, dir.path(), &repository, "")?;
        let id = super::face_id("node-a-mpd", EHR_A, super::CREATED)?;
        let request = http::Request::get(format!(
            "{}/MedicationDispense/{id}/_history/1",
            crate::fhir::FHIR
        ))
        .body(axum::body::Body::empty())?;
        let (status, text) = call(app, request).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        let records: Vec<String> = repository
            .wait_for(1, SETTLE)
            .await
            .into_iter()
            .filter(|record| record.contains("ehds-categories"))
            .collect();
        let [record] = records.as_slice() else {
            return Err(format!("one record: {records:#?}").into());
        };
        for expected in [
            "eehrxf-document-priority-category-cs|Electronic-Dispensations:construction",
            "MedicationDispense/md-1",
            "node-a-mpd",
            "\"action\":\"R\"",
        ] {
            assert!(record.contains(expected), "{expected}: {record}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_dispensation_whose_record_has_no_place_reaches_no_member() -> TestResult {
        let members = members((&[RX_A], &[RX_B]), created()).await?;
        let repository = FeedRepository::start().await;
        repository.set_up(false);
        let dir = tempfile::tempdir()?;
        let app = recording(&members, dir.path(), &repository, "spool_max_events = 1")?;
        let (status, text) = call(app.clone(), post(&dispensation(BY_IDENTIFIER))?).await?;
        assert_eq!(
            StatusCode::CREATED,
            status,
            "the first fills the spool: {text}"
        );
        let (status, text) = call(app, post(&dispensation(BY_IDENTIFIER))?).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
        assert_eq!(
            1,
            creates(&members.own_a).await?.len(),
            "Annex II 3.2: the refused dispensation never reaches its member"
        );
        Ok(())
    }
}

/// Node A's own endpoint answering the patient's dispensations with
/// `answer`, above the default answer of none.
async fn dispensations_at_a(members: &Members, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(answer)
        .with_priority(1)
        .mount(&members.own_a)
        .await;
}

/// A dispensation node A already holds, under the test dispensation's
/// business identifier.
fn already_held() -> String {
    format!(
        r#"{{"resourceType":"MedicationDispense","id":"md-0","status":"completed","identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}],"medicationCodeableConcept":{{"text":"Synthetic medicine A"}},"subject":{{"reference":"Patient/p-a"}}}}"#
    )
}

#[tokio::test]
async fn a_dispensation_the_owner_already_holds_is_refused_as_a_duplicate_with_nothing_written()
-> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    dispensations_at_a(
        &members,
        page(searchset(
            "https://cdr-a.example.org",
            &[&already_held()],
            None,
        )),
    )
    .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert!(text.contains("\"code\":\"duplicate\""), "{text}");
    let held = face_id("node-a-mpd", EHR_A, "md-0")?;
    assert!(
        text.contains(&format!("MedicationDispense/{held}")),
        "the refusal names the dispensation already written: {text}"
    );
    assert!(none_written(&members).await?, "nothing is written twice");
    for capture in members.proxy_a.journal() {
        let outside = format!(
            "{} {}",
            capture.path,
            capture.query.as_deref().unwrap_or_default()
        );
        assert!(
            !outside.contains(DISPENSATION_VALUE),
            "N33: the identifier is matched at the gateway, never sent: {outside}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn another_dispensation_of_the_patient_is_no_duplicate_and_is_written() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let other = already_held().replace(DISPENSATION_VALUE, "SYNTHETIC-MD-OTHER");
    dispensations_at_a(
        &members,
        page(searchset("https://cdr-a.example.org", &[&other], None)),
    )
    .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(1, creates(&members.own_a).await?.len(), "one create");
    Ok(())
}

#[tokio::test]
async fn an_owner_that_does_not_answer_the_duplicate_check_writes_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    dispensations_at_a(&members, ResponseTemplate::new(503)).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert!(
        text.contains("cannot be ruled out"),
        "a read that fails is never taken as no duplicate: {text}"
    );
    assert!(none_written(&members).await?);
    Ok(())
}

#[tokio::test]
async fn a_dispensation_with_no_business_identifier_is_refused_naming_the_element() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let unidentified = dispensation(BY_IDENTIFIER).replace(
        &format!(
            r#"  "identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}],
"#
        ),
        "",
    );
    assert!(!unidentified.contains(DISPENSATION_VALUE));
    let (status, text) = call(gateway(&members, dir.path())?, post(&unidentified)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(text.contains("MedicationDispense.identifier"), "{text}");
    assert!(none_written(&members).await?);
    Ok(())
}
