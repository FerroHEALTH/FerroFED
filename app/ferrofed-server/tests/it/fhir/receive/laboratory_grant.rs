// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A person's own laboratory reports through the FHIR face, as an
//! electronic health data access service asks for them (Regulation (EU)
//! 2025/327 Art 3(1), Art 14(1)(e)): a grant confined to one patient lists
//! and reads that patient's laboratory report, and is refused another
//! patient's list `403` before any member is asked (Federation Tier §5.2),
//! with no patient identifier reaching a node (§5.4.1, N33).

use std::error::Error;
use std::path::Path;

use ferrofed_testkit::laboratory;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::laboratory::{LAB_OBJECT, holding, lab_composition, receiving};
use super::{EHR_SYSTEM, OTHER, OTHER_A, OTHER_NAMESPACE, bound, gateway_with, rows, untouched};
use crate::auth::{bearing, minted};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::fhir::patient_grant::own_grant;
use crate::support::call;

/// The `ehr_id` of [`OTHER`] at node B.
const OTHER_B: &str = "6666ffff-6666-4666-8666-666666666666";

/// The gateway receiving laboratory reports at node A, its issuer's
/// patient tokens bound to node A, the cross-reference knowing both
/// patients at both members and the token's `ehrId` in node A's `ehr_id`
/// system.
fn gateway(dir: &Path, a: &Server, b: &Server) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-b", EHR_B),
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
        (OTHER_NAMESPACE, OTHER, "node-b", OTHER_B),
    ]);
    gateway_with(dir, (a, b), (&crossref, &receiving(dir)?), &bound()?)
}

/// The list of [`OTHER`]'s laboratory reports.
fn others_list() -> Result<http::Request<axum::body::Body>, http::Error> {
    http::Request::get(format!(
        "{}/DocumentReference?patient.identifier={}%7C{OTHER}&type=11502-2",
        crate::fhir::FHIR,
        OTHER_NAMESPACE.replace(':', "%3A")
    ))
    .body(axum::body::Body::empty())
}

#[tokio::test]
async fn a_persons_own_grant_lists_and_reads_their_laboratory_report() -> TestResult {
    let dir = tempfile::tempdir()?;
    let composition = lab_composition(dir.path())?;
    let row = format!(
        r#"[{composition},"{}","{}"]"#,
        super::laboratory::LAB_VERSION,
        laboratory::TEMPLATE
    );
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let app = gateway(dir.path(), &a, &b)?;
    let token = minted(&own_grant())?;
    let (status, text) = call(app.clone(), bearing(list("&type=11502-2")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 3(1): {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("Art 3(1): the person's one laboratory report: {bundle}").into());
    };
    assert_eq!(
        reference.pointer("/category/0/coding/0/code"),
        Some(&Value::from("Laboratory-Reports"))
    );
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, text) = call(app, bearing(binary(url)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 3(1): {text}");
    let read = eehrxf::receive::ReceivedDocument::read(&text)?;
    assert_eq!(eehrxf::category::Category::Laboratory, read.category()?);
    let document: Value = serde_json::from_str(&text)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{LAB_OBJECT}"))),
        "the person's own report, as the list names it"
    );
    for request in a.received_requests().await.unwrap_or_default() {
        let carried = format!("{} {}", request.url, String::from_utf8_lossy(&request.body));
        for withheld in [PATIENT, PATIENT_TAIL] {
            assert!(
                !carried.contains(withheld),
                "N33: no patient identifier reaches node A"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_persons_own_grant_asking_for_another_patients_laboratory_reports_is_refused()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let (a, b) = (holding("", None).await, holding("", None).await);
    let app = gateway(dir.path(), &a, &b)?;
    let token = minted(&own_grant())?;
    let (status, text) = call(app, bearing(others_list()?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "Art 3(1), §5.2: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.starts_with("patient-confinement"),
        "the code every confined route answers: {diagnostics}"
    );
    assert!(
        !text.contains(OTHER),
        "the refusal names no patient: {text}"
    );
    assert!(untouched(&a, &b).await?, "no member is asked");
    Ok(())
}
