// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A summary allergy coded in SNOMED CT, and a server that maps it to ATC.
//!
//! The MyHealth@EU Core binding of `AllergyIntolerance.code` to
//! `eHDSIAllergenWithExceptions` asks for the ATC coding (#2039).
//!
//! The substance is amoxicillin: SNOMED CT `372687004`, which the stub's
//! `ConceptMap/$translate` maps `equivalent` to ATC `J01CA04`, a member of
//! the Master Value Sets Catalogue's `eHDSIActiveIngredient`. Every other
//! translation answers no match. The codes are real; the patient and the
//! record are synthetic.
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
                "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "SNOMED-CT" },
                "code_string": SUBSTANCE.0
            }
        });
    }
    composition.to_string()
}

/// Starts the terminology server: `ConceptMap/$translate` of [`SUBSTANCE`]
/// into ATC answers [`INGREDIENT`] as `equivalent`, every other
/// translation no match.
pub async fn start() -> Server {
    let server = Server::start().await;
    let answer =
        |body: String| ResponseTemplate::new(200).set_body_raw(body, "application/fhir+json");
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .and(body_string_contains(SUBSTANCE.0))
        .and(body_string_contains(ATC))
        .respond_with(answer(format!(
            r#"{{"resourceType":"Parameters","parameter":[{{"name":"result","valueBoolean":true}},{{"name":"match","part":[{{"name":"equivalence","valueCode":"equivalent"}},{{"name":"concept","valueCoding":{{"system":"{ATC}","code":"{INGREDIENT}","display":"amoxicillin"}}}}]}}]}}"#
        )))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(answer(String::from(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
        )))
        .with_priority(5)
        .mount(&server)
        .await;
    server
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
