// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A summary transcoded for MyHealth@EU: each allergy gains, beside its
//! recorded SNOMED CT coding, the ATC active ingredient the terminology
//! server maps it to, so `AllergyIntolerance.code` meets the MyHealth@EU
//! Core binding to `eHDSIAllergenWithExceptions`; an allergen no
//! translation reaches is kept as recorded and listed as unbound
//! (Implementing Regulation (EU) 2026/2083 Art 3(4)).

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use eehrxf::terminology::translation::MYHEALTH_EU_CORE;
use ferrofed_eehrxf::summary::mappings::Mappings;
use ferrofed_eehrxf::summary::{Answer, Assembled, Held};
use ferrofed_testkit::eps;
use ferrofed_testkit::eps::transcoding::{self, ATC, INGREDIENT, SNOMED, SUBSTANCE};
use ferrofed_testkit::mock::Server;
use wiremock::matchers::path;
use wiremock::{Mock, ResponseTemplate};

use super::{allergies, answers, document_designating, header};

/// The allergy composition `uid` of one member, coded in SNOMED CT.
fn coded(uid: &str) -> Held {
    Held {
        uid: uid.to_owned(),
        template: eps::TEMPLATE.to_owned(),
        composition: transcoding::coded_allergy_composition(uid, eps::TEMPLATE),
        attestations: None,
    }
}

/// The terminology `server` answers under `/fhir`.
fn asking(server: &Server) -> Result<Terminology, Box<dyn Error>> {
    let endpoint = serde_json::from_value(serde_json::json!({
        "url": format!("{}/fhir", server.uri()),
        "timeout": "5s",
        "retry": { "max_attempts": 1 },
    }))?;
    let client = ferrofed_eehrxf::terminology::BridgeTerminology::from_endpoint(
        &endpoint,
        "eehrxf.terminology.endpoint",
    )?;
    Ok(Terminology::server(Arc::new(client)))
}

/// The summary of both members' coded allergies, transcoded for
/// MyHealth@EU through `server`.
async fn transcoded(server: &Server) -> Result<Assembled, Box<dyn Error>> {
    designated(server, None).await
}

/// The summary of [`transcoded`], each coding a translation adds given the
/// server's designation in `designations` when it is set.
async fn designated(
    server: &Server,
    designations: Option<&str>,
) -> Result<Assembled, Box<dyn Error>> {
    document_designating(
        &answers(
            &[coded(
                "7c4e1d20-0000-4000-8000-0000000000e1::cdr-a.example.org::1",
            )],
            &Answer::Answered(vec![coded(
                "7c4e1d20-0000-4000-8000-0000000000e2::cdr-b.example.org::1",
            )]),
        ),
        (header(), &[]),
        (
            &Mappings::compile(&[allergies()?])?,
            &asking(server)?,
            &MYHEALTH_EU_CORE,
        ),
        designations,
    )
    .await
}

/// The codings of each allergy's code in `bundle`, each as system and code.
fn allergy_codes(bundle: &serde_json::Value) -> Vec<Vec<(String, String)>> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == "AllergyIntolerance")
        .map(|resource| {
            resource["code"]["coding"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|coding| {
                    Some((
                        coding["system"].as_str()?.to_owned(),
                        coding["code"].as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn each_allergy_gains_its_atc_ingredient_beside_its_recorded_coding()
-> Result<(), Box<dyn Error>> {
    let server = transcoding::start().await;
    let assembled = transcoded(&server).await?;
    assert!(assembled.unbound.is_empty(), "{:?}", assembled.unbound);
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "the EPS profiles: {findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let codes = allergy_codes(&bundle);
    assert_eq!(2, codes.len(), "one allergy per member: {codes:?}");
    for codings in &codes {
        assert_eq!(
            Some(&(ATC.to_owned(), INGREDIENT.to_owned())),
            codings.last(),
            "the ATC ingredient after the recorded coding: {codings:?}"
        );
        assert_eq!(
            Some(&(SNOMED.to_owned(), SUBSTANCE.0.to_owned())),
            codings.first(),
            "the recorded SNOMED CT coding stays first, under its URI: {codings:?}"
        );
    }
    ferrofed_testkit::emit::document(
        ferrofed_testkit::emit::Lane::MyHealthEuSummary,
        "ferrofed-eehrxf-summary-myhealth-eu",
        &text,
    )?;
    Ok(())
}

#[tokio::test]
async fn an_allergen_no_translation_reaches_is_kept_as_recorded_and_listed_unbound()
-> Result<(), Box<dyn Error>> {
    let server = Server::start().await;
    // NOTE: MyHealth@EU Core 1.0.1 eHDSIAllergenWithExceptions holds the recorded SNOMED CT drug
    // allergen in no enumerated concept, which $validate-code decides, and no translation reaches it.
    for operation in [
        "/fhir/ValueSet/$validate-code",
        "/fhir/ConceptMap/$translate",
    ] {
        Mock::given(path(operation))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
                "application/fhir+json",
            ))
            .mount(&server)
            .await;
    }
    let assembled = transcoded(&server).await?;
    assert_eq!(
        2,
        assembled.unbound.len(),
        "each member's allergen meets no binding: {:?}",
        assembled.unbound
    );
    assert!(
        assembled
            .unbound
            .iter()
            .all(|unbound| unbound.location.starts_with("AllergyIntolerance.code")),
        "{:?}",
        assembled.unbound
    );
    let bundle: serde_json::Value = serde_json::to_value(&assembled.bundle)?;
    for codings in allergy_codes(&bundle) {
        assert!(
            codings.iter().all(|(system, _)| system != ATC),
            "no ATC coding is invented: {codings:?}"
        );
    }
    Ok(())
}

/// The Dutch designation the stub holds for [`INGREDIENT`].
const DUTCH: &str = "amoxicilline";

/// Mounts on `server` a `CodeSystem/$lookup` of [`INGREDIENT`] that holds
/// its Dutch designation (FHIR R4 `$lookup`, the `designation` parts).
async fn designating(server: &Server) {
    Mock::given(path("/fhir/CodeSystem/$lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"name","valueString":"ATC"}},{{"name":"display","valueString":"amoxicillin"}},{{"name":"designation","part":[{{"name":"language","valueCode":"nl"}},{{"name":"value","valueString":"{DUTCH}"}}]}}]}}"#
            ),
            "application/fhir+json",
        ))
        .mount(server)
        .await;
}

/// The coding of each allergy's code in `bundle` whose system is `system`.
fn codings_of<'b>(bundle: &'b serde_json::Value, system: &str) -> Vec<&'b serde_json::Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == "AllergyIntolerance")
        .flat_map(|resource| resource["code"]["coding"].as_array().into_iter().flatten())
        .filter(|coding| coding["system"] == system)
        .collect()
}

#[tokio::test]
async fn each_added_coding_carries_the_designation_in_the_requested_language()
-> Result<(), Box<dyn Error>> {
    let server = transcoding::start().await;
    designating(&server).await;
    let assembled = designated(&server, Some("nl")).await?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let added = codings_of(&bundle, ATC);
    assert_eq!(2, added.len(), "one ATC coding per allergy: {added:?}");
    for coding in added {
        assert!(
            coding.to_string().contains(DUTCH),
            "the Dutch designation the server holds: {coding}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_server_that_refuses_the_designation_fails_the_summary() -> Result<(), Box<dyn Error>> {
    let server = transcoding::start().await;
    Mock::given(path("/fhir/CodeSystem/$lookup"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let refused = designated(&server, Some("nl")).await;
    assert!(refused.is_err(), "a refused designation is never left out");
    Ok(())
}
