// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a receipt on the FHIR face answers beyond the write itself
//! (Regulation (EU) 2025/327 Annex II 2.2, 2.3 and 3.2, Art 38): every
//! refusal an `OperationOutcome`, one access record for every receipt that
//! reached its member and none for one that did not, a receipt whose record
//! cannot be stored refused with none of its answer, and the categories the
//! deployment receives named in its capability statement.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::Value;
#[cfg(feature = "binding-ihe")]
use wiremock::matchers::{any, method, path};
#[cfg(feature = "binding-ihe")]
use wiremock::{Mock, ResponseTemplate};

use super::receive::{
    EHR_SYSTEM, OTHER, OTHER_A, OTHER_NAMESPACE, SUBSTANCE, bound, creating, document, gateway,
    gateway_with, issue, post, receiving, rows, untouched,
};
use super::{FHIR, PUBLIC, TestResult, UNREACHED_SUPPLIER, fhir_tables_with};
use crate::auth::{bearing, claims, minted};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, registry, settings_with_room};
use crate::support::{call, send_as_is, signed};

#[tokio::test]
async fn a_body_that_is_not_utf8_is_refused_with_an_outcome() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let request = Request::post(format!("{FHIR}/Bundle"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(vec![0xff_u8, 0xfe, b'{', b'}']))?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("invalid", issue(&text)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_receipt_with_no_credential_is_refused_with_an_outcome() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let response = send_as_is(app, post(&document())?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    assert_eq!("login", issue(std::str::from_utf8(&body)?)?.0);
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn a_write_beyond_a_confined_grant_is_refused_with_an_outcome() -> TestResult {
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
    let other = eps::received_document(&[(OTHER_NAMESPACE, OTHER)], SUBSTANCE);
    let (status, text) = call(app, bearing(post(&other)?, &minted(&granted)?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("forbidden", code);
    assert!(!diagnostics.contains(OTHER), "§5.4.3: {diagnostics}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

/// Node C, whose one endpoint the operator has taken out of service.
const SUSPENDED_C: &str = r#"
[[organisation]]
id = "org-c"

[[node]]
id = "node-c"
organisation = "org-c"
system_id = "cdr-c.example.org"

[[endpoint]]
id = "node-c-pub"
node = "node-c"
url = "http://127.0.0.1:9"
connection_type = "openehr-rest-query"
managing_organisation = "org-c"
status = "suspended"
"#;

#[tokio::test]
async fn a_member_with_no_endpoint_in_service_is_refused_with_nothing_sent() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let listed = dir.path().join("registry.toml");
    std::fs::write(&listed, registry(&a.uri(), &b.uri(), SUSPENDED_C))?;
    let text = format!(
        "profile = \"development\"\n\n[server]\npublic_url = \"{PUBLIC}\"\n\n\
         [registry]\ndocument = {}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\n{}{}",
        toml::Value::String(listed.display().to_string()),
        rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]),
        receiving("node-c", &[NAMESPACE])
    );
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &settings_with_room());
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("transient", code);
    assert_eq!(
        "member node-c of the category Patient-Summaries has no endpoint it is reached through",
        diagnostics
    );
    assert!(untouched(&a, &b).await?, "N23: no member is asked");
    Ok(())
}

/// The `[pixm]` table of one PIX Manager at `pix` serving node A and node
/// B.
#[cfg(feature = "binding-ihe")]
fn pixm(pix: &str) -> String {
    format!(
        "\n[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.31\"\n\"node-b\" = \"urn:oid:2.999.32\"\n"
    )
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_patient_the_identity_service_cannot_resolve_is_a_failed_dependency() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let pix = Server::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&pix)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&pixm(&pix.uri()), &receiving("node-a", &[NAMESPACE])),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11: {text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("transient", code);
    assert!(diagnostics.contains("node-a"), "{diagnostics}");
    assert!(
        !pix.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "the PIX Manager was asked"
    );
    assert!(untouched(&a, &b).await?, "nothing is written");
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_names_the_categories_received() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let metadata = || Request::get(format!("{FHIR}/metadata")).body(Body::empty());
    let bundle = |text: &str| -> Result<Value, Box<dyn Error>> {
        let statement: Value = serde_json::from_str(text)?;
        let resources = statement
            .pointer("/rest/0/resource")
            .and_then(Value::as_array)
            .ok_or("the resources")?;
        let bundle = resources
            .iter()
            .find(|resource| resource["type"] == "Bundle")
            .ok_or("the Bundle resource")?;
        Ok(bundle.clone())
    };
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let received = bundle(&text)?;
    assert_eq!(received["interaction"][0]["code"], "create");
    assert_eq!(
        received["supportedProfile"][0],
        "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps"
    );
    let documentation = received["documentation"].as_str().ok_or("documentation")?;
    assert!(
        documentation.contains("Categories received: Patient-Summaries."),
        "{documentation}"
    );
    assert!(
        !documentation.contains("node-a"),
        "no member is named: {documentation}"
    );
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir_tables_with(UNREACHED_SUPPLIER, "", "")),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let unreceived = bundle(&text)?;
    assert_eq!(unreceived["supportedProfile"], Value::Null);
    let documentation = unreceived["documentation"]
        .as_str()
        .ok_or("documentation")?;
    assert!(
        documentation.contains(
            "Categories received: none. Categories with no receiving member: Patient-Summaries"
        ),
        "{documentation}"
    );
    Ok(())
}

#[cfg(feature = "binding-ihe")]
mod recorded {
    //! The access record of a receipt, at the harness Audit Record
    //! Repository (Annex II 3.2).

    use std::time::Duration;

    use ferrofed_testkit::atna_feed::FeedRepository;
    use ferrofed_testkit::mock::Server;
    use http::StatusCode;

    use super::super::TestResult;
    use super::super::receive::{
        VERSION_A, creating, document, gateway_with, post, receiving, rows,
    };
    use super::{issue, refusing};
    use crate::facade::{EHR_A, NAMESPACE, PATIENT, settings_with_room};
    use crate::feed_audit::{SETTLE, audit_tables};
    use crate::support::call;

    /// How long a test waits for a record it expects none of.
    const QUIET: Duration = Duration::from_secs(2);

    /// The gateway over `a` and `b` receiving patient summaries at node A
    /// and recording to `repository` with the `[audit.repository]` keys
    /// `extra`.
    fn recording(
        dir: &std::path::Path,
        (a, b): (&Server, &Server),
        repository: &FeedRepository,
        extra: &str,
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
        let fhir = format!(
            "{}{}",
            receiving("node-a", &[NAMESPACE]),
            audit_tables(repository, extra)
        );
        gateway_with(dir, (a, b), (&crossref, &fhir), &settings_with_room())
    }

    /// The access records among `records`.
    fn accesses(records: Vec<String>) -> Vec<String> {
        records
            .into_iter()
            .filter(|record| record.contains("ehds-categories"))
            .collect()
    }

    #[tokio::test]
    async fn a_receipt_the_member_refused_is_recorded_with_its_category_and_member() -> TestResult {
        let (a, b) = (refusing().await, Server::start().await);
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = recording(dir.path(), (&a, &b), &repository, "")?;
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::BAD_GATEWAY, status, "{text}");
        let records = accesses(repository.wait_for(1, SETTLE).await);
        assert_eq!(1, records.len(), "one record per receipt: {records:#?}");
        let record = records.first().ok_or("a record")?;
        for expected in [
            "eehrxf-document-priority-category-cs|Patient-Summaries:construction",
            "node-a-pub",
            EHR_A,
            "create Bundle",
        ] {
            assert!(record.contains(expected), "{expected}: {record}");
        }
        assert!(!record.contains(VERSION_A), "no version was created");
        Ok(())
    }

    #[tokio::test]
    async fn a_receipt_refused_before_any_member_writes_no_record() -> TestResult {
        let (a, b) = (creating().await, Server::start().await);
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = recording(dir.path(), (&a, &b), &repository, "")?;
        let lab = document().replacen("60591-5", "11502-2", 1);
        let (status, text) = call(app.clone(), post(&lab)?).await?;
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        let records = accesses(repository.wait_for(2, QUIET).await);
        assert_eq!(
            1,
            records.len(),
            "only the receipt that reached its member: {records:#?}"
        );
        let record = records.first().ok_or("a record")?;
        assert!(record.contains(VERSION_A), "{record}");
        Ok(())
    }

    #[tokio::test]
    async fn a_receipt_whose_record_cannot_be_stored_is_refused() -> TestResult {
        let (a, b) = (creating().await, Server::start().await);
        let repository = FeedRepository::start().await;
        repository.set_up(false);
        let dir = tempfile::tempdir()?;
        let app = recording(dir.path(), (&a, &b), &repository, "spool_max_events = 1")?;
        let (status, text) = call(app.clone(), post(&document())?).await?;
        assert_eq!(
            StatusCode::OK,
            status,
            "the first record fills the spool: {text}"
        );
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
        let (code, diagnostics) = issue(&text)?;
        assert_eq!("transient", code);
        assert!(diagnostics.contains("access-unrecorded"), "{diagnostics}");
        assert!(
            !text.contains(VERSION_A) && !text.contains("node-a"),
            "none of the answer leaves without its record: {text}"
        );
        Ok(())
    }

    /// The compositions member `a` was asked to create.
    async fn writes_at(a: &Server) -> Result<usize, Box<dyn std::error::Error>> {
        let target = format!("/v1/ehr/{EHR_A}/composition");
        Ok(a.received_requests()
            .await
            .ok_or("recording is on")?
            .iter()
            .filter(|request| request.method == http::Method::POST && request.url.path() == target)
            .count())
    }

    #[tokio::test]
    async fn a_receipt_whose_record_has_no_place_never_reaches_its_member() -> TestResult {
        let (a, b) = (creating().await, Server::start().await);
        let repository = FeedRepository::start().await;
        repository.set_up(false);
        let dir = tempfile::tempdir()?;
        let app = recording(dir.path(), (&a, &b), &repository, "spool_max_events = 1")?;
        let (status, text) = call(app.clone(), post(&document())?).await?;
        assert_eq!(
            StatusCode::OK,
            status,
            "the first record fills the spool: {text}"
        );
        let (status, text) = call(app, post(&document())?).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
        assert!(issue(&text)?.1.contains("access-unrecorded"), "{text}");
        assert_eq!(
            1,
            writes_at(&a).await?,
            "the refused receipt is never asked of the member, so a retry cannot write it twice"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_receipts_against_one_free_place_write_once() -> TestResult {
        let (a, b) = (creating().await, Server::start().await);
        let repository = FeedRepository::start().await;
        repository.set_up(false);
        let dir = tempfile::tempdir()?;
        let app = recording(dir.path(), (&a, &b), &repository, "spool_max_events = 1")?;
        let (first, second) = tokio::join!(
            call(app.clone(), post(&document())?),
            call(app, post(&document())?)
        );
        let mut statuses = [first?.0, second?.0];
        statuses.sort_unstable();
        assert_eq!([StatusCode::OK, StatusCode::SERVICE_UNAVAILABLE], statuses);
        assert_eq!(
            1,
            writes_at(&a).await?,
            "only the receipt that took the place"
        );
        Ok(())
    }
}

/// A member that refuses every composition `422`.
#[cfg(feature = "binding-ihe")]
async fn refusing() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            br#"{"message":"synthetic validation failure"}"#.to_vec(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}
