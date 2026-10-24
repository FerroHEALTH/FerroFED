// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each semantic transformation the FHIR face applies, logged on the access
//! record of the request that applied it (eHealth Network General
//! Guidelines Release 3.1 Art 6(3)): the direction, the category, the
//! mapping and terminology versions, and the elements transcoded,
//! untranscoded (eHealth Network Guidelines on Patient Summary Art 10(7))
//! and dropped, by path and never by value.

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;

use super::receive::{SUBSTANCE, creating, document, gateway_with, post, receiving, rows};
use super::{TestResult, UID_A, UID_B, gateway_over, node_holding, summary, supplier};
use crate::access::{accesses, details, named};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, settings_with_room};
use crate::feed_audit::{SETTLE, audit_tables};
use crate::support::{exchange, field};

/// The name of the entity a transformation is written as.
const ENTITY: &str = "ehds-transformation";

/// The substance the summary's members record.
const SUMMARY_SUBSTANCE: &str = "Synthetic substance";

/// The code of the clinical status the summary's members record.
const CLINICAL_STATUS: &str = "\"active\"";

/// The LOINC codes the received document carries.
const LOINC_CODES: [&str; 6] = [
    "60591-5", "11450-4", "48765-2", "10160-0", "47519-4", "46264-8",
];

/// What a request answered, the request id it was answered under, and the
/// one access record the repository holds once `expected` records came.
struct Recorded {
    /// The answer's status.
    status: StatusCode,
    /// The answer's body.
    text: String,
    /// The `x-request-id` the answer was echoed with.
    request_id: String,
    /// The access record, read as JSON.
    record: Value,
}

/// Sends `request` to `app` and reads the one access record `repository`
/// holds once `expected` records came.
async fn recorded(
    app: Router,
    request: Request<Body>,
    (repository, expected): (&FeedRepository, usize),
) -> Result<Recorded, Box<dyn Error>> {
    let (status, headers, body) = exchange(app, request).await?;
    let request_id = field(&headers, "x-request-id")
        .ok_or("the answer echoes its request id")?
        .to_owned();
    let records = repository.wait_for(expected, SETTLE).await;
    let [record] = accesses(&records)?
        .try_into()
        .map_err(|records| format!("one access record, got {records:?}"))?;
    Ok(Recorded {
        status,
        text: String::from_utf8(body)?,
        request_id,
        record,
    })
}

/// A patient summary from two members that each hold one allergy, recorded.
async fn summarised() -> Result<Recorded, Box<dyn Error>> {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let tables = audit_tables(&repository, "");
    let pdq = supplier().await?;
    let (app, _) = gateway_over(
        dir.path(),
        (&a.uri(), &b.uri(), &pdq.base_url()),
        &rows,
        ("", &tables),
    )?;
    // NOTE: PDQm §2:3.78.5.1; the header's ITI-78 exchange is audited beside the access record.
    recorded(app, summary("")?, (&repository, 2)).await
}

/// A received patient summary written to node A, recorded.
async fn received() -> Result<Recorded, Box<dyn Error>> {
    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let fhir = format!(
        "{}{}",
        receiving("node-a", &[NAMESPACE]),
        audit_tables(&repository, "")
    );
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &fhir),
        &settings_with_room(),
    )?;
    recorded(app, post(&document())?, (&repository, 1)).await
}

/// The one transformation entity of `record`, as JSON text.
fn the_transformation(record: &Value) -> Result<String, Box<dyn Error>> {
    let [entity] = named(record, ENTITY)
        .try_into()
        .map_err(|entities| format!("one transformation, got {entities:?}"))?;
    Ok(entity.to_string())
}

/// The value of the `detail` `kind` of the transformation of `record`.
fn detail(record: &Value, kind: &str) -> Vec<String> {
    details(record, ENTITY, kind)
}

/// The mapping and the terminology in force both directions name: the
/// fixture's context, template, profile and files, and the releases of the
/// preferred code systems.
fn assert_pins(record: &Value) {
    assert_eq!(
        detail(record, "mapping-context"),
        [eps::CONTEXT],
        "the context mapping that ran"
    );
    assert_eq!(
        detail(record, "mapping-template"),
        [eps::TEMPLATE],
        "the template it maps"
    );
    assert_eq!(
        detail(record, "mapping-profile"),
        ["http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core"],
        "the profile it maps to"
    );
    assert_eq!(
        detail(record, "mapping-file"),
        ["ferrofed_allergy.yml", "eu_allergy.context.yml"],
        "the model and context mapping files, in the order configured"
    );
    assert_eq!(
        detail(record, "terminology-version"),
        [
            "ehn-patient-summary-guidelines|3.4",
            "myhealth-eu-mvc|9.1.0"
        ],
        "the releases the preferred code systems are read from"
    );
    assert_eq!(
        detail(record, "ehds-transformation-category"),
        [
            "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs|Patient-Summaries"
        ],
        "Art 14(1)(a)"
    );
}

#[tokio::test]
async fn a_summary_records_its_transformation_by_path_with_nothing_dropped_as_zero() -> TestResult {
    let summary = summarised().await?;
    assert_eq!(StatusCode::OK, summary.status, "{}", summary.text);
    let record = &summary.record;
    assert_eq!(
        detail(record, "ehds-transformation-direction"),
        ["provided"]
    );
    assert_pins(record);
    assert!(
        detail(record, "mapping-package").is_empty(),
        "a summary is held to no received package"
    );
    assert_eq!(detail(record, "ehds-transcoded"), ["0"]);
    assert_eq!(detail(record, "ehds-transcoded-path"), Vec::<String>::new());
    assert_eq!(
        detail(record, "ehds-untranscoded"),
        ["4"],
        "Art 10(7): the substance as text and the status in a system §4 does not list, from each member"
    );
    assert_eq!(
        detail(record, "ehds-untranscoded-path"),
        [
            "AllergyIntolerance.clinicalStatus=2",
            "AllergyIntolerance.code=2"
        ]
    );
    assert_eq!(
        detail(record, "ehds-dropped"),
        ["0"],
        "a mapping that drops nothing records zero"
    );
    assert_eq!(detail(record, "ehds-dropped-path"), Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn a_receipt_records_its_transformation_by_path_with_the_loss_it_declared() -> TestResult {
    let receipt = received().await?;
    assert_eq!(StatusCode::OK, receipt.status, "{}", receipt.text);
    let record = &receipt.record;
    assert_eq!(
        detail(record, "ehds-transformation-direction"),
        ["received"]
    );
    assert_pins(record);
    assert_eq!(
        detail(record, "mapping-package"),
        ["hl7.fhir.eu.eps-1.0.0-ballot.tgz"],
        "the package the document was held to"
    );
    assert_eq!(detail(record, "ehds-transcoded"), ["6"]);
    assert_eq!(
        detail(record, "ehds-transcoded-path"),
        ["Composition.section.code=5", "Composition.type=1"],
        "every LOINC coding is in a preferred system"
    );
    assert_eq!(detail(record, "ehds-untranscoded"), ["6"]);
    assert_eq!(
        detail(record, "ehds-untranscoded-path"),
        [
            "AllergyIntolerance.clinicalStatus=1",
            "AllergyIntolerance.code=1",
            "Composition.section.emptyReason=4"
        ],
        "Art 10(7): the substance arrives as text alone, and the HL7 codes are in no preferred system"
    );
    assert_eq!(
        detail(record, "ehds-dropped"),
        ["2"],
        "the losses the mapping run declared"
    );
    assert_eq!(
        detail(record, "ehds-dropped-path"),
        [
            "AllergyIntolerance.clinicalStatus=1",
            "AllergyIntolerance.patient=1"
        ],
        "at the R4 element the loss's expression locates"
    );
    Ok(())
}

#[tokio::test]
async fn a_transformation_holds_no_clinical_value_code_value_or_patient_identifier() -> TestResult {
    let summary = summarised().await?;
    let receipt = received().await?;
    let summary_entity = the_transformation(&summary.record)?;
    let receipt_entity = the_transformation(&receipt.record)?;
    for (entity, withheld) in [
        (&summary_entity, SUMMARY_SUBSTANCE),
        (&summary_entity, CLINICAL_STATUS),
        (&summary_entity, PATIENT),
        (&summary_entity, PATIENT_TAIL),
        (&summary_entity, EHR_A),
        (&summary_entity, EHR_B),
        (&receipt_entity, SUBSTANCE),
        (&receipt_entity, PATIENT),
        (&receipt_entity, PATIENT_TAIL),
        (&receipt_entity, EHR_A),
    ] {
        assert!(
            !entity.contains(withheld),
            "Art 6(3) asks for a log by path, never by value: {withheld} in {entity}"
        );
    }
    for code in LOINC_CODES {
        assert!(
            !receipt_entity.contains(code),
            "a code's system may be named, never its value: {code} in {receipt_entity}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_transformation_shares_the_request_id_of_its_access_record() -> TestResult {
    for recorded in [summarised().await?, received().await?] {
        let record = &recorded.record;
        the_transformation(record)?;
        let transactions: Vec<&Value> = record["entity"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entity| entity["type"]["code"] == "XrequestId")
            .collect();
        let ids: Vec<&str> = transactions
            .iter()
            .filter_map(|entity| entity.pointer("/what/identifier/value"))
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            ids,
            [recorded.request_id.as_str()],
            "the transformation rides on the access record of the request that applied it"
        );
    }
    Ok(())
}
