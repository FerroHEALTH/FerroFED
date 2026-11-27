// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A person's own discharge and imaging reports through the FHIR face, as
//! an electronic health data access service asks for them (Regulation (EU)
//! 2025/327 Art 3(1), Art 14(1)(d) and (f)): a grant confined to one
//! patient lists each report and downloads it in the exchange format
//! (Art 3(2)), is refused another patient's list `403` before any member
//! is asked (Federation Tier §5.2), and reads a member the consent service
//! restricts exactly as one holding nothing (Art 8, §11.3), with no
//! patient identifier reaching a node (§5.4.1, N33).

use std::error::Error;
use std::path::Path;

use eehrxf::category::Category;
use eehrxf::receive::ReceivedDocument;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::{discharge as hdr, ehdsi_language, imaging as eu_imaging, pdqm_stub};
use http::StatusCode;
use serde_json::Value;

use super::{
    EHR_SYSTEM, OTHER, OTHER_A, OTHER_NAMESPACE, bound, discharge, gateway_with, imaging, rows,
    untouched,
};
use crate::auth::{bearing, minted};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, wire};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::fhir::patient_grant::own_grant;
use crate::support::call;

/// The `ehr_id` of [`OTHER`] at node B.
const OTHER_B: &str = "7777aaaa-7777-4777-8777-777777777777";

/// The LOINC document type of a discharge report.
const DISCHARGE_TYPE: &str = "34105-7";

/// The LOINC document type of an imaging report.
const IMAGING_TYPE: &str = "18748-4";

/// The `[federation.consent]` table that withholds a restriction's fact,
/// as an EU deployment sets it, and the pre-filter row denying node A for
/// [`PATIENT`].
fn restricted_at_a() -> String {
    format!(
        "\n[federation.consent]\ndisclose = false\n\n[[testkit.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-a\"\n"
    )
}

/// The gateway writing discharge and imaging reports, its issuer's patient
/// tokens bound to node A, the cross-reference knowing both patients at
/// both members and the token's `ehrId` in node A's `ehr_id` system, with
/// the further configuration `consent` after the cross-reference.
fn gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    (pdq, terminology): (&Server, &Server),
    consent: &str,
) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-b", EHR_B),
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
        (OTHER_NAMESPACE, OTHER, "node-b", OTHER_B),
    ]);
    let reports = format!("{}{}", discharge::declared(), imaging::declared());
    gateway_with(
        dir,
        (a, b),
        (
            &format!("{crossref}{consent}"),
            &discharge::writing(&pdqm_stub::base_url(pdq), &reports, terminology),
        ),
        &bound()?,
    )
}

/// The one report of the type `code` the person's own grant lists from
/// `app`, and the document its signed URL downloads.
async fn own_report(app: axum::Router, code: &str) -> Result<(Value, String), Box<dyn Error>> {
    let token = minted(&own_grant())?;
    let (status, text) = call(
        app.clone(),
        bearing(list(&format!("&type={code}"))?, &token)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "Art 3(1): {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("Art 3(1): the person's one report of type {code}: {bundle}").into());
    };
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, document) = call(app, bearing(binary(url)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 3(2): {document}");
    Ok(((*reference).clone(), document))
}

#[tokio::test]
async fn a_persons_own_grant_lists_and_downloads_their_discharge_report() -> TestResult {
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
    let app = gateway(dir.path(), (&a, &b), (&pdq, &terminology), "")?;
    let (reference, document) = own_report(app, DISCHARGE_TYPE).await?;
    assert_eq!(
        reference.pointer("/category/0/coding/1/code"),
        Some(&Value::from("Discharge-Reports"))
    );
    assert_eq!(
        Category::Discharge,
        ReceivedDocument::read(&document)?.category()?,
        "Art 3(2): a discharge report in the exchange format"
    );
    let document: Value = serde_json::from_str(&document)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{}", discharge::OBJECT))),
        "the person's own report, as the list names it"
    );
    wire(&a).await?.assert_withholds(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier reaches node A",
    );
    Ok(())
}

#[tokio::test]
async fn a_persons_own_grant_lists_and_downloads_their_imaging_report() -> TestResult {
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
    let (pdq, terminology) = (crate::fhir::supplier().await, ehdsi_language::start().await);
    let app = gateway(dir.path(), (&a, &b), (&pdq, &terminology), "")?;
    let (reference, document) = own_report(app, IMAGING_TYPE).await?;
    assert_eq!(
        reference.pointer("/category/0/coding/1/code"),
        Some(&Value::from("Medical-Imaging"))
    );
    assert_eq!(
        Category::Imaging,
        ReceivedDocument::read(&document)?.category()?,
        "Art 3(2): an imaging report in the exchange format"
    );
    let document: Value = serde_json::from_str(&document)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{}", imaging::OBJECT))),
        "the person's own report, as the list names it"
    );
    wire(&a).await?.assert_withholds(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier reaches node A",
    );
    Ok(())
}

#[tokio::test]
async fn a_persons_own_grant_asking_for_another_patients_reports_is_refused() -> TestResult {
    for code in [DISCHARGE_TYPE, IMAGING_TYPE] {
        let dir = tempfile::tempdir()?;
        let (a, b) = (
            discharge::holding("", None).await,
            discharge::holding("", None).await,
        );
        let (pdq, terminology) = (crate::fhir::supplier().await, ehdsi_language::start().await);
        let app = gateway(dir.path(), (&a, &b), (&pdq, &terminology), "")?;
        let token = minted(&own_grant())?;
        let others = http::Request::get(format!(
            "{}/DocumentReference?patient.identifier={}%7C{OTHER}&type={code}",
            crate::fhir::FHIR,
            OTHER_NAMESPACE.replace(':', "%3A")
        ))
        .body(axum::body::Body::empty())?;
        let (status, text) = call(app, bearing(others, &token)?).await?;
        assert_eq!(
            StatusCode::FORBIDDEN,
            status,
            "{code}: Art 3(1), §5.2: {text}"
        );
        let outcome: Value = serde_json::from_str(&text)?;
        let diagnostics = outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .ok_or("diagnostics")?;
        assert!(
            diagnostics.starts_with("patient-confinement"),
            "{code}: the code every confined route answers: {diagnostics}"
        );
        assert!(
            !text.contains(OTHER),
            "{code}: the refusal names no patient: {text}"
        );
        assert!(untouched(&a, &b).await?, "{code}: no member is asked");
    }
    Ok(())
}

#[tokio::test]
async fn a_member_the_consent_service_restricts_reads_to_the_persons_grant_as_one_holding_nothing()
-> TestResult {
    let composition = hdr::composition(discharge::VERSION);
    let row = format!(
        r#"[{composition},"{}","{}"]"#,
        discharge::VERSION,
        hdr::TEMPLATE
    );
    let (pdq, terminology) = (crate::fhir::supplier().await, ehdsi_language::start().await);
    let token = minted(&own_grant())?;
    let restricted = {
        let (a, b) = (
            discharge::holding(&row, Some(&composition)).await,
            discharge::holding("", None).await,
        );
        let dir = tempfile::tempdir()?;
        let app = gateway(
            dir.path(),
            (&a, &b),
            (&pdq, &terminology),
            &restricted_at_a(),
        )?;
        let answer = call(
            app,
            bearing(list(&format!("&type={DISCHARGE_TYPE}"))?, &token)?,
        )
        .await?;
        assert!(
            received_nothing(&a).await?,
            "Art 8: the restricted member is never asked"
        );
        answer
    };
    let empty = {
        let (a, b) = (
            discharge::holding("", None).await,
            discharge::holding("", None).await,
        );
        let dir = tempfile::tempdir()?;
        let app = gateway(dir.path(), (&a, &b), (&pdq, &terminology), "")?;
        call(
            app,
            bearing(list(&format!("&type={DISCHARGE_TYPE}"))?, &token)?,
        )
        .await?
    };
    assert_eq!(StatusCode::OK, restricted.0, "{}", restricted.1);
    assert_eq!(
        serde_json::from_str::<Value>(&empty.1)?,
        serde_json::from_str::<Value>(&restricted.1)?,
        "Art 8, §11.3: the restriction's fact is not visible"
    );
    Ok(())
}

/// Whether `server` received no request at all.
async fn received_nothing(server: &Server) -> Result<bool, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .is_empty())
}
