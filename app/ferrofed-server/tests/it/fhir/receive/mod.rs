// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document received on the FHIR face, written as one composition to the
//! member the deployment declares for its category (Regulation (EU) 2025/327
//! Annex II 2.2 and 2.3; Federation Tier §2.3, §12.4, N23): the member's own
//! `ehr_id` and no patient identifier in the path, the query string or the
//! headers (§5.4.1, N33, CP-26), no other member asked, and every refusal
//! answered before anything reaches a member.

#[cfg(feature = "binding-ihe")]
mod audit;
mod check;
mod display;
mod inserted;
mod refused;
mod representative;
mod start;
mod write;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::PatientBinding;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::{eps, fhir_packages};
use http::{Request, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{FHIR, PUBLIC, UNREACHED_SUPPLIER, fhir_tables_with};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, received, registry, settings_with_room};

/// The version node A creates for the composition.
pub(super) const VERSION_A: &str = "7c4e1d20-0000-4000-8000-0000000008a1::cdr-a.example.org::1";

/// A second synthetic patient, and its `ehr_id` at node A.
pub(super) const OTHER: &str = "SENTINEL-OTHER-71xw";
pub(super) const OTHER_A: &str = "4444dddd-4444-4444-8444-444444444444";

/// A second identifier system of the example OID arc, which names
/// [`OTHER`].
pub(super) const OTHER_NAMESPACE: &str = "urn:oid:2.999.2";

/// The identifier system of node A's `ehr_id`s at the cross-reference.
pub(super) const EHR_SYSTEM: &str = "urn:oid:2.999.9.1";

/// The synthetic substance the document records.
pub(super) const SUBSTANCE: &str = "Synthetic substance one";

/// The `[[dev.crossref]]` rows, each `(namespace, value, member, ehr_id)`.
pub(super) fn rows(rows: &[(&str, &str, &str, &str)]) -> String {
    rows.iter()
        .fold(String::new(), |mut text, (namespace, value, member, ehr_id)| {
            // NOTE: writing to a String cannot fail, so the result is dropped.
            let _written: std::fmt::Result = write!(
                text,
                "\n[[dev.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
            );
            text
        })
}

/// The `[fhir]` tables of a face that receives patient summaries at
/// `member`, resolving their patient in `namespaces`.
#[expect(
    clippy::expect_used,
    reason = "a test helper the suites call for a configuration text; a package that cannot be fetched fails the test"
)]
pub(super) fn receiving(member: &str, namespaces: &[&str]) -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string());
    let [model, context] = eps::mapping_files();
    let dependencies = fhir_packages::dependencies()
        .expect("the Patient Summary's package closure")
        .iter()
        .map(|dependency| path(dependency).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let namespaces = namespaces
        .iter()
        .map(|namespace| format!("\"{namespace}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fhir_tables_with(
        UNREACHED_SUPPLIER,
        &format!("receive_namespaces = [{namespaces}]"),
        &format!(
            "\n[[fhir.receive]]\ncategory = \"Patient-Summaries\"\nmember = \"{member}\"\n\
             package = {}\ndependencies = [{dependencies}]\ntemplate = {}\n\
             files = [{}, {}]\ncontext = \"{}\"\nlanguage = \"en\"\nterritory = \"NL\"\n",
            path(&eps::package()),
            path(&eps::opt()),
            path(&model),
            path(&context),
            eps::CONTEXT
        ),
    )
}

/// The configuration text of a development gateway over node A at `a`,
/// node B at `b` and the further registry entries `members`, the
/// cross-reference `crossref` and the face `fhir`.
fn text(
    dir: &Path,
    (a, b, members): (&str, &str, &str),
    crossref: &str,
    fhir: &str,
) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, members))?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[server]\npublic_url = \"{PUBLIC}\"\n\n\
         [registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\
         {crossref}{fhir}"
    ))
}

/// The gateway of [`text`] over node A and node B alone, driven with
/// `server`.
pub(super) fn gateway_with(
    dir: &Path,
    nodes: (&Server, &Server),
    (crossref, fhir): (&str, &str),
    server: &ServerSettings,
) -> Result<Router, Box<dyn Error>> {
    gateway_listing(dir, nodes, "", (crossref, fhir), server)
}

/// The gateway of [`text`] whose registry also lists `members`, driven
/// with `server`.
pub(super) fn gateway_listing(
    dir: &Path,
    (a, b): (&Server, &Server),
    members: &str,
    (crossref, fhir): (&str, &str),
    server: &ServerSettings,
) -> Result<Router, Box<dyn Error>> {
    let text = text(dir, (&a.uri(), &b.uri(), members), crossref, fhir)?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, server))
}

/// The gateway receiving patient summaries at node A, the patient known at
/// both nodes.
pub(super) fn gateway(dir: &Path, a: &Server, b: &Server) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &receiving("node-a", &[NAMESPACE])),
        &settings_with_room(),
    )
}

/// A member that creates every composition as [`VERSION_A`].
pub(super) async fn creating() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", format!("\"{VERSION_A}\"").as_str())
                .insert_header(
                    "Location",
                    format!("https://cdr-a.example.org/v1/ehr/{EHR_A}/composition/{VERSION_A}")
                        .as_str(),
                ),
        )
        .mount(&server)
        .await;
    server
}

/// `POST {fhir-base}/Bundle` carrying `document`.
pub(super) fn post(document: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("{FHIR}/Bundle"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(document.to_owned()))
}

/// The patient summary of [`PATIENT`].
pub(super) fn document() -> String {
    eps::received_document(&[(NAMESPACE, PATIENT)], SUBSTANCE)
}

/// The issue type and the diagnostics of the first issue of the
/// `OperationOutcome` `text` holds.
pub(super) fn issue(text: &str) -> Result<(String, String), Box<dyn Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    if outcome.get("resourceType") != Some(&Value::from("OperationOutcome")) {
        return Err(format!("no OperationOutcome: {text}").into());
    }
    let read = |pointer: &str| {
        outcome
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok((read("/issue/0/code"), read("/issue/0/diagnostics")))
}

/// Whether neither node received anything.
pub(super) async fn untouched(a: &Server, b: &Server) -> Result<bool, Box<dyn Error>> {
    Ok(received(a).await?.is_empty() && received(b).await?.is_empty())
}

/// The suite's server settings, its issuer's patient tokens bound to node
/// A, whose `ehr_id`s the cross-reference knows under [`EHR_SYSTEM`].
pub(super) fn bound() -> Result<ServerSettings, Box<dyn Error>> {
    let mut server = settings_with_room();
    for issuer in &mut server.auth.issuers {
        issuer.patient = Some(PatientBinding {
            endpoint: EndpointId::new("node-a-pub")?,
            ehr_id_system: IdentifierNamespace::new(EHR_SYSTEM)?,
            act_is_representative: false,
        });
    }
    Ok(server)
}
