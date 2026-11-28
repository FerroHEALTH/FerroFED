// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A summary allergy coded in SNOMED CT, and a server that maps it to ATC
//! and a diagnosis coded in SNOMED CT to ICD-10.
//!
//! The MyHealth@EU Core binding of `AllergyIntolerance.code` to
//! `eHDSIAllergenWithExceptions` asks for the ATC coding, and that of
//! `Condition.code` to `eHDSIProblemWithExceptions` the ICD-10 coding
//! (#2039).
//!
//! The substance is amoxicillin: SNOMED CT `372687004`, which the stub's
//! `ConceptMap/$translate` maps `equivalent` to ATC `J01CA04`, a member of
//! the Master Value Sets Catalogue's `eHDSIActiveIngredient`. The diagnosis
//! is asthma: SNOMED CT `195967001`, which it maps `equivalent` to ICD-10
//! `J45.9`. Every other translation answers no match. The codes are real;
//! the patient and the record are synthetic.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the composition is edited as JSON"
)]

use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;

/// The SNOMED CT system URI.
pub const SNOMED: &str = "http://snomed.info/sct";

/// The ATC system URI, as the Master Value Sets Catalogue names it.
pub const ATC: &str = "http://www.whocc.no/atc";

/// The SNOMED CT concept of the substance, and its display.
pub const SUBSTANCE: (&str, &str) = ("372687004", "Amoxicillin");

/// The ATC active ingredient the stub maps [`SUBSTANCE`] to.
pub const INGREDIENT: &str = "J01CA04";

/// The ICD-10 system URI.
pub const ICD_10: &str = "http://hl7.org/fhir/sid/icd-10";

/// The SNOMED CT concept of the diagnosis, and its display.
pub const DIAGNOSIS: (&str, &str) = ("195967001", "Asthma");

/// The ICD-10 category the stub maps [`DIAGNOSIS`] to.
pub const CATEGORY: &str = "J45.9";

/// The synthetic allergy composition `uid` of `template`, its substance
/// coded [`SUBSTANCE`] in SNOMED CT.
///
/// # Panics
///
/// Never: the fixture is built from a literal whose shape is fixed.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a test fixture built from a literal; a broken literal fails the test"
)]
pub fn coded_allergy_composition(uid: &str, template: &str) -> String {
    let mut composition: Value =
        serde_json::from_str(&super::allergy_composition(uid, template, SUBSTANCE.1))
            .expect("the fixture composition should read as JSON");
    if let Some(substance) = composition.pointer_mut("/content/0/data/items/0/value") {
        *substance = serde_json::json!({
            "_type": "DV_CODED_TEXT",
            "value": SUBSTANCE.1,
            "defining_code": {
                "_type": "CODE_PHRASE",
                "terminology_id": { "_type": "TERMINOLOGY_ID", "value": SNOMED },
                "code_string": SUBSTANCE.0
            }
        });
    }
    composition.to_string()
}

/// Starts the terminology server of [`mount`].
pub async fn start() -> Server {
    let server = Server::start().await;
    mount(&server).await;
    server
}

/// Mounts on `server` the answers of `ConceptMap/$translate` and
/// `ValueSet/$validate-code` under `/fhir`.
///
/// [`SUBSTANCE`] into ATC answers [`INGREDIENT`] and [`DIAGNOSIS`] into
/// ICD-10 answers [`CATEGORY`], each `equivalent`, and every other
/// translation no match. `$validate-code` answers that no recorded concept
/// is a member: neither SNOMED CT concept is in the MyHealth@EU Core value
/// set its element binds, the allergen set taking SNOMED CT only through
/// its non-drug allergens.
pub async fn mount(server: &Server) {
    Mock::given(path("/fhir/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
            "application/fhir+json",
        ))
        .mount(server)
        .await;
    let answer =
        |body: String| ResponseTemplate::new(200).set_body_raw(body, "application/fhir+json");
    let matched = |system: &str, code: &str, display: &str| {
        format!(
            r#"{{"resourceType":"Parameters","parameter":[{{"name":"result","valueBoolean":true}},{{"name":"match","part":[{{"name":"equivalence","valueCode":"equivalent"}},{{"name":"concept","valueCoding":{{"system":"{system}","code":"{code}","display":"{display}"}}}}]}}]}}"#
        )
    };
    for (source, target, held) in [
        (SUBSTANCE.0, ATC, (INGREDIENT, "amoxicillin")),
        (DIAGNOSIS.0, ICD_10, (CATEGORY, "Asthma, unspecified")),
    ] {
        Mock::given(method("POST"))
            .and(path("/fhir/ConceptMap/$translate"))
            .and(body_string_contains(source))
            .and(body_string_contains(target))
            .respond_with(answer(matched(target, held.0, held.1)))
            .with_priority(1)
            .mount(server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(answer(String::from(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
        )))
        .with_priority(5)
        .mount(server)
        .await;
}

/// The `[eehrxf.terminology]` table of a face that asks `server` and
/// transcodes each summary for MyHealth@EU.
#[must_use]
pub fn table(server: &Server) -> String {
    format!(
        "\n[eehrxf.terminology]\ntranscode = \"myhealth-eu\"\n\n[eehrxf.terminology.endpoint]\nurl = \"{}/fhir\"\ntimeout = '5s'\n",
        server.uri()
    )
}
