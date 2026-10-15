// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record of a receipt (Regulation (EU) 2025/327 Annex II 3.2).

use ferrofed_testkit::mock::Server;
use http::StatusCode;

use super::{VERSION_A, creating, document, gateway_with, post, receiving, rows};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, settings_with_room};
use crate::fhir::TestResult;
use crate::support::call;

#[tokio::test]
async fn a_receipt_is_recorded_once_as_a_creation_of_its_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

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
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(
        1,
        records.len(),
        "one access record per receipt: {records:#?}"
    );
    let record = records.first().ok_or("a record")?;
    for expected in [
        "eehrxf-document-priority-category-cs|Patient-Summaries:construction",
        "node-a-pub",
        EHR_A,
        VERSION_A,
        "create Bundle",
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    let [record] = crate::access::accesses(&records)?
        .try_into()
        .map_err(|records| format!("one access record, got {records:?}"))?;
    assert_eq!(
        vec!["active"],
        crate::access::details(&record, "origin", "status"),
        "§11.1, N16: the origin is named in the status set a routed write's is"
    );
    assert_eq!(
        vec![VERSION_A],
        crate::access::details(&record, "origin", "written-version"),
        "Annex II 3.2: the version the member created, as its answer named it"
    );
    Ok(())
}
