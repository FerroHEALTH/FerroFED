// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members that serve electronic prescriptions and dispensations
//! (Regulation (EU) 2025/327 Art 14(1)(b) and (c)): a deployment that
//! declares none answers every route of the two categories as not served,
//! never with an empty list, and asks no member; the capability statement
//! lists `MedicationRequest` and `MedicationDispense` only for a category a
//! member is declared for; `config check` names the members; and a member
//! declared without its own endpoint in the HL7 Europe MPD profiles, or
//! unknown to the registry, refuses the start.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_eehrxf::serving::ServingError;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_server::config::Config;
use ferrofed_server::config::fhir::FhirError;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::fhir::locator::Locator;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::receive::{issue, untouched};
use super::{FHIR, TestResult, UNREACHED_SUPPLIER, gateway_registered, text_registered};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT};
use crate::run::binary;
use crate::support::call;

/// The `[[fhir.serve]]` tables declaring node A for both categories.
const SERVED: &str = "\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\"node-a\"]\n\n\
     [[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [\"node-a\"]\n";

/// Node A's own endpoint at `base`, serving `categories` in `profiles`.
fn own_endpoint(base: &str, categories: &str, profiles: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"node-a-mpd\"\nnode = \"node-a\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [{categories}]\nprofiles = [{profiles}]\nehr_id_system = \"urn:oid:2.999.4.1\"\n"
    )
}

/// Node A's own endpoint at `base`, serving both categories in their MPD
/// profiles.
fn mpd_endpoint(base: &str) -> String {
    own_endpoint(
        base,
        "\"Electronic-Prescriptions\", \"Electronic-Dispensations\"",
        &format!("\"{MPD_MEDICATION_REQUEST}\", \"{MPD_MEDICATION_DISPENSE}\""),
    )
}

/// The search of the test patient's prescriptions.
fn search() -> Result<Request<Body>, http::Error> {
    let identifier = format!("{}%7C{PATIENT}", NAMESPACE.replace(':', "%3A"));
    Request::get(format!(
        "{FHIR}/MedicationRequest?patient.identifier={identifier}"
    ))
    .body(Body::empty())
}

/// The read of node A's prescription `synthetic-rx-1` by the id the face
/// names it by.
fn read() -> Result<Request<Body>, Box<dyn Error>> {
    let id = Locator::new(
        EndpointId::new("node-a-mpd")?,
        EhrId::new(EHR_A)?,
        "synthetic-rx-1",
    )
    .id()
    .ok_or("the parts fit an id")?;
    Ok(Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())?)
}

/// The id the face names node A's dispensation `md-1` by.
fn dispensation_id() -> Result<String, Box<dyn Error>> {
    Ok(
        Locator::new(EndpointId::new("node-a-mpd")?, EhrId::new(EHR_A)?, "md-1")
            .id()
            .ok_or("the parts fit an id")?,
    )
}

/// The four routes of the two categories, for the test patient.
fn routes() -> Result<Vec<Request<Body>>, Box<dyn Error>> {
    Ok(vec![
        search()?,
        read()?,
        Request::post(format!("{FHIR}/MedicationDispense"))
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(r#"{"resourceType":"MedicationDispense"}"#))?,
        Request::get(format!("{FHIR}/MedicationDispense/{}", dispensation_id()?))
            .body(Body::empty())?,
    ])
}

/// `GET {fhir-base}/metadata`.
fn metadata() -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/metadata")).body(Body::empty())
}

/// The resource types the capability statement `text` lists.
fn listed(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let statement: Value = serde_json::from_str(text)?;
    Ok(statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .ok_or("rest[0].resource")?
        .iter()
        .filter_map(|resource| resource["type"].as_str().map(str::to_owned))
        .collect())
}

#[tokio::test]
async fn with_no_member_declared_every_route_answers_not_served_and_asks_no_member() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    for request in routes()? {
        let (app, _) = gateway_registered(
            dir.path(),
            (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
            &rows,
            "",
            ("", ""),
        )?;
        let (status, text) = call(app, request).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
        let (code, diagnostics) = issue(&text)?;
        assert_eq!("not-supported", code, "{text}");
        assert!(
            diagnostics.contains("are not served by this deployment"),
            "{diagnostics}"
        );
    }
    assert!(untouched(&a, &b).await?, "no member is asked");
    Ok(())
}

#[tokio::test]
async fn with_no_member_declared_the_capability_statement_lists_neither_resource() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let (app, _) = gateway_registered(
        dir.path(),
        (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
        &[("node-a", EHR_A)],
        "",
        ("", ""),
    )?;
    let (status, text) = call(app, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let types = listed(&text)?;
    assert!(types.iter().any(|kind| kind == "Patient"), "{types:?}");
    for kind in ["MedicationRequest", "MedicationDispense"] {
        assert!(
            !types.iter().any(|listed| listed == kind),
            "{kind}: {types:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_declared_member_lists_both_resources_in_their_mpd_profiles() -> TestResult {
    let (a, b, own) = (
        Server::start().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    let (app, _) = gateway_registered(
        dir.path(),
        (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
        &[("node-a", EHR_A)],
        &mpd_endpoint(&own.uri()),
        ("", SERVED),
    )?;
    let (status, text) = call(app, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let statement: Value = serde_json::from_str(&text)?;
    let resource = |kind: &str| {
        statement
            .pointer("/rest/0/resource")
            .and_then(Value::as_array)
            .and_then(|resources| resources.iter().find(|resource| resource["type"] == kind))
            .cloned()
            .ok_or_else(|| format!("{kind} is not listed: {text}"))
    };
    let requests = resource("MedicationRequest")?;
    assert_eq!(
        Value::from(vec![MPD_MEDICATION_REQUEST]),
        requests["supportedProfile"]
    );
    let codes: Vec<&str> = requests["interaction"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|interaction| interaction["code"].as_str())
        .collect();
    assert_eq!(vec!["search-type", "read"], codes);
    let dispenses = resource("MedicationDispense")?;
    assert_eq!(
        Value::from(vec![MPD_MEDICATION_DISPENSE]),
        dispenses["supportedProfile"]
    );
    assert_eq!("create", dispenses["interaction"][0]["code"]);
    Ok(())
}

#[tokio::test]
async fn every_interaction_declared_for_a_prescription_or_dispensation_is_one_the_route_serves()
-> TestResult {
    let (a, b, own) = (
        Server::start().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    let gateway = || {
        gateway_registered(
            dir.path(),
            (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
            &[("node-a", EHR_A)],
            &mpd_endpoint(&own.uri()),
            ("", SERVED),
        )
    };
    let (app, _) = gateway()?;
    let (status, text) = call(app, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let statement: Value = serde_json::from_str(&text)?;
    let declared: Vec<(String, String)> = statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .ok_or("rest[0].resource")?
        .iter()
        .filter(|resource| {
            resource["type"] == "MedicationRequest" || resource["type"] == "MedicationDispense"
        })
        .flat_map(|resource| {
            let kind = resource["type"].as_str().unwrap_or_default().to_owned();
            resource["interaction"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |interaction| {
                    interaction["code"]
                        .as_str()
                        .map(|code| (kind.clone(), code.to_owned()))
                })
        })
        .collect();
    let named = |kind: &str, code: &str| {
        declared
            .iter()
            .any(|(held, interaction)| held == kind && interaction == code)
    };
    assert!(named("MedicationRequest", "read"), "{declared:?}");
    assert!(named("MedicationDispense", "create"), "{declared:?}");
    for (kind, code) in &declared {
        let request = exercising(kind, code)?
            .ok_or_else(|| format!("{kind} {code} is declared and no route serves it"))?;
        let before = crate::facade::received(&own).await?.len();
        let (app, _) = gateway()?;
        let (status, text) = call(app, request).await?;
        assert_ne!(
            StatusCode::NOT_IMPLEMENTED,
            status,
            "FHIR R4 http.html §3.1.0.10: {kind} {code} is declared, so it is served: {text}"
        );
        assert!(
            crate::facade::received(&own).await?.len() > before,
            "{kind} {code} reaches the member's own endpoint: {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_served_category_is_never_answered_as_not_served() -> TestResult {
    let (a, b, own) = (
        Server::start().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    for (position, request) in routes()?.into_iter().enumerate() {
        let (app, _) = gateway_registered(
            dir.path(),
            (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
            &[("node-a", EHR_A)],
            &mpd_endpoint(&own.uri()),
            ("", SERVED),
        )?;
        let (status, text) = call(app, request).await?;
        assert_ne!(StatusCode::NOT_FOUND, status, "{text}");
        assert!(!text.contains("not served"), "{text}");
        // The search, the read and the dispensation read each ask the member once; the
        // dispensation, held to its profile first, is refused before any member is asked.
        assert_eq!(
            [1, 2, 2, 3].get(position).copied().ok_or("a route")?,
            crate::facade::received(&own).await?.len(),
            "route {position}: the search and the reads reach the member's own endpoint"
        );
    }
    assert!(untouched(&a, &b).await?);
    Ok(())
}

#[tokio::test]
async fn another_method_on_a_prescription_path_is_refused_at_the_gate() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let (app, _) = gateway_registered(
        dir.path(),
        (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
        &[("node-a", EHR_A)],
        "",
        ("", ""),
    )?;
    let request = Request::post(format!("{FHIR}/MedicationRequest"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(r#"{"resourceType":"MedicationRequest"}"#))?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert!(untouched(&a, &b).await?);
    Ok(())
}

/// What loading the federation gives whose registry also declares
/// `members`, with the cross-reference rows `rows` and the `[fhir]`
/// tables `tables` added.
fn loaded(
    members: &str,
    rows: &[(&str, &str)],
    tables: &str,
) -> Result<Result<Option<Federation>, FederationError>, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (
            "http://a.example.org",
            "http://b.example.org",
            UNREACHED_SUPPLIER,
        ),
        rows,
        members,
        ("", tables),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings))
}

#[test]
fn a_member_declared_with_its_mpd_endpoint_starts() -> TestResult {
    let loaded = loaded(
        &mpd_endpoint("http://a.example.org"),
        &[("node-a", EHR_A)],
        SERVED,
    )?;
    assert!(matches!(loaded, Ok(Some(_))), "{loaded:?}");
    Ok(())
}

#[test]
fn a_member_with_no_endpoint_of_its_own_refuses_the_start() -> TestResult {
    let loaded = loaded("", &[("node-a", EHR_A)], SERVED)?;
    assert!(
        matches!(
            loaded,
            Err(FederationError::ServingWithoutEndpoint {
                category: "Electronic-Prescriptions",
                ref member,
            }) if member == "node-a"
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn a_member_whose_endpoint_serves_another_category_refuses_the_start() -> TestResult {
    let members = own_endpoint(
        "http://a.example.org",
        "\"Electronic-Prescriptions\"",
        &format!("\"{MPD_MEDICATION_REQUEST}\""),
    );
    let loaded = loaded(&members, &[("node-a", EHR_A)], SERVED)?;
    assert!(
        matches!(
            loaded,
            Err(FederationError::ServingWithoutEndpoint {
                category: "Electronic-Dispensations",
                ..
            })
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn an_endpoint_without_the_mpd_profile_refuses_the_start() -> TestResult {
    let members = own_endpoint(
        "http://a.example.org",
        "\"Electronic-Prescriptions\", \"Electronic-Dispensations\"",
        &format!("\"{MPD_MEDICATION_REQUEST}\""),
    );
    let loaded = loaded(&members, &[("node-a", EHR_A)], SERVED)?;
    assert!(
        matches!(
            loaded,
            Err(FederationError::NativeProfileMissing { ref category, profile, .. })
                if category == "Electronic-Dispensations" && profile == MPD_MEDICATION_DISPENSE
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn a_member_the_registry_does_not_hold_refuses_the_start() -> TestResult {
    let tables = SERVED.replace("node-a", "node-z");
    let loaded = loaded("", &[("node-a", EHR_A)], &tables)?;
    assert!(
        matches!(
            loaded,
            Err(FederationError::ServingMemberUnknown { ref member, .. }) if member == "node-z"
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn serving_with_no_resolver_refuses_the_start() -> TestResult {
    let loaded = loaded(&mpd_endpoint("http://a.example.org"), &[], SERVED)?;
    assert!(
        matches!(loaded, Err(FederationError::ServingWithoutResolver)),
        "{loaded:?}"
    );
    Ok(())
}

/// What resolving the configuration with the `[fhir]` tables `tables`
/// added gives.
fn resolved(
    tables: &str,
) -> Result<Result<(), ferrofed_server::config::error::Error>, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (
            "http://a.example.org",
            "http://b.example.org",
            UNREACHED_SUPPLIER,
        ),
        &[("node-a", EHR_A)],
        "",
        ("", tables),
    )?;
    Ok(
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
            .resolve()
            .map(|_| ()),
    )
}

#[test]
fn a_category_no_member_serves_resource_by_resource_is_refused() -> TestResult {
    let resolved =
        resolved("\n[[fhir.serve]]\ncategory = \"Patient-Summaries\"\nmembers = [\"node-a\"]\n")?;
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(FhirError::Serving(ServingError::Unservable { ref category })))
                if category == "Patient-Summaries"
        ),
        "{resolved:?}"
    );
    Ok(())
}

#[test]
fn a_dispensation_member_that_serves_no_prescription_is_refused() -> TestResult {
    let resolved = resolved(
        "\n[[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [\"node-a\"]\n",
    )?;
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(FhirError::Serving(
                ServingError::DispensingWithoutPrescribing { ref member }
            ))) if member == "node-a"
        ),
        "{resolved:?}"
    );
    Ok(())
}

/// The standard output of `config check` over the gateway whose registry
/// also declares `members`, with the `[fhir]` tables `tables` added, after
/// asserting that it passed.
fn checked(members: &str, tables: &str) -> Result<String, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let text = text_registered(
        dir.path(),
        (
            "http://a.example.org",
            "http://b.example.org",
            UNREACHED_SUPPLIER,
        ),
        &[("node-a", EHR_A)],
        members,
        ("", tables),
    )?;
    let output = binary(&["config", "check"], &text)?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(stdout)
}

#[test]
fn config_check_names_the_serving_members() -> TestResult {
    let stdout = checked(&mpd_endpoint("http://a.example.org"), SERVED)?;
    for code in ["Electronic-Prescriptions", "Electronic-Dispensations"] {
        assert!(
            stdout.contains(&format!(
                "ferrofed: note: [[fhir.serve]] {code}: the FHIR face serves this Art 14(1) category from the members node-a, each on its own endpoint\n"
            )),
            "{stdout}"
        );
    }
    assert!(!stdout.contains("names no member"), "{stdout}");
    Ok(())
}

#[test]
fn config_check_names_the_categories_no_member_serves() -> TestResult {
    let stdout = checked("", "")?;
    assert!(
        stdout.contains(
            "ferrofed: note: [[fhir.serve]] names no member for Electronic-Prescriptions, Electronic-Dispensations: the FHIR face answers each of these Art 14(1) categories as not served\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "ferrofed: note: [[fhir.receive]] names no receiving member for Patient-Summaries: the FHIR face refuses a document of this Art 14(1) category\n"
        ),
        "a category served resource by resource never arrives as a document: {stdout}"
    );
    Ok(())
}
/// A request of the interaction or operation `code` of the resource type
/// `kind` the capability statement declares, or `None` for a pair this test
/// knows no request for.
fn exercising(kind: &str, code: &str) -> Result<Option<Request<Body>>, Box<dyn Error>> {
    let patient = format!("{}%7C{PATIENT}", NAMESPACE.replace(':', "%3A"));
    let fhir_json = |method: http::Method, path: &str, body: String| {
        Request::builder()
            .method(method)
            .uri(format!("{FHIR}{path}"))
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(body))
    };
    Ok(Some(match (kind, code) {
        ("Patient", "summary") => {
            Request::get(format!("{FHIR}/Patient/$summary?identifier={patient}"))
                .body(Body::empty())?
        }
        ("DocumentReference", "search-type") => Request::get(format!(
            "{FHIR}/DocumentReference?patient.identifier={patient}"
        ))
        .body(Body::empty())?,
        ("Binary", "read") => {
            Request::get(format!("{FHIR}/Binary/synthetic-token")).body(Body::empty())?
        }
        ("Bundle", "create") => {
            fhir_json(http::Method::POST, "/Bundle", super::receive::document())?
        }
        ("MedicationRequest", "search-type") => search()?,
        ("MedicationRequest", "read") => read()?,
        ("MedicationDispense", "create") => fhir_json(
            http::Method::POST,
            "/MedicationDispense",
            super::prescriptions::dispense::dispensation(
                super::prescriptions::dispense::BY_IDENTIFIER,
            ),
        )?,
        ("MedicationDispense", "read") => Request::get(format!(
            "{FHIR}/MedicationDispense/{}",
            Locator::new(EndpointId::new("node-a-mpd")?, EhrId::new(EHR_A)?, "md-1")
                .id()
                .ok_or("the parts fit an id")?
        ))
        .body(Body::empty())?,
        _ => return Ok(None),
    }))
}

/// Every interaction and operation the capability statement `text`
/// declares, each as its resource type and its code or name.
fn declared_pairs(text: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let statement: Value = serde_json::from_str(text)?;
    let mut declared = Vec::new();
    for resource in statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .ok_or("rest[0].resource")?
    {
        let kind = resource["type"].as_str().ok_or("a type")?;
        for (list, key) in [("interaction", "code"), ("operation", "name")] {
            for entry in resource[list].as_array().into_iter().flatten() {
                let code = entry[key].as_str().ok_or("a code")?;
                declared.push((kind.to_owned(), code.to_owned()));
            }
        }
    }
    Ok(declared)
}

/// Whether the answer `status` and `text` is one the configured gateway
/// gives a request it does not serve: `501`, the face's `404` for a path it
/// has no route for, a `not-supported` `404` of a category no member serves,
/// or the refusal of a document no member receives.
fn unserved(status: StatusCode, text: &str) -> bool {
    status == StatusCode::NOT_IMPLEMENTED
        || text.contains("the FHIR face serves")
        || (status == StatusCode::NOT_FOUND
            && issue(text).is_ok_and(|(code, _)| code == "not-supported"))
        || text.contains("no member receives documents")
}

/// Holds every interaction and operation the gateway `gateway` makes
/// declares to an answer that serves it.
async fn every_declared_is_served(
    gateway: impl Fn() -> Result<axum::Router, Box<dyn Error>>,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let (status, text) = call(gateway()?, metadata()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let declared = declared_pairs(&text)?;
    for (kind, code) in &declared {
        let request = exercising(kind, code)?.ok_or_else(|| {
            format!("{kind} {code} is declared and this test has no request for it")
        })?;
        let (status, text) = call(gateway()?, request).await?;
        assert!(
            !unserved(status, &text),
            "FHIR R4 capabilitystatement: {kind} {code} is declared, so this gateway serves it: {status} {text}"
        );
    }
    Ok(declared)
}

#[tokio::test]
async fn every_interaction_and_operation_the_statement_declares_is_served() -> TestResult {
    let (a, b, own) = (
        Server::start().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    let declared = every_declared_is_served(|| {
        Ok(gateway_registered(
            dir.path(),
            (&a.uri(), &b.uri(), UNREACHED_SUPPLIER),
            &[("node-a", EHR_A)],
            &mpd_endpoint(&own.uri()),
            ("", SERVED),
        )?
        .0)
    })
    .await?;
    assert!(declared.len() >= 7, "{declared:?}");
    assert!(
        !declared.iter().any(|(kind, _)| kind == "Bundle"),
        "with no [[fhir.receive]], no Bundle create is declared: {declared:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_receiving_gateway_declares_and_serves_the_bundle_create() -> TestResult {
    let (a, b, own) = (
        super::receive::creating().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    let crossref = super::receive::rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!(
        "{}{SERVED}",
        super::receive::receiving("node-a", &[NAMESPACE])
    );
    let declared = every_declared_is_served(|| {
        super::receive::gateway_listing(
            dir.path(),
            (&a, &b),
            &mpd_endpoint(&own.uri()),
            (&crossref, &fhir),
            &crate::facade::settings_with_room(),
        )
    })
    .await?;
    assert!(
        declared
            .iter()
            .any(|(kind, code)| kind == "Bundle" && code == "create"),
        "{declared:?}"
    );
    Ok(())
}
