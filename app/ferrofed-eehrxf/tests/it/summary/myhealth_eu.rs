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
use ferrofed_testkit::eps::transcoding::{self, ATC, INGREDIENT, SUBSTANCE};
use ferrofed_testkit::mock::Server;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{allergies, answers, document_asking, header};

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
    document_asking(
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
        assert!(
            codings.iter().any(|(_, code)| code == SUBSTANCE.0),
            "the recorded SNOMED CT coding stays: {codings:?}"
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
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
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
