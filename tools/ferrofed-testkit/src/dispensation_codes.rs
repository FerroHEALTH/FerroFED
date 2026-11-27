// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation's medicine coded as a national contact point sends it, in
//! ATC, and a terminology server that maps it back into SNOMED CT.
//!
//! The medicine is amoxicillin: ATC `J01CA04`, a member of the Master Value
//! Sets Catalogue's `eHDSIActiveIngredient`, which the stub's
//! `ConceptMap/$translate` maps `equivalent` to SNOMED CT `27658006`. Every
//! other translation answers no match. The codes are real; the patient and
//! the dispensation are synthetic.

use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;

/// The ATC system URI, as the Master Value Sets Catalogue names it.
pub const ATC: &str = "http://www.whocc.no/atc";

/// The SNOMED CT system URI.
pub const SNOMED: &str = "http://snomed.info/sct";

/// The medicine as the contact point codes it, in ATC, and its display.
pub const SENT: (&str, &str) = ("J01CA04", "amoxicillin");

/// The SNOMED CT concept the stub maps [`SENT`] to, and its display.
pub const RECORDED: (&str, &str) = ("27658006", "Product containing amoxicillin");

/// The `medicationCodeableConcept` of a dispensation of [`SENT`].
#[must_use]
pub fn medication() -> String {
    format!(
        r#"{{"coding":[{{"system":"{ATC}","code":"{}","display":"{}"}}],"text":"Synthetic medicine A"}}"#,
        SENT.0, SENT.1
    )
}

/// Mounts on `server` the answers of `ConceptMap/$translate` under `/fhir`.
///
/// [`SENT`] into SNOMED CT answers [`RECORDED`] as `equivalent`, and every
/// other translation no match.
pub async fn mount(server: &Server) {
    let answer =
        |body: String| ResponseTemplate::new(200).set_body_raw(body, "application/fhir+json");
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .and(body_string_contains(SENT.0))
        .and(body_string_contains(SNOMED))
        .respond_with(answer(format!(
            r#"{{"resourceType":"Parameters","parameter":[{{"name":"result","valueBoolean":true}},{{"name":"match","part":[{{"name":"equivalence","valueCode":"equivalent"}},{{"name":"concept","valueCoding":{{"system":"{SNOMED}","code":"{}","display":"{}"}}}}]}}]}}"#,
            RECORDED.0, RECORDED.1
        )))
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(answer(String::from(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
        )))
        .with_priority(5)
        .mount(server)
        .await;
}

/// Starts the terminology server of [`mount`].
pub async fn start() -> Server {
    let server = Server::start().await;
    mount(&server).await;
    server
}

/// The `[eehrxf.terminology]` table of a face that asks `server` and
/// transcodes for MyHealth@EU.
#[must_use]
pub fn table(server: &Server) -> String {
    format!(
        "\n[eehrxf.terminology]\ntranscode = \"myhealth-eu\"\n\n[eehrxf.terminology.endpoint]\nurl = \"{}/fhir\"\ntimeout = '5s'\n",
        server.uri()
    )
}
