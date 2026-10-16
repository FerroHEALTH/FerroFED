// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read of one prescription by the id the search names it by:
//! `GET {fhir-base}/MedicationRequest/{id}` is answered from the member the
//! id names, by its own `ehr_id` and the member's own id of the resource
//! alone (§5.4.1, N33), with the member named (§7a.3, N31) and the id the
//! request named (FHIR R4 `http.html` §3.1.0.2); an id with no owner answers
//! `404` alike whatever the reason, an `ehr_id` two members claim `409` with
//! the integrity incident (§12.5.2, N42), a member answering two `409`
//! without one, a caller confined to another patient `403`, and a silent
//! member is named (§11.1).

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::proxy::Fault;
use http::{Request, StatusCode, header};
use serde_json::Value;
use wiremock::ResponseTemplate;

use super::{
    AT_A, Members, RX_A, RX_B, SERVED, as_face, face_id, own_endpoint, page, search, searchset,
};
use crate::auth::{bearing, claims, minted};
use crate::ehr_id_collision::{captured, incidents};
use crate::facade::{EHR_A, EHR_B, PATIENT, PATIENT_TAIL, wire};
use crate::fhir::receive::{OTHER_A, bound, rows};
use crate::fhir::{FHIR, TestResult, UNREACHED_SUPPLIER, text_registered};
use crate::support::{call, issuer};

/// `GET {fhir-base}/MedicationRequest/{id}`.
fn read(id: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())
}

/// The members, each answering with its own prescription.
async fn answering() -> Result<Members, Box<dyn Error>> {
    Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await
}

/// The `id` the searchset `text` names its match of the member's own id
/// `own` at `at` by.
fn listed_id(text: &str, own: &str, at: (&str, &str)) -> Result<String, Box<dyn Error>> {
    let id = face_id(at.0, at.1, own)?;
    let bundle: Value = serde_json::from_str(text)?;
    let listed = super::entries(&bundle, "match")
        .into_iter()
        .any(|resource| resource["id"] == id.as_str());
    if !listed {
        return Err(format!("the search lists {own} by its face id: {text}").into());
    }
    Ok(id)
}

#[tokio::test]
async fn a_prescription_the_search_lists_is_read_at_its_member_as_sent() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let (status, text) = call(app.clone(), search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let id = listed_id(&text, "rx-a1", AT_A)?;
    let asked_b = members.proxy_b.journal().len();
    let (status, acting, text) = crate::path_ehr_id::answer(app, read(&id)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answered: fhir_types::codec::Value = serde_json::from_str(&text)?;
    assert_eq!(
        as_face(RX_A, AT_A)?,
        answered,
        "FHIR R4 http.html §3.1.0.2: the member's prescription, its id the one read: {text}"
    );
    assert_eq!(
        Some("node-a-mpd"),
        acting.as_deref(),
        "§7a.3, N31: the owning member is named"
    );
    assert_eq!(
        2,
        members.proxy_a.journal().len(),
        "the search, then the read"
    );
    assert_eq!(
        asked_b,
        members.proxy_b.journal().len(),
        "the read reaches the member its id names alone"
    );
    assert!(members.no_query().await?, "no query reaches a Query API");
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_member_is_asked_by_its_ehr_id_and_its_own_id_alone() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let (status, text) = call(members.both(dir.path())?, read(&id)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let journal = members.proxy_a.journal();
    let [request] = journal.as_slice() else {
        return Err(format!("one request, got {}", journal.len()).into());
    };
    assert_eq!(request.path, "/fhir/MedicationRequest");
    let pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(request.query.as_deref().unwrap_or_default().as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
    assert_eq!(
        pairs,
        [
            (
                "patient.identifier".to_owned(),
                format!("{}|{EHR_A}", super::EHR_SYSTEM)
            ),
            ("_id".to_owned(), "rx-a1".to_owned()),
        ],
        "N33: the member's ehr_id and its own id alone (FHIR R4 search §3.1.1.4.1)"
    );
    for withheld in [PATIENT, PATIENT_TAIL, id.as_str()] {
        assert!(
            !members.proxy_a.journal_contains(withheld.as_bytes()),
            "N33: no identifier and no face id in the path, the query or a header"
        );
        let captured = wire(&members.own_a).await?;
        assert!(
            !captured.contains(withheld),
            "N33: none in the caller claims either: {captured}"
        );
    }
    assert!(
        members.proxy_b.journal().is_empty(),
        "node B is never asked"
    );
    Ok(())
}

#[tokio::test]
async fn an_id_with_no_owner_answers_404_alike_whatever_the_reason() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let unsent = [
        String::from("synthetic-rx-1"),
        face_id("node-z-mpd", EHR_A, "rx-a1")?,
        face_id("node-a-pub", EHR_A, "rx-a1")?,
    ];
    let mut bodies = Vec::new();
    for id in &unsent {
        let (status, text) = call(app.clone(), read(id)?).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{id}: {text}");
        bodies.push(serde_json::from_str::<Value>(&text)?);
    }
    assert!(members.unasked(), "an id naming no member is sent nowhere");
    let (status, text) = call(app, read(&face_id(AT_A.0, AT_A.1, "rx-a9")?)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    bodies.push(serde_json::from_str::<Value>(&text)?);
    assert_eq!(1, members.proxy_a.journal().len(), "the member was asked");
    let first = bodies.first().ok_or("an answer")?;
    assert_eq!(first["issue"][0]["code"], "not-found", "{first}");
    for body in &bodies {
        assert_eq!(first, body, "§11.3: every id with no owner answers alike");
    }
    Ok(())
}

#[tokio::test]
async fn an_id_naming_a_member_that_places_its_ehr_id_elsewhere_has_no_owner() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let (status, text) = call(app.clone(), search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let asked = members.proxy_a.journal().len();
    let forged = face_id(AT_A.0, EHR_B, "rx-a1")?;
    let (status, text) = call(app, read(&forged)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "§12.5.1: {text}");
    assert_eq!(
        asked,
        members.proxy_a.journal().len(),
        "node B's ehr_id is never asked of node A"
    );
    Ok(())
}

// conformance: CP-33 track-11
#[tokio::test]
async fn an_ehr_id_two_members_claim_refuses_the_read_with_one_incident() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_A)],
        ("", ""),
    )?;
    let (status, text) = call(app.clone(), search("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let asked = (
        members.proxy_a.journal().len(),
        members.proxy_b.journal().len(),
    );
    let id = face_id(AT_A.0, EHR_A, "rx-a1")?;
    let (answers, logs) = captured(&app, vec![read(&id)?]).await?;
    let [(status, acting, text)] = answers.as_slice() else {
        return Err("one answer".into());
    };
    assert_eq!(StatusCode::CONFLICT, *status, "§12.5.2, N42: {text}");
    assert!(
        text.contains("node-a-pub") && text.contains("node-b-pub"),
        "both claimants are named: {text}"
    );
    assert!(acting.is_none(), "no member acted: {text}");
    assert_eq!(
        asked,
        (
            members.proxy_a.journal().len(),
            members.proxy_b.journal().len()
        ),
        "no claimant is asked"
    );
    let found = incidents(&logs)?;
    let [incident] = found.as_slice() else {
        return Err(format!("one incident for one refusal: {logs}").into());
    };
    assert_eq!("EhrIdCollision", incident.kind);
    assert_eq!("[node-a-pub, node-b-pub]", incident.claimants);
    Ok(())
}

#[tokio::test]
async fn a_member_answering_two_prescriptions_for_one_id_is_a_conflict_with_no_incident()
-> TestResult {
    let twice = searchset("https://cdr-a.example.org", &[RX_A, RX_A], None);
    let members = Members::start(
        page(twice),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let (answers, logs) = captured(&app, vec![read(&id)?]).await?;
    let [(status, _, text)] = answers.as_slice() else {
        return Err("one answer".into());
    };
    assert_eq!(StatusCode::CONFLICT, *status, "{text}");
    assert!(text.contains("node-a-mpd"), "the member is named: {text}");
    assert!(
        !text.contains("Synthetic medicine"),
        "neither prescription is answered: {text}"
    );
    assert!(incidents(&logs)?.is_empty(), "no incident: {logs}");
    Ok(())
}

#[tokio::test]
async fn a_member_answering_another_prescription_is_never_passed_through() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let id = face_id(AT_A.0, AT_A.1, "rx-other")?;
    let (status, text) = call(members.both(dir.path())?, read(&id)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(text.contains("node-a-mpd (node-error)"), "{text}");
    assert!(!text.contains("Synthetic medicine"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_silent_member_is_named_and_nothing_is_answered() -> TestResult {
    let members = Members::start(
        ResponseTemplate::new(500),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let (status, text) = call(app.clone(), read(&id)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.1: {text}");
    assert!(text.contains("node-a-mpd (node-error)"), "{text}");
    members.proxy_a.set_fault(Fault::Refuse);
    let (status, text) = call(app, read(&id)?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "§11.4: {text}");
    assert!(text.contains("node-a-mpd (offline)"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_read_takes_the_composition_read_scope() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    for (scope, admitted) in [
        ("user/composition-*.s", false),
        ("user/aql-*.s user/composition-org.example.rx.v1.r", false),
        ("user/composition-*.r", true),
    ] {
        let mut claims = crate::support::claims();
        claims.scope = Some(scope.to_owned());
        let token = issuer().mint(&claims)?;
        let mut request = read(&id)?;
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
        let asked = members.proxy_a.journal().len();
        let (status, text) = call(app.clone(), request).await?;
        if admitted {
            assert_eq!(StatusCode::OK, status, "{scope}: {text}");
        } else {
            assert_eq!(StatusCode::FORBIDDEN, status, "{scope}: {text}");
            assert_eq!(
                asked,
                members.proxy_a.journal().len(),
                "{scope}: nothing is sent"
            );
        }
    }
    Ok(())
}

/// The gateway over `members`, its issuer's patient tokens bound to node A
/// and the cross-reference knowing the token's patient at both members and
/// another patient at node A.
fn confined_gateway(members: &Members, dir: &Path) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (crate::fhir::receive::EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (crate::fhir::receive::EHR_SYSTEM, EHR_A, "node-b", EHR_B),
        (crate::fhir::receive::EHR_SYSTEM, OTHER_A, "node-a", OTHER_A),
    ]);
    let own = format!(
        "{}{}",
        own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
        own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
    );
    let text = text_registered(
        dir,
        (&members.a.uri(), &members.b.uri(), UNREACHED_SUPPLIER),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own,
        ("", &format!("{SERVED}{crossref}")),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, &bound()?))
}

#[tokio::test]
async fn a_caller_confined_to_one_patient_reads_that_patients_prescription_alone() -> TestResult {
    let members = answering().await?;
    let dir = tempfile::tempdir()?;
    let app = confined_gateway(&members, dir.path())?;
    let mut granted = claims();
    granted.scope = Some("patient/composition-*.r".to_owned());
    granted.ehr_id = Some(EHR_A.to_owned());
    let token = minted(&granted)?;
    let own = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let (status, text) = call(app.clone(), bearing(read(&own)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "the patient's own: {text}");
    let asked = members.proxy_a.journal().len();
    let forged = face_id(AT_A.0, OTHER_A, "rx-a1")?;
    let (status, text) = call(app, bearing(read(&forged)?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    assert!(text.contains("patient-confinement"), "{text}");
    assert_eq!(
        asked,
        members.proxy_a.journal().len(),
        "an id beyond the caller's patient is sent nowhere"
    );
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_read_is_recorded_once_under_electronic_prescriptions_naming_its_member() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = answering().await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        ("", &tables),
    )?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let (status, text) = call(app, read(&id)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    let [record] = records.as_slice() else {
        return Err(format!("one access record per request: {records:#?}").into());
    };
    assert!(
        record.contains("eehrxf-document-priority-category-cs|Electronic-Prescriptions"),
        "Art 14(1)(b): the category of a prescription read: {record}"
    );
    for named in ["node-a-mpd", EHR_A] {
        assert!(record.contains(named), "{named}: {record}");
    }
    for unnamed in ["node-b-mpd", "SYNTHETIC-RX", "Synthetic medicine"] {
        assert!(!record.contains(unnamed), "{unnamed}: {record}");
    }
    Ok(())
}
