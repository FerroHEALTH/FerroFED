// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A restriction of processing set at one FerroEHR node is honoured across
//! the federation boundary: the federated query returns the unrestricted
//! composition of that node and leaves out the restricted one, and a
//! follow-up read of the restricted one through the gateway answers the
//! node's own refusal (§13.2, N8, N27; §12.5, N22; CP-36).
//!
//! The consent decision stays with the node (§13.2): the gateway learns
//! nothing of the mark and decides nothing, so what the node withholds never
//! reaches the merged answer. The mark is FerroEHR's ADMIN API extension
//! (GDPR Art. 18), set by the harness as the node's operator would set it.

use ferrofed_testkit::containers::{self, marks};
use ferrofed_testkit::seed::{self, CompositionSeed, DemoComposition, EhrSeed, SeedPlan};
use http::{Request, StatusCode};
use uuid::Uuid;

use crate::e2e::{Answer, EHR_A, EHR_B, PATIENT, TestResult, gateway, patient_query, plan, query};
use crate::support::call;

/// The versioned object and the version uid of the `index`th composition a
/// seed committed.
fn committed(
    report: &seed::SeedReport,
    index: usize,
) -> Result<(Uuid, String), Box<dyn std::error::Error>> {
    let uid = report
        .compositions
        .get(index)
        .and_then(|committed| committed.version_uid.clone())
        .ok_or("the node named each committed version in its ETag")?;
    let object = Uuid::try_parse(uid.split("::").next().unwrap_or_default())?;
    Ok((object, uid))
}

// conformance: CP-36
#[tokio::test]
async fn a_restriction_at_one_node_is_honoured_by_the_federated_query() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = marks::two_nodes().await?;
    let at_a = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR_A,
            subject: Some(PATIENT),
        }],
        template: true,
        compositions: vec![
            CompositionSeed {
                ehr_id: EHR_A,
                composition: DemoComposition::FirstHospital,
            },
            CompositionSeed {
                ehr_id: EHR_A,
                composition: DemoComposition::SecondHospital,
            },
        ],
    };
    let report = seed::seed(&nodes.a.api_root(), &at_a).await?;
    let (_, served) = committed(&report, 0)?;
    let (restricted, restricted_uid) = committed(&report, 1)?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    marks::restrict(&nodes.a.api_root(), EHR_A, restricted).await?;
    let dir = tempfile::tempdir()?;

    let (status, text) = call(
        gateway(dir.path(), &nodes.a, &nodes.b)?,
        query(&patient_query())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer.meta.federation.complete,
        "both nodes answered: {text}"
    );
    let rows: Vec<String> = answer.rows.into_iter().flatten().collect();
    assert_eq!(2, rows.len(), "one composition from each node: {rows:?}");
    assert!(
        rows.contains(&served),
        "node A's unrestricted composition: {rows:?}"
    );
    assert!(
        !text.contains(&restricted.to_string()),
        "the restricted composition leaves the merged answer: {text}"
    );

    let read = |uid: &str| {
        Request::get(format!("/v1/ehr/{EHR_A}/composition/{uid}"))
            .header(http::header::ACCEPT, "application/json")
            .body(axum::body::Body::empty())
    };
    let (status, text) = call(gateway(dir.path(), &nodes.a, &nodes.b)?, read(&served)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the unrestricted composition is served: {text}"
    );
    let (status, text) = call(
        gateway(dir.path(), &nodes.a, &nodes.b)?,
        read(&restricted_uid)?,
    )
    .await?;
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "the node's refusal of the restricted composition reaches the client: {text}"
    );
    Ok(())
}
