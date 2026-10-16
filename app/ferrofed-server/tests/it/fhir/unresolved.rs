// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A patient the cross-reference could not answer for at any member is
//! never read as a patient no member holds (§11.1, §11.3, §11.4, N6, N37):
//! the patient summary, the document list and the prescription search fail
//! under all-or-nothing naming every member `not-resolved`, and name each
//! one under `partial`; a patient every member answers it does not hold
//! still gets the empty answer.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use super::list::{self, native_endpoint};
use super::prescriptions::{self, SERVED};
use super::{TestResult, UID_A, UID_B, node_holding, summary, supplier, text_registered};
use crate::facade::EHR_A;
use crate::support::call;

/// The `ehr_id` domain of node A at the PIX Manager, in the example arc.
const DOMAIN_A: &str = "urn:oid:2.999.10";

/// The `ehr_id` domain of node B at the PIX Manager.
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The bases no member's own endpoint is reached at, node A's and node
/// B's, for members that must stay unasked.
const UNASKED: [&str; 2] = ["http://127.0.0.1:9/a", "http://127.0.0.1:9/b"];

/// What the PIX Manager one member is resolved through answers.
#[derive(Debug, Clone, Copy)]
enum CrossReference {
    /// `500` to every request: it cannot answer for its member.
    Failing,
    /// `404` with `not-found` to every request: it does not know the
    /// patient at its member (ITI-83 §2:3.83.4.2.2.2).
    NotKnowing,
    /// The patient's `ehr_id` at node A, in node A's domain.
    ResolvingA,
}

impl CrossReference {
    /// The PIX Manager answering as `self` says.
    async fn start(self) -> Server {
        let response = match self {
            Self::Failing => ResponseTemplate::new(500),
            Self::NotKnowing => ResponseTemplate::new(404).set_body_raw(
                r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#,
                "application/fhir+json",
            ),
            Self::ResolvingA => ResponseTemplate::new(200).set_body_raw(
                format!(
                    r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}}]}}"#
                ),
                "application/fhir+json",
            ),
        };
        let server = Server::start().await;
        Mock::given(any())
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }
}

/// The three routes of the FHIR face that reach a member for one patient.
#[derive(Debug, Clone, Copy)]
enum Route {
    /// `GET {fhir-base}/Patient/$summary`, over the section queries.
    Summary,
    /// `GET {fhir-base}/DocumentReference`, over the members' own summary
    /// endpoints.
    List,
    /// `GET {fhir-base}/MedicationRequest`, over the members' own
    /// prescription endpoints.
    Prescriptions,
}

impl Route {
    /// The request of the route for the test patient.
    fn request(self) -> Result<Request<Body>, http::Error> {
        match self {
            Self::Summary => summary(""),
            Self::List => list::list(""),
            Self::Prescriptions => prescriptions::search(""),
        }
    }

    /// The registry entries and the tables the route is served with.
    fn served(self) -> (String, &'static str) {
        match self {
            Self::Summary => (String::new(), ""),
            Self::List => (
                format!(
                    "{}{}",
                    native_endpoint("node-a-eps", ("node-a", "org-a"), UNASKED[0]),
                    native_endpoint("node-b-eps", ("node-b", "org-b"), UNASKED[1])
                ),
                "",
            ),
            Self::Prescriptions => (
                format!(
                    "{}{}",
                    prescriptions::own_endpoint("node-a-mpd", ("node-a", "org-a"), UNASKED[0]),
                    prescriptions::own_endpoint("node-b-mpd", ("node-b", "org-b"), UNASKED[1])
                ),
                SERVED,
            ),
        }
    }
}

/// What `route` answers for the test patient while every member is
/// resolved through a PIX Manager answering as `crossref` says, under
/// all-or-nothing or, when `partial`, under `partial`: its status and its
/// text, and whether a section query reached a member's Query API.
async fn answered(
    route: Route,
    crossref: CrossReference,
    partial: bool,
) -> Result<(StatusCode, String, bool), Box<dyn Error>> {
    let (status, text, queried) = answered_by(route, [crossref, crossref], partial).await?;
    Ok((status, text, queried.0 || queried.1))
}

/// What `route` answers for the test patient while node A is resolved
/// through a PIX Manager answering as `a` says and node B through one
/// answering as `b` says, under all-or-nothing or, when `partial`, under
/// `partial`: its status and its text, and whether a section query reached
/// node A's and node B's Query API.
async fn answered_by(
    route: Route,
    [a_crossref, b_crossref]: [CrossReference; 2],
    partial: bool,
) -> Result<(StatusCode, String, (bool, bool)), Box<dyn Error>> {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let pdq = supplier().await?;
    let (pix_a, pix_b) = (a_crossref.start().await, b_crossref.start().await);
    let (members, served) = route.served();
    let resolvers = format!(
        "{served}\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\
         \"node-a\" = \"{DOMAIN_A}\"\n\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n\
         [pixm.manager.members]\n\"node-b\" = \"{DOMAIN_B}\"\n",
        pix_a.uri(),
        pix_b.uri()
    );
    let federation = if partial { "best_effort = true\n" } else { "" };
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (&a.uri(), &b.uri(), &pdq.base_url()),
        &[],
        &members,
        (federation, &resolvers),
    )?;
    let settings = ferrofed_server::config::Config::from_sources(
        Some(&crate::support::signed(&text)),
        &std::collections::BTreeMap::new(),
    )?
    .resolve()?;
    let state = std::sync::Arc::new(ferrofed_server::state::AppState::build(&settings)?);
    let app = ferrofed_server::router(state, &crate::facade::settings_with_room());
    let mut request = route.request()?;
    if partial {
        request
            .headers_mut()
            .insert("openehr-federation-completeness", "partial".parse()?);
    }
    let (status, text) = call(app, request).await?;
    let queried = (
        !crate::facade::received(&a).await?.is_empty(),
        !crate::facade::received(&b).await?.is_empty(),
    );
    Ok((status, text, queried))
}

/// Whether `text` names both members `not-resolved`.
fn names_both_unresolved(text: &str) -> bool {
    ["node-a-pub", "node-b-pub"]
        .iter()
        .all(|endpoint| text.contains(&format!("{endpoint} (not-resolved)")))
}

/// Asserts that `route` fails under all-or-nothing when the cross-reference
/// answers for no member, naming each.
async fn fails_naming_each(route: Route) -> TestResult {
    let (status, text, queried) = answered(route, CrossReference::Failing, false).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "{route:?}: §11.4, never the answer for an unknown patient: {text}"
    );
    assert!(names_both_unresolved(&text), "{route:?}: {text}");
    assert!(!queried, "{route:?}: no member is asked");
    Ok(())
}

/// Asserts that `route` answers under `partial` when the cross-reference
/// answers for no member, naming each: the summary in the narrative of
/// every section, by the organisation that holds the member's data, and a
/// list in an `outcome` entry per member, with no match.
async fn names_each_under_partial(route: Route) -> TestResult {
    let (status, text, queried) = answered(route, CrossReference::Failing, true).await?;
    assert_eq!(StatusCode::OK, status, "{route:?}: §11.4: {text}");
    assert!(!queried, "{route:?}: no member is asked");
    let bundle: Value = serde_json::from_str(&text)?;
    if let Route::Summary = route {
        let findings = eps::check(&text)?;
        assert!(findings.is_empty(), "the EPS profiles: {findings:#?}");
        let sections = bundle
            .pointer("/entry/0/resource/section")
            .and_then(Value::as_array)
            .ok_or("the sections")?;
        for section in sections {
            assert_eq!(
                section.pointer("/emptyReason/coding/0/code"),
                Some(&Value::from("unavailable")),
                "never nilknown: {section}"
            );
            let div = section
                .pointer("/text/div")
                .and_then(Value::as_str)
                .ok_or("a narrative")?;
            assert!(
                div.contains("org-a (not-resolved), org-b (not-resolved)"),
                "{div}"
            );
        }
        return Ok(());
    }
    assert!(names_both_unresolved(&text), "{route:?}: {bundle}");
    let entries = bundle
        .get("entry")
        .and_then(Value::as_array)
        .ok_or("entries")?;
    assert!(
        entries
            .iter()
            .all(|entry| entry.pointer("/search/mode") == Some(&Value::from("outcome"))),
        "{route:?}: no match, not even a summary the gateway would write: {bundle}"
    );
    assert_eq!(
        2,
        entries.len(),
        "{route:?}: one outcome per member: {bundle}"
    );
    Ok(())
}

/// Asserts that `route` answers as for a patient no member holds when every
/// member answers that it does not hold the patient.
async fn is_empty_for_an_unknown_patient(route: Route) -> TestResult {
    let (status, text, queried) = answered(route, CrossReference::NotKnowing, false).await?;
    assert!(!names_both_unresolved(&text), "{route:?}: {text}");
    assert!(!queried, "{route:?}: no member is asked");
    let bundle: Value = serde_json::from_str(&text)?;
    match route {
        Route::Summary => {
            assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
            assert_eq!(
                bundle.pointer("/issue/0/code"),
                Some(&Value::from("not-found")),
                "{bundle}"
            );
        }
        Route::List | Route::Prescriptions => {
            assert_eq!(StatusCode::OK, status, "{route:?}: §11.3: {text}");
            assert_eq!(
                bundle.get("type"),
                Some(&Value::from("searchset")),
                "{bundle}"
            );
            assert!(
                bundle
                    .get("entry")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty),
                "{route:?}: no entry: {bundle}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_summary_resolved_nowhere_fails_naming_each_member() -> TestResult {
    fails_naming_each(Route::Summary).await
}

#[tokio::test]
async fn a_list_resolved_nowhere_fails_naming_each_member() -> TestResult {
    fails_naming_each(Route::List).await
}

#[tokio::test]
async fn a_prescription_search_resolved_nowhere_fails_naming_each_member() -> TestResult {
    fails_naming_each(Route::Prescriptions).await
}

#[tokio::test]
async fn under_partial_a_summary_resolved_nowhere_names_each_member() -> TestResult {
    names_each_under_partial(Route::Summary).await
}

#[tokio::test]
async fn under_partial_a_list_resolved_nowhere_names_each_member() -> TestResult {
    names_each_under_partial(Route::List).await
}

#[tokio::test]
async fn under_partial_a_prescription_search_resolved_nowhere_names_each_member() -> TestResult {
    names_each_under_partial(Route::Prescriptions).await
}

#[tokio::test]
async fn a_summary_for_a_patient_no_member_holds_is_not_found() -> TestResult {
    is_empty_for_an_unknown_patient(Route::Summary).await
}

// conformance: CP-12
#[tokio::test]
async fn a_list_for_a_patient_no_member_holds_is_empty() -> TestResult {
    is_empty_for_an_unknown_patient(Route::List).await
}

// conformance: CP-12
#[tokio::test]
async fn a_prescription_search_for_a_patient_no_member_holds_is_empty() -> TestResult {
    is_empty_for_an_unknown_patient(Route::Prescriptions).await
}

#[tokio::test]
async fn under_partial_a_summary_names_the_member_it_could_not_resolve_in_every_section()
-> TestResult {
    let (status, text, (queried_a, queried_b)) = answered_by(
        Route::Summary,
        [CrossReference::ResolvingA, CrossReference::Failing],
        true,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "the EPS profiles: {findings:#?}");
    assert!(queried_a, "node A, which resolved, is asked: {text}");
    assert!(!queried_b, "node B, which did not, is never asked");
    let document: Value = serde_json::from_str(&text)?;
    let sections = document
        .pointer("/entry/0/resource/section")
        .and_then(Value::as_array)
        .ok_or("the sections")?;
    for section in sections {
        if let Some(reason) = section.pointer("/emptyReason/coding/0/code") {
            assert_eq!(
                &Value::from("unavailable"),
                reason,
                "never nilknown while a member is unresolved: {section}"
            );
        }
        let div = section
            .pointer("/text/div")
            .and_then(Value::as_str)
            .ok_or("a narrative")?;
        assert!(
            div.contains("No answer from: org-b (not-resolved)"),
            "{div}"
        );
    }
    Ok(())
}
