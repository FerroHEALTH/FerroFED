// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query over two FerroEHR nodes, the patient resolved through
//! the harness PIX Manager over ITI-83: only a member that knows the patient
//! is asked, by its own `ehr_id` (§11.1; N7, N8, N33).

use std::error::Error;

use ferrofed_testkit::containers;
use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::seed::{self, CrossReferenceSeed, DemoComposition, EhrDomain, PatientId};
use http::StatusCode;
use uuid::Uuid;

use crate::e2e::{
    Answer, EHR_A, EHR_B, PATIENT, TestResult, assert_no_patient_identifier_on_the_wire,
    body_holds, gateway_resolving, patient_query, plan, query,
};
use crate::support::call;

/// The `ehr_id` domains of node A and node B at the harness PIX Manager.
pub(crate) const DOMAIN_A: EhrDomain = EhrDomain::new(1);
pub(crate) const DOMAIN_B: EhrDomain = EhrDomain::new(2);

/// The two nodes, seeded, and the harness PIX Manager fed over ITI-104 with
/// the patient at `ehrs`, plus an unrelated patient so both domains are known.
pub(crate) async fn nodes_and_pix(
    ehrs: Vec<(EhrDomain, Uuid)>,
) -> Result<(containers::TwoNodes, PixManager), Box<dyn Error>> {
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
    let pix = PixManager::start().await?;
    let fed = [
        CrossReferenceSeed {
            patient: PATIENT,
            ehrs,
        },
        CrossReferenceSeed {
            patient: PatientId::new(1, 39),
            ehrs: vec![
                (
                    DOMAIN_A,
                    Uuid::from_u128(0x5555_5555_5555_4555_8555_5555_5555_5555),
                ),
                (
                    DOMAIN_B,
                    Uuid::from_u128(0x6666_6666_6666_4666_8666_6666_6666_6666),
                ),
            ],
        },
    ];
    for cross_reference in &fed {
        let status = seed::feed(&pix.base_url(), cross_reference).await?;
        assert_eq!(StatusCode::CREATED, status, "ITI-104 creates the Patient");
    }
    Ok((nodes, pix))
}

/// The resolver configuration over the harness PIX Manager, under the
/// development profile, the only one that admits its plain `http`.
pub(crate) fn pixm_resolver(pix: &PixManager) -> String {
    pixm_resolver_at(&pix.base_url())
}

/// The resolver configuration of [`pixm_resolver`], the Manager reached at
/// `url`, for a scenario that puts a capturing proxy in front of it.
pub(crate) fn pixm_resolver_at(url: &str) -> String {
    format!(
        "profile = \"development\"\n\n[[pixm.manager]]\nurl = \"{url}\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n",
        DOMAIN_A.system(),
        DOMAIN_B.system()
    )
}

// conformance: CP-3 CP-36 track-2
#[tokio::test]
async fn a_pix_resolved_query_asks_only_the_member_that_knows_the_patient() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A)])).await?;
    let resolver = pixm_resolver(&pix);
    let dir = tempfile::tempdir()?;

    let patient = patient_query();
    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        !answer.meta.federation.complete,
        "a not-resolved member leaves the answer incomplete (§11.1)"
    );
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "not-resolved", None),
        ],
        reported,
        "{text}"
    );
    assert_eq!(1, answer.rows.len(), "the rows of node A alone: {text}");

    let journal = nodes.a.proxy.journal();
    let sent = journal.first().ok_or("node A was asked")?;
    assert!(
        body_holds(sent, &EHR_A.to_string()),
        "node A is asked by its own ehr_id (N7)"
    );
    assert!(
        nodes.b.proxy.journal().is_empty(),
        "node B, where the patient is not known, is never asked (N8)"
    );
    assert_no_patient_identifier_on_the_wire(&nodes);
    assert_eq!(1, pix.queries(), "one ITI-83 call for both members");
    Ok(())
}

// conformance: CP-3 track-2
#[tokio::test]
async fn a_patient_fed_at_both_members_resolves_through_pix_at_both() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)])).await?;
    let resolver = pixm_resolver(&pix);
    let dir = tempfile::tempdir()?;

    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient_query())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer.meta.federation.complete,
        "both members resolved: {text}"
    );
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "active", Some(1)),
        ],
        reported,
        "{text}"
    );
    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let journal = node.proxy.journal();
        let sent = journal.first().ok_or("each node was asked")?;
        assert!(
            body_holds(sent, &own.to_string()) && !body_holds(sent, &other.to_string()),
            "{} is asked by its own ehr_id alone (N7)",
            node.node.system_id()
        );
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    assert_eq!(1, pix.queries(), "one ITI-83 call for both members");
    Ok(())
}
