// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step of `[pdqm]`, ahead of localization and
//! resolution (Annex A §A.2 and §A.7): a client names the patient in a
//! namespace the cross-reference does not map, the harness PDQm Supplier
//! answers with the master identity, the PIX Manager resolves that, and each
//! node is asked by its own `ehr_id` alone (§5.2, §5.4.1, N3, N6, N33). No
//! match, several matches, an outage under the localization failure policy
//! (§14.1, N4), the configuration refusals, the health and metrics surface,
//! and the audit records each have their own cases.

mod audit;
mod config;
mod flow;
mod subject;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use http::{Request, StatusCode};
use serde::Deserialize;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, settings_with_room};
use crate::support::call;

/// The client's identifier, in a namespace the cross-reference does not map.
pub(crate) const CLIENT_ID: &str = "SENTINEL-LOCAL-9q4w";
pub(crate) const LOCAL: &str = "urn:oid:2.999.7";

/// The master domain, and the master identifier the Supplier knows the
/// patient under.
pub(crate) const MASTER: &str = "urn:oid:2.999.1";
pub(crate) const MASTER_ID: &str = "SENTINEL-MASTER-7z3x";

/// The `ehr_id` domains of node A and node B at the PIX Manager.
pub(crate) const DOMAIN_A: &str = "urn:oid:2.999.10";
pub(crate) const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The façade query naming the patient by the client's identifier.
pub(crate) fn local_query() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{CLIENT_ID}' \
         AND e/ehr_status/subject/external_ref/namespace = '{LOCAL}'"
    )
}

/// A harness Supplier that knows the patient under the client's identifier
/// and the master identifier.
pub(crate) async fn supplier() -> Result<PdqSupplier, Box<dyn Error>> {
    let supplier = PdqSupplier::start().await?;
    supplier.add(&[(LOCAL, CLIENT_ID), (MASTER, MASTER_ID)], true)?;
    Ok(supplier)
}

/// A PIX Manager that resolves the master identifier at node A and node B,
/// and knows no other identifier (ITI-83 §2:3.83.4.2.3, Case 2).
pub(crate) async fn manager() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .and(query_param("sourceIdentifier", format!("{MASTER}|{MASTER_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}},{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_B}","value":"{EHR_B}"}}}}]}}"#
            )
            .into_bytes(),
            "application/fhir+json",
        ))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .respond_with(ResponseTemplate::new(404).set_body_raw(
            br#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#.to_vec(),
            "application/fhir+json",
        ))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// The `[pixm]` table of the PIX Manager at `pix`, the `[pdqm]` table of the
/// Supplier at `pdq` asked with `transaction`, and `[audit]` to the log.
pub(crate) fn tables(pix: &str, pdq: &str, transaction: &str) -> String {
    format!(
        "[audit]\ndestination = \"log\"\n\n{}\n{}",
        pixm(pix),
        pdqm(pdq, transaction)
    )
}

/// The `[pixm]` table of the PIX Manager at `pix` serving node A and node B.
pub(crate) fn pixm(pix: &str) -> String {
    format!(
        "[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n"
    )
}

/// The `[pdqm]` table of the Supplier whose FHIR base is `base`.
pub(crate) fn pdqm(base: &str, transaction: &str) -> String {
    format!(
        "[pdqm]\nurl = \"{base}\"\ntransaction = \"{transaction}\"\nmaster = \"{MASTER}\"\ntimeout_ms = 1000\n\n[pdqm.namespaces]\n\"{LOCAL}\" = \"{LOCAL}\"\n"
    )
}

/// A development gateway over the registry `registry`, with the
/// `[federation]` keys `federation` beside the budgets and the id, and the
/// tables `tables`; its state too, for the metrics it holds.
pub(crate) fn gateway(
    dir: &Path,
    registry: &str,
    federation: &str,
    tables: &str,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let settings = settings(dir, registry, federation, tables)?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(Arc::clone(&state), &settings_with_room());
    Ok((app, state))
}

/// The settings of the development gateway [`gateway`] builds.
pub(crate) fn settings(
    dir: &Path,
    registry: &str,
    federation: &str,
    tables: &str,
) -> Result<Settings, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nid = \"example-federation\"\n{federation}\n\n{tables}"
    );
    Ok(Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?)
}

/// The `[federation]` key of an ask-all deployment, which has no localizer.
pub(crate) const ASK_ALL: &str = "node_selection = \"ask-all\"";

/// The state `GET /health/dependencies` reports of the demographics service.
pub(crate) async fn demographics_state(app: &Router) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        demographics: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    if status != StatusCode::OK {
        return Err(format!("/health/dependencies answered {status}: {text}").into());
    }
    Ok(serde_json::from_str::<Report>(&text)?.demographics)
}
