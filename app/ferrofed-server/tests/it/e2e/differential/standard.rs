// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The differential run over the standard seed: the patient's EHR at both
//! nodes, the hospital's composition at node A and the clinic's at node B.
//!
//! The steps are the Connectathon scenarios of tracks 1, 2, 3, 4, 6, 7 and 9
//! that both gateways can be configured for alike (§16.3). The read of an
//! `ehr_id` neither gateway has seen comes first, and the writes come last,
//! so every query step reads the same data at both gateways.

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::containers::{self, ProxiedNode};
use ferrofed_testkit::proxy::Fault;
use http::{Method, StatusCode};
use openehr_federation::headers::{COMPLETENESS, COMPLETENESS_PARTIAL, ENDPOINT};
use serde::Deserialize;
use uuid::Uuid;

use crate::e2e::differential::observe::Shape;
use crate::e2e::differential::{
    AT_A_ONLY, Bench, Call, Faulted, NOWHERE, PER_NODE, REFUSAL, Step, differ,
};
use crate::e2e::scenario::{patient_predicate, seed_both};
use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult, composition_carrying};

/// An `ehr_id` no member holds.
const UNKNOWN_EHR: Uuid = Uuid::from_u128(0x9999_9999_9999_4999_8999_9999_9999_9999);

/// Returns a step with no fault.
fn step(
    id: &'static str,
    track: &'static str,
    what: &'static str,
    call: Call,
    shape: Shape,
) -> Step {
    Step {
        id,
        track,
        what,
        call,
        shape,
        fault: None,
    }
}

/// Returns `step` with `fault` set on `node`.
fn faulted(mut step: Step, node: Faulted, fault: Fault) -> Step {
    step.fault = Some((node, fault));
    step
}

/// The patient's compositions, each uid aliased.
fn compositions() -> String {
    format!(
        "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    )
}

/// The compositions of `patient`, from the members `directive` selects.
fn directed(directive: &str, patient: ferrofed_testkit::seed::PatientId) -> String {
    format!(
        "SELECT c/uid/value AS uid FROM {directive} CONTAINS EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        patient.value(),
        patient.namespace()
    )
}

/// The routing steps no earlier query informs, and the self-description.
fn unseen() -> Vec<Step> {
    vec![
        step(
            "t6-unseen-ehr-read",
            "6",
            "a read of an ehr_id the gateway has not seen is found by a read-only probe",
            Call::bare(Method::GET, format!("/v1/ehr/{EHR_B}")),
            Shape::Passthrough,
        ),
        step(
            "t6-unrouted-write",
            "6",
            "a write no target, binding or index routes is refused and never probed",
            Call::bare(Method::POST, format!("/v1/ehr/{UNKNOWN_EHR}/composition")).with_body("{}"),
            Shape::Passthrough,
        ),
        step(
            "t6-untargeted-create",
            "6",
            "a new EHR that names no node is refused",
            Call::bare(Method::POST, "/v1/ehr"),
            Shape::Passthrough,
        ),
        step(
            "t9-options",
            "9",
            "the self-description at OPTIONS {base}/",
            Call::bare(Method::OPTIONS, "/"),
            Shape::Options,
        ),
    ]
}

/// The steps of tracks 1 and 2: transparency and the two subject carriers.
fn carriers() -> Result<Vec<Step>, Box<dyn Error>> {
    let (namespace, value) = (PATIENT.namespace(), PATIENT.value());
    let observations = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
    let encoded = crate::query_get::encoded(&compositions());
    Ok(vec![
        step(
            "t1-post",
            "1",
            "a plain patient query by POST",
            Call::aql(&compositions())?,
            Shape::Rows,
        ),
        step(
            "t1-get",
            "1",
            "the same query by the ITS-REST GET form",
            Call::bare(Method::GET, format!("/v1/query/aql?q={encoded}")),
            Shape::Rows,
        ),
        step(
            "t2-external-ref-carrier",
            "2",
            "the patient through EHR_STATUS.subject.external_ref, over observations",
            Call::aql(&format!(
                "SELECT c/uid/value {observations} WHERE {}",
                patient_predicate()
            ))?,
            Shape::Rows,
        ),
        step(
            "t2-entry-carrier",
            "2",
            "the patient through an ENTRY-level subject, over observations",
            Call::aql(&format!(
                "SELECT c/uid/value {observations} WHERE o/subject/identifiers/id = '{value}' \
                 AND o/subject/identifiers/issuer = '{namespace}'"
            ))?,
            Shape::Rows,
        ),
        step(
            "t2-subject-column",
            "2",
            "a selected subject column is the resolution input, never asked of a node",
            Call::aql(&format!(
                "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value AS uid \
                 FROM EHR e CONTAINS COMPOSITION c WHERE {}",
                patient_predicate()
            ))?,
            Shape::Rows,
        ),
    ])
}

/// The steps of track 3: the directive, the header and their conflict.
fn targeting() -> Result<Vec<Step>, Box<dyn Error>> {
    Ok(vec![
        step(
            "t3-endpoint-directive",
            "3",
            "FROM ENDPOINT selects node A and reports node B excluded",
            Call::aql(&directed(r#"ENDPOINT ["node-a-pub"]"#, PATIENT))?,
            Shape::Rows,
        ),
        step(
            "t3-organisation-directive",
            "3",
            "FROM ORGANISATION selects the organisation's endpoints",
            Call::aql(&directed(r#"ORGANISATION ["org-b"]"#, PATIENT))?,
            Shape::Rows,
        ),
        step(
            "t3-endpoint-header",
            "3",
            "the endpoint header selects what the directive selects",
            Call::aql(&compositions())?.with(ENDPOINT, "node-b-pub"),
            Shape::Rows,
        ),
        step(
            "t3-conflicting-targets",
            "3",
            "a directive and a header naming different node sets",
            Call::aql(&directed(r#"ENDPOINT ["node-b-pub"]"#, PATIENT))?
                .with(ENDPOINT, "node-a-pub"),
            Shape::Rows,
        ),
        step(
            "t3-endpoint-parameter",
            "3",
            "a ?endpoint= query parameter is not a targeting mechanism",
            Call {
                target: "/v1/query/aql?endpoint=node-b-pub".to_owned(),
                ..Call::aql(&compositions())?
            },
            Shape::Rows,
        ),
        step(
            "t3-named-member-unresolved",
            "3",
            "a named member where the patient does not resolve is reported, not errored",
            Call::aql(&directed(
                r#"ENDPOINT ["node-a-pub", "node-b-pub"]"#,
                AT_A_ONLY,
            ))?,
            Shape::Rows,
        ),
    ])
}

/// The steps of track 4: a failing node under each completeness mode.
fn failures() -> Result<Vec<Step>, Box<dyn Error>> {
    Ok(vec![
        faulted(
            step(
                "t4-node-error",
                "4",
                "node B answers 500",
                Call::aql(&compositions())?,
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Status(StatusCode::INTERNAL_SERVER_ERROR),
        ),
        faulted(
            step(
                "t4-offline",
                "4",
                "node B refuses the connection",
                Call::aql(&compositions())?,
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Refuse,
        ),
        faulted(
            step(
                "t4-time-out",
                "4",
                "node B answers after the per-node timeout",
                Call::aql(&compositions())?,
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Delay(PER_NODE + Duration::from_secs(3)),
        ),
        faulted(
            step(
                "t4-prefer-wait",
                "4",
                "a client wait shorter than the budget, with node B slow",
                Call::aql(&compositions())?.with("prefer", "wait=2"),
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Delay(PER_NODE + Duration::from_secs(3)),
        ),
        faulted(
            step(
                "t4-partial",
                "4",
                "best-effort requested, with node B answering 500",
                Call::aql(&compositions())?.with(COMPLETENESS, COMPLETENESS_PARTIAL),
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Status(StatusCode::INTERNAL_SERVER_ERROR),
        ),
        step(
            "t4-found-nowhere",
            "4",
            "a patient resolved at no member",
            Call::aql(&format!(
                "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c \
                 WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
                 AND e/ehr_status/subject/external_ref/namespace = '{}'",
                NOWHERE.value(),
                NOWHERE.namespace()
            ))?,
            Shape::Rows,
        ),
    ])
}

/// The steps of tracks 7 and 9: a node's consent refusal, and one EHR
/// named by its `ehr_id` in both forms.
fn consent_and_ehr_forms() -> Result<Vec<Step>, Box<dyn Error>> {
    Ok(vec![
        step(
            "t7-anonymous-caller",
            "7",
            "a caller that presents no credential",
            Call::aql(&compositions())?.anonymous(),
            Shape::Rows,
        ),
        faulted(
            step(
                "t7-node-consent-refusal",
                "7",
                "node B refuses with a consent refusal, no consent service configured",
                Call::aql(&compositions())?,
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Reply(StatusCode::FORBIDDEN, REFUSAL),
        ),
        faulted(
            step(
                "t7-directed-consent-refusal",
                "7",
                "a query directed to node B, which refuses with a consent refusal",
                Call::aql(&directed(r#"ENDPOINT ["node-b-pub"]"#, PATIENT))?,
                Shape::Rows,
            ),
            Faulted::B,
            Fault::Reply(StatusCode::FORBIDDEN, REFUSAL),
        ),
        step(
            "t9-ehr-id-where-form",
            "9",
            "one EHR named by ehr_id in a WHERE predicate",
            Call::aql(&format!(
                "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_A}'"
            ))?,
            Shape::Rows,
        ),
        step(
            "t9-ehr-id-from-form",
            "9",
            "the same EHR named in the FROM EHR predicate",
            Call::aql(&format!(
                "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_A}'] CONTAINS COMPOSITION c"
            ))?,
            Shape::Rows,
        ),
    ])
}

/// The follow-up reads and the writes, which come after every query.
fn follow_ups(uid: &str) -> Result<Vec<Step>, Box<dyn Error>> {
    Ok(vec![
        step(
            "t6-follow-up-composition",
            "6",
            "a follow-up read of node A's composition by its uid",
            Call::bare(Method::GET, format!("/v1/ehr/{EHR_A}/composition/{uid}")),
            Shape::Passthrough,
        ),
        step(
            "t6-follow-up-ehr",
            "6",
            "a follow-up read of node A's EHR",
            Call::bare(Method::GET, format!("/v1/ehr/{EHR_A}")),
            Shape::Passthrough,
        ),
        step(
            "t6-targeted-create",
            "6",
            "a new EHR at the one node the client names",
            Call::bare(Method::POST, "/v1/ehr").with(ENDPOINT, "node-b-pub"),
            Shape::Passthrough,
        ),
        step(
            "t9-commit",
            "9",
            "a composition committed to node A's EHR, its body carrying a DV_IDENTIFIER",
            Call::bare(Method::POST, format!("/v1/ehr/{EHR_A}/composition"))
                .with_body(composition_carrying(PATIENT)?),
            Shape::Passthrough,
        ),
    ])
}

/// The rows of an ITS-REST result set whose cells are text.
#[derive(Debug, Deserialize)]
struct Rows {
    rows: Vec<Vec<String>>,
}

/// Returns the uid of the one composition in `ehr_id` at `node`, asked of
/// the node directly.
async fn composition_uid(node: &ProxiedNode, ehr_id: Uuid) -> Result<String, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Adhoc {
        q: String,
    }
    let answer = reqwest::Client::new()
        .post(format!("{}/v1/query/aql", node.node.api_root()))
        .json(&Adhoc {
            q: format!(
                "SELECT c/uid/value FROM EHR e[ehr_id/value='{ehr_id}'] CONTAINS COMPOSITION c"
            ),
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Rows>()
        .await?;
    answer
        .rows
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "node A holds the seeded composition".into())
}

#[tokio::test]
async fn the_standard_seed_differs_only_as_adjudicated() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let uid = composition_uid(&nodes.a, EHR_A).await?;
    let bench = Box::pin(Bench::start(nodes)).await?;
    let mut steps = unseen();
    steps.extend(carriers()?);
    steps.extend(targeting()?);
    steps.extend(failures()?);
    steps.extend(consent_and_ehr_forms()?);
    steps.extend(follow_ups(&uid)?);
    Box::pin(differ(&bench, "standard", steps)).await
}
