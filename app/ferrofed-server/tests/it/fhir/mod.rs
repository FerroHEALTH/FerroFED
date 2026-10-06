// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 face of the European exchange format (Regulation (EU)
//! 2025/327 Annex II 2.1): `{fhir-base}/Patient/$summary` and
//! `{fhir-base}/metadata` over two mock members, the patient summary held
//! to the vendored HL7 Europe Patient Summary profiles, no patient
//! identifier reaching a node (§5.4.1, N33), every request authenticated and
//! every summary recorded (Annex II 3.2), every error an `OperationOutcome`,
//! and the ITS-REST face under `{base}` unchanged (N1, N28).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the documents and the records are read as JSON values"
)]

mod config;
mod header;
mod refusals;
mod summary;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use http::Request;
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, registry, settings_with_room};

type TestResult = Result<(), Box<dyn Error>>;

/// `{fhir-base}`.
const FHIR: &str = "/fhir";

/// The public URL of the gateway under test, in the `example.org` domain.
const PUBLIC: &str = "https://gw.example.org";

/// The uid of the composition node A holds.
const UID_A: &str = "7c4e1d20-0000-4000-8000-0000000000a1::cdr-a.example.org::1";

/// The uid of the composition node B holds.
const UID_B: &str = "7c4e1d20-0000-4000-8000-0000000000b1::cdr-b.example.org::1";

/// The archetype only the allergies section query names.
const ALLERGIES: &str = "openEHR-EHR-EVALUATION.adverse_reaction_risk.v1";

/// The synthetic family name the harness PDQm Supplier holds the patient
/// under.
const FAMILY: &str = "SENTINEL-FAMILY-5t2v";

/// The synthetic given name the harness PDQm Supplier holds the patient
/// under.
const GIVEN: &str = "SENTINEL-GIVEN-8m1c";

/// The synthetic birth date the harness PDQm Supplier holds.
const BIRTH_DATE: &str = "1970-01-01";

/// A PDQm Supplier base no test reaches, for a configuration that is only
/// read.
const UNREACHED_SUPPLIER: &str = "http://127.0.0.1:9/fhir/";

/// A harness PDQm Supplier that holds the test patient under its
/// identifier, with a synthetic name and birth date.
async fn supplier() -> Result<PdqSupplier, Box<dyn Error>> {
    let supplier = PdqSupplier::start().await?;
    let id = supplier.add(&[(NAMESPACE, PATIENT)], true)?;
    supplier.describe(&id, (FAMILY, GIVEN), Some(BIRTH_DATE))?;
    Ok(supplier)
}

/// The `[fhir]` tables of the face, its allergies fed by the testkit's
/// fixture mapping, and the `[pdqm]` table of the Supplier at `pdq` the
/// summary header is asked of, whose master domain is the test patient's
/// namespace.
fn fhir_tables(pdq: &str) -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string());
    let [model, context] = eps::mapping_files();
    format!(
        "\n[fhir]\nbase = \"{FHIR}\"\n\n[fhir.operator]\nname = \"Synthetic Operator\"\n\
         identifier_system = \"urn:oid:2.999.9\"\nidentifier_value = \"operator-1\"\n\n\
         [[fhir.mapping]]\nsection = \"allergies-and-intolerances\"\ntemplate = {}\n\
         files = [{}, {}]\ncontext = \"{}\"\n\n\
         [pdqm]\nurl = \"{pdq}\"\ntransaction = \"iti-78\"\nmaster = \"{NAMESPACE}\"\n\
         timeout_ms = 1000\n\n[pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n",
        path(&eps::opt()),
        path(&model),
        path(&context),
        eps::CONTEXT
    )
}

/// A development gateway over node A at `a` and node B at `b`, the summary
/// header asked of the PDQm Supplier at `pdq`, the patient known at the
/// members `rows` name, serving the face, with the top-level `[federation]`
/// keys `federation` and the tables `tables` added.
fn gateway_over(
    dir: &Path,
    (a, b, pdq): (&str, &str, &str),
    rows: &[(&str, &str)],
    (federation, tables): (&str, &str),
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[server]\npublic_url = \"{PUBLIC}\"\n\n\
         [registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\
         {federation}\n{}{}{tables}",
        crossref(rows),
        fhir_tables(pdq)
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok((
        ferrofed_server::router(Arc::clone(&state), &settings_with_room()),
        state,
    ))
}

/// The gateway of [`gateway_over`] over node A and node B, the patient
/// known at both, the header asked of `pdq`.
fn gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    pdq: &PdqSupplier,
) -> Result<Router, Box<dyn Error>> {
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    Ok(gateway_over(dir, (&a.uri(), &b.uri(), &pdq.base_url()), &rows, ("", ""))?.0)
}

/// A node answering the allergies section query with one composition of
/// the fixture template, the uid `uid`, and every other section query with
/// no row.
async fn node_holding(uid: &str) -> Server {
    let server = Server::start().await;
    let composition = eps::allergy_composition(uid, eps::TEMPLATE, "Synthetic substance");
    let columns = r#"[{"name":"composition","path":"c"},{"name":"uid","path":"c/uid/value"},{"name":"template_id","path":"c/archetype_details/template_id/value"}]"#;
    let held = format!(
        r#"{{"q":"node","columns":{columns},"rows":[[{composition},"{uid}","{}"]]}}"#,
        eps::TEMPLATE
    );
    let none = format!(r#"{{"q":"node","columns":{columns},"rows":[]}}"#);
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(ALLERGIES))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(held.into_bytes(), "application/json"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(none.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// `GET {fhir-base}/Patient/$summary` for the test patient, with the query
/// `extra` added.
fn summary(extra: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "{FHIR}/Patient/$summary?identifier={}%7C{PATIENT}{extra}",
        NAMESPACE.replace(':', "%3A")
    ))
    .body(Body::empty())
}

/// The composition section of `document` coded `code`.
fn section<'a>(document: &'a Value, code: &str) -> Option<&'a Value> {
    document
        .pointer("/entry/0/resource/section")?
        .as_array()?
        .iter()
        .find(|section| section.pointer("/code/coding/0/code") == Some(&Value::from(code)))
}

/// The resources of `document` of the type `kind`.
fn resources<'a>(document: &'a Value, kind: &str) -> Vec<&'a Value> {
    document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect()
}
