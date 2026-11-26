// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stub terminology server that answers `ValueSet/$validate-code` on the
//! MyHealth@EU value set eHDSILanguage, which `composition-hdr-myhealtheu`
//! binds `Composition.language` to with strength `required`.
//!
//! It holds `nl-NL`, the language-region tag of the synthetic records
//! written in Dutch in the Netherlands, and answers `false` for every other
//! code (FHIR R4 `ValueSet/$validate-code`, the `result` parameter). No
//! specification governs the harness: our own design.

use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;

/// The FHIR base the stub answers under, below its URI.
pub const BASE: &str = "/fhir";

/// Starts the stub; its FHIR base is its URI followed by [`BASE`].
pub async fn start() -> Server {
    let server = Server::start().await;
    let answer = |result: bool| {
        ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"result","valueBoolean":{result}}}]}}"#
            ),
            "application/fhir+json",
        )
    };
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/ValueSet/$validate-code")))
        .and(body_string_contains("nl-NL"))
        .respond_with(answer(true))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{BASE}/ValueSet/$validate-code")))
        .respond_with(answer(false))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// The `[eehrxf.terminology.endpoint]` table that points a gateway at the
/// stub `server`.
#[must_use]
pub fn table(server: &Server) -> String {
    format!(
        "\n[eehrxf.terminology.endpoint]\nurl = \"{}{BASE}\"\n",
        server.uri()
    )
}
