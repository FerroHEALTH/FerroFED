// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the start makes of the serving members: a member declared with its own
//! endpoint in the HL7 Europe MPD profiles starts, one with no such endpoint,
//! one unknown to the registry or one with no resolver refuses the start, and
//! `config check` names the members.

use std::collections::BTreeMap;
use std::error::Error;

use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_eehrxf::serving::ServingError;
use ferrofed_server::config::Config;
use ferrofed_server::config::fhir::FhirError;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::{SERVED, exercising, metadata, mpd_endpoint, own_endpoint};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, registry};
use crate::fhir::receive::issue;
use crate::fhir::{TestResult, UNREACHED_SUPPLIER, gateway_registered, text_registered};
use crate::run::binary;
use crate::support::{call, unasked_pixm};

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
        &[],
        members,
        (
            "",
            &format!(
                "{tables}{}",
                unasked_pixm(&registry(
                    "http://a.example.org",
                    "http://b.example.org",
                    members
                ))
            ),
        ),
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
            "ferrofed: note: [[fhir.receive]] names no receiving member for Patient-Summaries, Laboratory-Reports: the FHIR face refuses a document of any of these Art 14(1) categories\n"
        ),
        "a category served resource by resource never arrives as a document: {stdout}"
    );
    Ok(())
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
        crate::fhir::receive::creating().await,
        Server::start().await,
        Server::start().await,
    );
    let dir = tempfile::tempdir()?;
    let crossref = crate::fhir::receive::rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!(
        "{}{SERVED}",
        crate::fhir::receive::receiving("node-a", &[NAMESPACE])
    );
    let declared = every_declared_is_served(|| {
        crate::fhir::receive::gateway_listing(
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

/// A `[[fhir.serve]]` of prescriptions with node A as the table `member`.
fn mapped_table(member: &str) -> String {
    format!("\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [{member}]\n")
}

/// `path` as a TOML string.
fn quoted(path: &std::path::Path) -> String {
    toml::Value::String(path.display().to_string()).to_string()
}

#[test]
fn a_map_mode_member_whose_template_the_mpd_project_does_not_map_is_refused() -> TestResult {
    let opt = ferrofed_testkit::eps::opt();
    let resolved = resolved(&mapped_table(&format!(
        "{{ member = \"node-a\", mode = \"map\", template = {} }}",
        quoted(&opt)
    )))?;
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(FhirError::Mapped { ref member, .. }))
                if member == "node-a"
        ),
        "{resolved:?}"
    );
    Ok(())
}

#[test]
fn a_map_mode_member_with_no_template_is_refused() -> TestResult {
    let resolved = resolved(&mapped_table("{ member = \"node-a\", mode = \"map\" }"))?;
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(FhirError::MapTemplate { ref member, .. }))
                if member == "node-a"
        ),
        "{resolved:?}"
    );
    Ok(())
}

#[test]
fn a_map_mode_member_of_dispensations_is_refused() -> TestResult {
    let template = ferrofed_testkit::medication::template_path();
    let resolved = resolved(&format!(
        "{}\n[[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [{{ member = \"node-a\", mode = \"map\", template = {} }}]\n",
        mapped_table("\"node-a\""),
        quoted(&template)
    ))?;
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(FhirError::MapCategory { ref member, .. }))
                if member == "node-a"
        ),
        "{resolved:?}"
    );
    Ok(())
}
