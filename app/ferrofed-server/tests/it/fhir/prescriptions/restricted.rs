// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the search discloses and to whom: an unknown patient and a
//! restricted one answer alike (§11.3, Regulation (EU) 2025/327 Art 8), the
//! search takes the `composition-` search scope, a malformed search is
//! refused before any member is asked, the identifier never reaches the log,
//! and each search is recorded once (Annex II 3.2).

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::{
    Members, RX_A, RX_B, SERVED, built, face_id, own, own_endpoint, page, search, searchset,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL};
use crate::fhir::{FHIR, TestResult};
use crate::support::{call, claims, issuer};

/// A `[[testkit.crossref]]` row knowing another patient at node A.
fn another_patient() -> String {
    format!(
        "\n[[testkit.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"SYNTHETIC-OTHER-4q\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    )
}

/// A `[[testkit.consent_denied]]` row denying node A for the test patient.
fn denied_at_a() -> String {
    format!(
        "\n[[testkit.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-a\"\n"
    )
}

#[tokio::test]
async fn an_unknown_patient_and_a_restricted_one_answer_alike_and_nothing_is_sent() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let unknown = members.gateway(dir.path(), &[], ("", &another_patient()))?;
    let restricted = members.gateway(dir.path(), &[("node-a", EHR_A)], ("", &denied_at_a()))?;
    let (unknown_status, unknown_text) = call(unknown, search("")?).await?;
    let (restricted_status, restricted_text) = call(restricted, search("")?).await?;
    assert_eq!(
        StatusCode::OK,
        unknown_status,
        "FHIR R4 search §3.1.1.3: {unknown_text}"
    );
    assert_eq!(
        (
            unknown_status,
            serde_json::from_str::<Value>(&unknown_text)?
        ),
        (
            restricted_status,
            serde_json::from_str::<Value>(&restricted_text)?
        ),
        "§11.3, Art 8: a restricted patient answers as an unknown one"
    );
    let bundle: Value = serde_json::from_str(&unknown_text)?;
    assert_eq!(bundle["total"], Value::from(0));
    assert!(bundle.get("entry").is_none(), "{bundle}");
    assert!(members.unasked(), "no member is asked");
    assert!(members.no_query().await?);
    Ok(())
}

/// The `[access_log]` table naming the namespace the patient behind an
/// `ehr_id` is named in, which the read asks the pre-filter about.
fn patient_namespaces() -> String {
    format!("\n[access_log]\npatient_namespaces = [\"{NAMESPACE}\"]\n")
}

/// `GET {fhir-base}/MedicationRequest/{id}`.
fn read(id: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())
}

/// §14 Step 1, N27a; Regulation (EU) 2025/327 Art 8, Art 11(5): a read by
/// the id of a prescription at a member the pre-filter denies for the
/// patient behind its `ehr_id` answers as an id with no owner, and the
/// member is asked nothing, as the search leaves that member out.
#[tokio::test]
async fn a_read_at_a_member_the_prefilter_denies_answers_as_an_unknown_id_and_asks_nothing()
-> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let tables = format!("{}{}", denied_at_a(), patient_namespaces());
    let restricted = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        ("", &tables),
    )?;
    let denied = face_id("node-a-mpd", EHR_A, "rx-a1")?;
    let (denied_status, denied_text) = call(restricted.clone(), read(&denied)?).await?;
    let (unknown_status, unknown_text) =
        call(restricted, read("not-an-id-the-face-wrote")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, denied_status, "{denied_text}");
    assert_eq!(
        (
            unknown_status,
            serde_json::from_str::<Value>(&unknown_text)?
        ),
        (denied_status, serde_json::from_str::<Value>(&denied_text)?),
        "Art 8: a denied member's prescription reads as an id with no owner"
    );
    for trace in ["consent", "restrict", "denied"] {
        assert!(
            !denied_text.to_ascii_lowercase().contains(trace),
            "{denied_text}"
        );
    }
    assert!(members.unasked(), "the denied member is asked nothing");
    Ok(())
}

/// The same read where the pre-filter denies another member reaches the
/// member the id names.
#[tokio::test]
async fn a_read_at_a_member_the_prefilter_admits_is_answered() -> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let tables = format!("{}{}", denied_at_a(), patient_namespaces());
    let app = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        ("", &tables),
    )?;
    let admitted = face_id("node-b-mpd", EHR_B, "rx-b1")?;
    let (status, text) = call(app, read(&admitted)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!members.proxy_b.journal().is_empty(), "node B is asked");
    assert!(members.proxy_a.journal().is_empty(), "node A is not");
    Ok(())
}

#[tokio::test]
async fn a_caller_without_the_composition_search_scope_is_forbidden_and_nothing_is_sent()
-> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    for (scope, admitted) in [
        ("user/aql-*.s user/composition-org.example.rx.v1.s", false),
        ("user/composition-*.r", false),
        ("user/composition-*.s", true),
    ] {
        let mut claims = claims();
        claims.scope = Some(scope.to_owned());
        let token = issuer().mint(&claims)?;
        let mut request = search("")?;
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
        let (status, text) = call(app.clone(), request).await?;
        if admitted {
            assert_eq!(StatusCode::OK, status, "{scope}: {text}");
        } else {
            assert_eq!(StatusCode::FORBIDDEN, status, "{scope}: {text}");
            assert!(members.unasked(), "{scope}: nothing is sent");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_search_naming_no_patient_or_another_parameter_is_refused_and_nothing_is_sent()
-> TestResult {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    for query in [
        String::from("_count=1"),
        format!("patient.identifier={PATIENT}"),
        format!("patient.identifier=urn%3Aoid%3A2.999.1%7C{PATIENT}&status=active"),
        format!("patient.identifier=urn%3Aoid%3A2.999.1%7C{PATIENT}&_count=x"),
    ] {
        let request =
            Request::get(format!("{FHIR}/MedicationRequest?{query}")).body(Body::empty())?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        assert!(text.contains("OperationOutcome"), "{text}");
        assert!(!text.contains(PATIENT_TAIL), "{text}");
    }
    assert!(members.unasked());
    Ok(())
}

#[test]
fn the_patient_identifier_never_reaches_the_log() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let dir = tempfile::tempdir()?;
    let (app, _servers) = runtime.block_on(async {
        let (own_a, own_b) = (
            own(page(searchset("https://cdr-a.example.org", &[RX_A], None))).await,
            own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await,
        );
        let (a, b) = (Server::start().await, Server::start().await);
        let members = format!(
            "{}{}",
            own_endpoint("node-a-mpd", ("node-a", "org-a"), &own_a.uri()),
            own_endpoint("node-b-mpd", ("node-b", "org-b"), &own_b.uri())
        );
        let app = built(
            dir.path(),
            (&a.uri(), &b.uri()),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
            &members,
            ("", SERVED),
        )?;
        Ok::<_, Box<dyn Error>>((app, [own_a, own_b, a, b]))
    })?;
    let text = crate::request_log::logged(&app, "trace", vec![search("")?, search("&_count=x")?])?;
    let lines = crate::support::request_lines(&text)?;
    assert_eq!(2, lines.len(), "both requests were logged: {text}");
    for needle in [PATIENT, PATIENT_TAIL, "SYNTHETIC-RX", "Synthetic medicine"] {
        assert!(!text.contains(needle), "{needle} reached the log: {text}");
    }
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_search_is_recorded_once_under_electronic_prescriptions_naming_each_member() -> TestResult
{
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[RX_A], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let tables = audit_tables(&repository, "");
    let app = members.gateway(
        dir.path(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        ("", &tables),
    )?;
    let (status, text) = call(app, search("")?).await?;
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
        "Art 14(1)(b): the category of a prescription search: {record}"
    );
    for named in ["node-a-mpd", "node-b-mpd", EHR_A, EHR_B] {
        assert!(record.contains(named), "{named}: {record}");
    }
    for unnamed in ["SYNTHETIC-RX", "Synthetic medicine"] {
        assert!(!record.contains(unnamed), "no clinical content: {record}");
    }
    let mut event: Value = serde_json::from_str(record)?;
    let entities = event["entity"].as_array_mut().ok_or("entities")?;
    let before = entities.len();
    entities.retain(|entity| {
        entity.get("query").is_none()
            && entity.pointer("/what/identifier/value") != Some(&Value::from(PATIENT))
    });
    assert!(
        entities.len() < before,
        "the record names its data subject: {record}"
    );
    let rest = event.to_string();
    assert!(
        !rest.contains(PATIENT_TAIL),
        "Annex II 3.2: the patient is named as the data subject and in the query alone: {rest}"
    );
    Ok(())
}
