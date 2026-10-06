// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document received on the FHIR face, written as one composition to the
//! member the deployment declares for its category (Regulation (EU) 2025/327
//! Annex II 2.2 and 2.3; Federation Tier §2.3, §12.4, N23): the member's own
//! `ehr_id` and no patient identifier in the path, the query string or the
//! headers (§5.4.1, N33, CP-26), no other member asked, and every refusal
//! answered before anything reaches a member.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::PatientBinding;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::state::AppState;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{FHIR, PUBLIC, TestResult, UNREACHED_SUPPLIER, fhir_tables_with};
use crate::auth::{bearing, claims, minted};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, registry, settings_with_room,
};
use crate::support::call;

/// The version node A creates for the composition.
const VERSION_A: &str = "7c4e1d20-0000-4000-8000-0000000008a1::cdr-a.example.org::1";

/// A second synthetic patient, and its `ehr_id` at node A.
const OTHER: &str = "SENTINEL-OTHER-71xw";
const OTHER_A: &str = "4444dddd-4444-4444-8444-444444444444";

/// A second identifier system of the example OID arc, which names
/// [`OTHER`].
const OTHER_NAMESPACE: &str = "urn:oid:2.999.2";

/// The identifier system of node A's `ehr_id`s at the cross-reference.
const EHR_SYSTEM: &str = "urn:oid:2.999.9.1";

/// The synthetic substance the document records.
const SUBSTANCE: &str = "Synthetic substance one";

/// The `[[dev.crossref]]` rows, each `(namespace, value, member, ehr_id)`.
fn rows(rows: &[(&str, &str, &str, &str)]) -> String {
    rows.iter()
        .fold(String::new(), |mut text, (namespace, value, member, ehr_id)| {
            // NOTE: writing to a String cannot fail, so the result is dropped.
            let _written: std::fmt::Result = write!(
                text,
                "\n[[dev.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
            );
            text
        })
}

/// The `[fhir]` tables of a face that receives patient summaries at
/// `member`, resolving their patient in `namespaces`.
fn receiving(member: &str, namespaces: &[&str]) -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string());
    let [model, context] = eps::mapping_files();
    let namespaces = namespaces
        .iter()
        .map(|namespace| format!("\"{namespace}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fhir_tables_with(
        UNREACHED_SUPPLIER,
        &format!("receive_namespaces = [{namespaces}]"),
        &format!(
            "\n[[fhir.receive]]\ncategory = \"Patient-Summaries\"\nmember = \"{member}\"\n\
             package = {}\ntemplate = {}\nfiles = [{}, {}]\ncontext = \"{}\"\n\
             language = \"en\"\nterritory = \"NL\"\n",
            path(&eps::package()),
            path(&eps::opt()),
            path(&model),
            path(&context),
            eps::CONTEXT
        ),
    )
}

/// The configuration text of a development gateway over node A at `a` and
/// node B at `b`, the cross-reference `crossref` and the face `fhir`.
fn text(
    dir: &Path,
    (a, b): (&str, &str),
    crossref: &str,
    fhir: &str,
) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[server]\npublic_url = \"{PUBLIC}\"\n\n\
         [registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\
         {crossref}{fhir}"
    ))
}

/// The gateway of [`text`], driven with `server`.
fn gateway_with(
    dir: &Path,
    nodes: (&Server, &Server),
    (crossref, fhir): (&str, &str),
    server: &ServerSettings,
) -> Result<Router, Box<dyn Error>> {
    let text = text(dir, (&nodes.0.uri(), &nodes.1.uri()), crossref, fhir)?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, server))
}

/// The gateway receiving patient summaries at node A, the patient known at
/// both nodes.
fn gateway(dir: &Path, a: &Server, b: &Server) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &receiving("node-a", &[NAMESPACE])),
        &settings_with_room(),
    )
}

/// A member that creates every composition as [`VERSION_A`].
async fn creating() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", format!("\"{VERSION_A}\"").as_str())
                .insert_header(
                    "Location",
                    format!("https://cdr-a.example.org/v1/ehr/{EHR_A}/composition/{VERSION_A}")
                        .as_str(),
                ),
        )
        .mount(&server)
        .await;
    server
}

/// `POST {fhir-base}/Bundle` carrying `document`.
fn post(document: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("{FHIR}/Bundle"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(document.to_owned()))
}

/// The patient summary of [`PATIENT`].
fn document() -> String {
    eps::received_document(&[(NAMESPACE, PATIENT)], SUBSTANCE)
}

/// The issue type and the diagnostics of the first issue of the
/// `OperationOutcome` `text` holds.
fn issue(text: &str) -> Result<(String, String), Box<dyn Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    if outcome.get("resourceType") != Some(&Value::from("OperationOutcome")) {
        return Err(format!("no OperationOutcome: {text}").into());
    }
    let read = |pointer: &str| {
        outcome
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok((read("/issue/0/code"), read("/issue/0/diagnostics")))
}

/// Every request target and header `server` received, the bodies left out.
async fn composed(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut text = String::new();
    for request in requests {
        text.push_str(request.url.as_str());
        for (name, value) in &request.headers {
            text.push_str(name.as_str());
            text.push_str(value.to_str().unwrap_or("<opaque>"));
        }
    }
    Ok(text)
}

/// Whether neither node received anything.
async fn untouched(a: &Server, b: &Server) -> Result<bool, Box<dyn Error>> {
    Ok(received(a).await?.is_empty() && received(b).await?.is_empty())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_document_is_written_to_its_member_by_its_ehr_id_alone() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("informational", code);
    for named in ["node-a", "node-a-pub", VERSION_A, "Patient-Summaries"] {
        assert!(diagnostics.contains(named), "{named}: {diagnostics}");
    }
    let requests = a.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "one write");
    let write = requests.first().ok_or("the write")?;
    assert_eq!(write.url.path(), format!("/v1/ehr/{EHR_A}/composition"));
    assert_eq!(write.url.query(), None, "N33: no query string");
    let wire = composed(&a).await?;
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !wire.contains(withheld),
            "N33: no identifier in the path or the headers: {wire}"
        );
    }
    let body = String::from_utf8(write.body.clone())?;
    assert!(body.contains(SUBSTANCE), "the mapped content: {body}");
    assert!(
        body.contains("original_content") && body.contains(PATIENT),
        "the received document is kept whole in the composition: {body}"
    );
    assert!(
        received(&b).await?.is_empty(),
        "N23: no other member is asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_read_scope_does_not_admit_a_write() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    for scope in [
        "user/composition-*.r user/aql-*.s",
        "user/composition-*.rus",
        "user/composition-ferrofed.eehrxf.allergy.v1.c",
    ] {
        let mut granted = claims();
        granted.scope = Some(scope.to_owned());
        let request = bearing(post(&document())?, &minted(&granted)?)?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{scope}: {text}");
        assert_eq!("forbidden", issue(&text)?.0, "{scope}");
    }
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_category_no_member_receives_is_refused_naming_it() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir_tables_with(UNREACHED_SUPPLIER, "", "")),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-supported", code);
    assert!(diagnostics.contains("Patient-Summaries"), "{diagnostics}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_patient_the_member_holds_no_ehr_for_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-b", EHR_B)]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &receiving("node-a", &[NAMESPACE])),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-found", code);
    assert!(diagnostics.contains("node-a"), "{diagnostics}");
    assert!(!text.contains(PATIENT_TAIL), "§5.4.3: never echoed: {text}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn identifiers_that_name_two_patients_are_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
    ]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref,
            &receiving("node-a", &[NAMESPACE, OTHER_NAMESPACE]),
        ),
        &settings_with_room(),
    )?;
    let mixed =
        eps::received_document(&[(NAMESPACE, PATIENT), (OTHER_NAMESPACE, OTHER)], SUBSTANCE);
    let (status, text) = call(app.clone(), post(&mixed)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    let unknown_beside = eps::received_document(
        &[
            (NAMESPACE, PATIENT),
            (OTHER_NAMESPACE, "SENTINEL-NONE-55mm"),
        ],
        SUBSTANCE,
    );
    let (status, text) = call(app, post(&unknown_beside)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("business-rule", issue(&text)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_patient_named_in_no_resolved_namespace_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let elsewhere = eps::received_document(&[("urn:oid:2.999.7", PATIENT)], SUBSTANCE);
    let (status, text) = call(app, post(&elsewhere)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!("required", issue(&text)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

/// The suite's server settings, its issuer's patient tokens bound to node
/// A, whose `ehr_id`s the cross-reference knows under [`EHR_SYSTEM`].
fn bound() -> Result<ServerSettings, Box<dyn Error>> {
    let mut server = settings_with_room();
    for issuer in &mut server.auth.issuers {
        issuer.patient = Some(PatientBinding {
            endpoint: EndpointId::new("node-a-pub")?,
            ehr_id_system: IdentifierNamespace::new(EHR_SYSTEM)?,
        });
    }
    Ok(server)
}

#[tokio::test]
async fn a_caller_confined_to_one_patient_writes_only_into_its_ehr() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
    ]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref,
            &receiving("node-a", &[NAMESPACE, OTHER_NAMESPACE]),
        ),
        &bound()?,
    )?;
    let mut granted = claims();
    granted.scope = Some(String::from("patient/composition-*.c"));
    granted.ehr_id = Some(EHR_A.to_owned());
    let token = minted(&granted)?;
    let other = eps::received_document(&[(OTHER_NAMESPACE, OTHER)], SUBSTANCE);
    let (status, text) = call(app.clone(), bearing(post(&other)?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    assert!(
        untouched(&a, &b).await?,
        "nothing is written for another patient"
    );
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "the token's own patient: {text}");
    assert_eq!(1, received(&a).await?.len());
    Ok(())
}

#[tokio::test]
async fn a_member_that_refuses_the_composition_is_named_with_its_status() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            br#"{"message":"synthetic validation failure"}"#.to_vec(),
            "application/json",
        ))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(
        StatusCode::BAD_GATEWAY,
        status,
        "§11: never a success: {text}"
    );
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("processing", code);
    assert!(
        diagnostics.contains("422") && diagnostics.contains("node-a-pub"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_that_names_no_version_is_no_success() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(201))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "§11: {text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("exception", code);
    assert!(diagnostics.contains("node-a-pub"), "{diagnostics}");
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_answer_is_a_time_out() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(201).set_delay(std::time::Duration::from_secs(5)))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "{text}");
    assert_eq!("timeout", issue(&text)?.0);
    Ok(())
}

#[tokio::test]
async fn a_document_the_component_does_not_read_is_refused_before_any_member() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let collection = document().replacen("\"document\"", "\"collection\"", 1);
    let lab = document().replacen("60591-5", "11502-2", 1);
    let sectionless = document().replacen("\"47519-4\"", "\"00000-0\"", 1);
    for (text, status, code) in [
        ("not json", StatusCode::BAD_REQUEST, "invalid"),
        (collection.as_str(), StatusCode::BAD_REQUEST, "invalid"),
        (
            lab.as_str(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-supported",
        ),
        (
            sectionless.as_str(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "required",
        ),
    ] {
        let (answered, body) = call(app.clone(), post(text)?).await?;
        assert_eq!(status, answered, "{body}");
        assert_eq!(code, issue(&body)?.0, "{body}");
    }
    let (status, text) = call(
        app,
        Request::post(format!("{FHIR}/Bundle"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(document()))?,
    )
    .await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{text}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_variant_of_the_bundle_path_is_refused_at_the_gate() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    for path in [
        format!("{FHIR}/bundle"),
        format!("{FHIR}/Bundle/"),
        format!("{FHIR}//Bundle"),
        format!("{FHIR}/%42undle"),
        format!("{FHIR}/Bundle/synthetic"),
    ] {
        let request = Request::post(&path)
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(document()))?;
        let (status, text) = call(app.clone(), request).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{path}: {text}");
    }
    let (status, text) = call(
        app,
        Request::get(format!("{FHIR}/Bundle")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "a read of the end-point: {text}"
    );
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[test]
fn a_member_the_registry_does_not_hold_refuses_the_start() -> TestResult {
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org"),
        &crossref,
        &receiving("node-z", &[NAMESPACE]),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let loaded = Federation::load(&settings);
    assert!(
        matches!(
            loaded,
            Err(FederationError::ReceivingMemberUnknown {
                category: "Patient-Summaries",
                ref member,
            }) if member == "node-z"
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn receiving_with_no_resolver_refuses_the_start() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org"),
        "",
        &receiving("node-a", &[NAMESPACE]),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let loaded = Federation::load(&settings);
    assert!(
        matches!(loaded, Err(FederationError::ReceivingWithoutResolver)),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn receiving_with_no_namespace_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org"),
        &crossref,
        &receiving("node-a", &[]),
    )?;
    let resolved =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve();
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(
                ferrofed_server::config::fhir::FhirError::Missing {
                    key: "fhir.receive_namespaces"
                }
            ))
        ),
        "{resolved:?}"
    );
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_receipt_is_recorded_once_as_a_creation_of_its_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!(
        "{}{}",
        receiving("node-a", &[NAMESPACE]),
        audit_tables(&repository, "")
    );
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
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
        "one access record per receipt: {records:#?}"
    );
    let record = records.first().ok_or("a record")?;
    for expected in [
        "eehrxf-document-priority-category-cs|Patient-Summaries:construction",
        "node-a-pub",
        EHR_A,
        VERSION_A,
        "create Bundle",
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}
