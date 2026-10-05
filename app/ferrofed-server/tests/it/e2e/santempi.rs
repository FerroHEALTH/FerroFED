// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query over two FerroEHR nodes, the patient resolved through
//! a deployable PIX Manager, SanteMPI, over ITI-83 (PIXm 3.1.0 §2:3.83;
//! §5.2, N3, N7, N33).
//!
//! Each member's own feed registers the patient at the Manager with the
//! member's `ehr_id` in the member's `ehr_id` domain, by one PMIR ITI-93
//! message (PMIR 1.6.0 §2:3.93), the way the identity page of the book
//! tells an operator to keep a Manager current.

use std::error::Error;

use ferrofed_testkit::containers::{self, santempi};
use ferrofed_testkit::seed::{self, DemoComposition, PatientId};
use http::StatusCode;

use crate::e2e::pixm::{DOMAIN_A, DOMAIN_B};
use crate::e2e::{
    Answer, EHR_A, EHR_B, PATIENT, TestResult, assert_no_patient_identifier_on_the_wire,
    body_holds, gateway_resolving, patient_query, plan, query,
};
use crate::support::call;

/// The two nodes, each seeded with the patient's EHR, their journals cleared.
async fn seeded_nodes() -> Result<containers::TwoNodes, Box<dyn Error>> {
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    Ok(nodes)
}

/// The resolver configuration over SanteMPI at `url`, asked with the bearer
/// token `token`, under the development profile, the only one that admits
/// the harness's plain `http`.
fn santempi_resolver(url: &str, token: &str) -> String {
    format!(
        "profile = \"development\"\n\n[[pixm.manager]]\nurl = {}\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n\n[pixm.manager.credentials]\nbearer_token = {}\n",
        toml::Value::String(url.to_owned()),
        DOMAIN_A.system(),
        DOMAIN_B.system(),
        toml::Value::String(token.to_owned()),
    )
}

/// The patient query of [`patient_query`], for `patient`.
fn query_for(patient: PatientId) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        patient.value(),
        patient.namespace()
    )
}

/// What the answer reports for each endpoint: its id, status and row count.
fn reported(answer: &Answer) -> Vec<(&str, &str, Option<u64>)> {
    answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect()
}

// conformance: CP-3 track-2
#[tokio::test]
async fn a_patient_each_member_fed_to_santempi_is_answered_by_both() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let namespace = PATIENT.namespace();
    let members = [DOMAIN_A, DOMAIN_B];
    let (nodes, mpi) = tokio::join!(
        Box::pin(seeded_nodes()),
        Box::pin(santempi::santempi(&namespace, &members))
    );
    let (nodes, mpi) = (nodes?, mpi?);
    mpi.feed(DOMAIN_A, PATIENT, EHR_A).await?;
    mpi.feed(DOMAIN_B, PATIENT, EHR_B).await?;
    let token = mpi.token(&santempi::consumer()).await?;
    let resolver = santempi_resolver(&mpi.fhir_base(), &token);
    let dir = tempfile::tempdir()?;

    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient_query())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer.meta.federation.complete,
        "both members resolved through SanteMPI: {text}"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "active", Some(1)),
        ],
        reported(&answer),
        "{text}"
    );
    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let journal = node.proxy.journal();
        let sent = journal.first().ok_or("each node was asked")?;
        assert!(
            body_holds(sent, &own.to_string()) && !body_holds(sent, &other.to_string()),
            "{} is asked by the ehr_id its own feed registered (N7)",
            node.node.system_id()
        );
    }
    assert_no_patient_identifier_on_the_wire(&nodes);

    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let unknown = PatientId::new(1, 39);
    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&query_for(unknown))?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        !answer.meta.federation.complete,
        "a patient no member fed is unknown at both (N6): {text}"
    );
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved", None),
            ("node-b-pub", "not-resolved", None),
        ],
        reported(&answer),
        "{text}"
    );
    assert!(
        nodes.a.proxy.journal().is_empty() && nodes.b.proxy.journal().is_empty(),
        "no member is asked about a patient the Manager does not know (N8)"
    );
    Ok(())
}
