// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR face's stable ids over FerroFED's own store on a real PostgreSQL
//! serving TLS (#1976): `patient=Patient/<id>` lists the patient's
//! documents, and an id the store does not hold, never recorded or past its
//! window, answers the empty `searchset` with an `outcome` entry asking for
//! the lookup again (FHIR R4 search §3.1.1.7), with no member asked.

use ferrofed_testkit::containers::{self, database};

use super::TestResult;
use super::store::{executed, store_toml};
use crate::fhir::list::patient::{SUMMARY_TYPE, declared, listed, patient_id, search, unknown};
use crate::fhir::list::{DOCUMENT_A, DOCUMENT_B, Members, entries, fhir_document, list};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_store_a_patient_id_lists_its_documents_until_it_expires() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("patient_id", &[]).await?;
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "patient_id", dir.path())?;
    let app = tokio::task::block_in_place(|| members.gateway(dir.path(), ("", &store)))?;
    assert!(declared(&app).await?.iter().any(|name| name == "patient"));
    let by_identifier = listed(&app, list(SUMMARY_TYPE)?).await?;
    let id = patient_id(&by_identifier)?;
    for reference in [format!("Patient%2F{id}"), id.clone()] {
        let by_id = listed(&app, search(&format!("patient={reference}{SUMMARY_TYPE}"))?).await?;
        assert_eq!(
            entries(&by_id, "match").len(),
            entries(&by_identifier, "match").len(),
            "the id resolves to the patient the identifier names: {by_id}"
        );
        assert_eq!(patient_id(&by_id)?, id);
    }
    let never = "a".repeat(52);
    unknown(&listed(&app, search(&format!("patient=Patient%2F{never}"))?).await?)?;
    executed(
        &server.plain_url("patient_id"),
        "UPDATE ferrofed.patient_id SET expires_at = now() - interval '1 second'",
    )
    .await?;
    members.proxy_a.clear_journal();
    members.proxy_b.clear_journal();
    unknown(&listed(&app, search(&format!("patient=Patient%2F{id}"))?).await?)?;
    assert!(
        members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty(),
        "an expired id asks no member"
    );
    Ok(())
}
