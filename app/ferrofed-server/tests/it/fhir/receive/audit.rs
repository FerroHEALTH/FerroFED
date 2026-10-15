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

/// Regulation (EU) 2025/327 Art 5: the record of a document a person
/// inserted under their own grant says so, and a professional's does not.
#[tokio::test]
async fn a_receipt_a_person_inserted_is_recorded_as_inserted_by_them() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ehds_logging::balp::inserted::ENTITY;
    use ferrofed_testkit::atna_feed::FeedRepository;

    use super::inserted::{PERSON, own_grant, person_gateway};
    use crate::auth::{bearing, minted};

    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = person_gateway(dir.path(), (&a, &b), &audit_tables(&repository, ""))?;
    let token = minted(&own_grant())?;
    let (status, text) = call(app.clone(), bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (status, text) = call(app, post(&document())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = repository.wait_for(2, SETTLE).await;
    let records = crate::access::accesses(&records)?;
    assert_eq!(2, records.len(), "one record per receipt: {records:#?}");
    let (marked, unmarked): (Vec<_>, Vec<_>) = records
        .iter()
        .partition(|record| !crate::access::named(record, ENTITY).is_empty());
    let ([person], [professional]) = (marked.as_slice(), unmarked.as_slice()) else {
        return Err(format!("one marked record and one not: {records:#?}").into());
    };
    assert_eq!(
        vec!["true"],
        crate::access::details(person, ENTITY, ENTITY),
        "the person's receipt is recorded as inserted by them: {person}"
    );
    assert!(
        person.to_string().contains(PERSON) && !professional.to_string().contains(PERSON),
        "the marked record is the person's own, a professional's carries no such mark"
    );
    Ok(())
}

/// Regulation (EU) 2025/327 Art 4(2), Art 5: the record of a document a
/// representative inserted under the person's grant says a representative
/// inserted it, never the person.
#[tokio::test]
async fn a_receipt_a_representative_inserted_is_recorded_as_theirs() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ehds_logging::balp::inserted::{ENTITY, REPRESENTATIVE_ENTITY};
    use ferrofed_testkit::atna_feed::FeedRepository;

    use super::representative::{REPRESENTATIVE, acted_for, representing_gateway};
    use crate::auth::{bearing, minted};

    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = representing_gateway(dir.path(), (&a, &b), &audit_tables(&repository, ""))?;
    let token = minted(&acted_for(&[("sub", REPRESENTATIVE)]))?;
    let (status, text) = call(app, bearing(post(&document())?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records = repository.wait_for(1, SETTLE).await;
    let records = crate::access::accesses(&records)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the receipt: {records:#?}").into());
    };
    assert_eq!(
        vec!["true"],
        crate::access::details(record, REPRESENTATIVE_ENTITY, REPRESENTATIVE_ENTITY),
        "the receipt is recorded as inserted by a representative: {record}"
    );
    assert!(
        crate::access::named(record, ENTITY).is_empty(),
        "a representative's insertion is not recorded as the person's: {record}"
    );
    Ok(())
}
