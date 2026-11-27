// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's caller lists and reads the reports the face
//! serves, as it relays the request of a health professional of another
//! Member State (Regulation (EU) 2025/327 Art 7(2), Art 23): a laboratory,
//! a discharge and an imaging report of the patient its treatment
//! relationship confirmation binds, each read through the signed URL the
//! list names, with no patient identifier reaching a node (§5.4.1, N33).

use std::error::Error;

use eehrxf::category::Category;
use eehrxf::receive::ReceivedDocument;
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::{
    discharge as hdr, ehdsi_language, imaging as eu_imaging, laboratory as lab, pdqm_stub,
};
use http::StatusCode;
use serde_json::Value;

use super::{discharge, gateway_with, imaging, laboratory, rows};
use crate::auth::minted;
use crate::contact_point::{bound, relaying};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, settings_with_room, wire};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::support::call;

/// The cross-reference knowing the TRC patient at both members.
fn crossref() -> String {
    rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ])
}

/// The suite's server settings with the test contact point trusted.
fn admitting() -> ServerSettings {
    let mut server = settings_with_room();
    server.auth = crate::contact_point::declared();
    server
}

/// The one report of the type `code` the contact point's caller lists from
/// `app`, the document its signed URL reads, and the category that
/// document is read as.
async fn relayed_report(
    app: axum::Router,
    code: &str,
) -> Result<(Value, String, Category), Box<dyn Error>> {
    let token = minted(&relaying())?;
    let (status, text) = call(app.clone(), bound(list(&format!("&type={code}"))?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 7(2): {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("Art 7(2): the patient's one report of type {code}: {bundle}").into());
    };
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, document) = call(app, bound(binary(url)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 7(2): {document}");
    let category = ReceivedDocument::read(&document)?.category()?;
    Ok(((*reference).clone(), document, category))
}

/// Asserts `document` is named `urn:uuid:{object}`, as the list names it.
fn named(document: &str, object: &str) -> TestResult {
    let document: Value = serde_json::from_str(document)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{object}"))),
        "the patient's own report, as the list names it"
    );
    Ok(())
}

/// Asserts no patient identifier reached `node`.
async fn withheld(node: &Server) -> TestResult {
    wire(node).await?.assert_withholds(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier reaches node A",
    );
    Ok(())
}

#[tokio::test]
async fn a_contact_point_lists_and_reads_a_laboratory_report() -> TestResult {
    let dir = tempfile::tempdir()?;
    let composition = laboratory::lab_composition(dir.path())?;
    let row = format!(
        r#"[{composition},"{}","{}"]"#,
        laboratory::LAB_VERSION,
        lab::TEMPLATE
    );
    let (a, b) = (
        laboratory::holding(&row, Some(&composition)).await,
        laboratory::holding("", None).await,
    );
    let pdq = crate::fhir::supplier().await;
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref(),
            &laboratory::receiving(dir.path(), &pdqm_stub::base_url(&pdq))?,
        ),
        &admitting(),
    )?;
    let (reference, document, category) = relayed_report(app, "11502-2").await?;
    assert_eq!(
        reference.pointer("/category/0/coding/1/code"),
        Some(&Value::from("Laboratory-Reports"))
    );
    assert_eq!(Category::Laboratory, category);
    named(&document, laboratory::LAB_OBJECT)?;
    withheld(&a).await
}

#[tokio::test]
async fn a_contact_point_lists_and_reads_a_discharge_report() -> TestResult {
    let composition = hdr::composition(discharge::VERSION);
    let row = format!(
        r#"[{composition},"{}","{}"]"#,
        discharge::VERSION,
        hdr::TEMPLATE
    );
    let (a, b) = (
        discharge::holding(&row, Some(&composition)).await,
        discharge::holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let (pdq, terminology) = (crate::fhir::supplier().await, ehdsi_language::start().await);
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref(),
            &discharge::writing(
                &pdqm_stub::base_url(&pdq),
                &discharge::declared(),
                &terminology,
            ),
        ),
        &admitting(),
    )?;
    let (reference, document, category) = relayed_report(app, "34105-7").await?;
    assert_eq!(
        reference.pointer("/category/0/coding/1/code"),
        Some(&Value::from("Discharge-Reports"))
    );
    assert_eq!(Category::Discharge, category);
    named(&document, discharge::OBJECT)?;
    withheld(&a).await
}

#[tokio::test]
async fn a_contact_point_lists_and_reads_an_imaging_report() -> TestResult {
    let composition = imaging::written_composition()?;
    let row = format!(
        r#"[{composition},"{}","{}"]"#,
        imaging::VERSION,
        eu_imaging::TEMPLATE
    );
    let (a, b) = (
        imaging::holding(&row, Some(&composition)).await,
        imaging::holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let pdq = crate::fhir::supplier().await;
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (
            &crossref(),
            &imaging::writing(&pdqm_stub::base_url(&pdq), &imaging::declared()),
        ),
        &admitting(),
    )?;
    let (reference, document, category) = relayed_report(app, "18748-4").await?;
    assert_eq!(
        reference.pointer("/category/0/coding/1/code"),
        Some(&Value::from("Medical-Imaging"))
    );
    assert_eq!(Category::Imaging, category);
    named(&document, imaging::OBJECT)?;
    withheld(&a).await
}
