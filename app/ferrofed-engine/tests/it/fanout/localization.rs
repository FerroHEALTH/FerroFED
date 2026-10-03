// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A plan that localization left empty: no destination is not the same as no
//! node set, nothing is sent, `complete` stays true, and a localizer's
//! failure is carried in `meta.federation` as well as on each endpoint
//! (§11.1, §11.4, §14.1, N4, N37).

use http::StatusCode;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;

use ferrofed_engine::fanout::{LOCALIZATION_MEMBER, Plan};
use ferrofed_registry::id::EndpointId;

use super::{TestResult, budget, federation, run, statuses, validated_body};

/// The failure a localizer that did not answer leaves on every member.
fn failure() -> ErrorDetail {
    ErrorDetail::Text("the localizer could not answer: synthetic outage".to_owned())
}

/// A plan in which localization named neither endpoint.
fn not_localized(error: Option<ErrorDetail>) -> Result<Plan, Box<dyn std::error::Error>> {
    let mut plan = Plan::new();
    for id in ["node_a", "node_b"] {
        plan = plan.settle(
            EndpointId::new(id)?,
            Outcome::NotLocalized {
                error: error.clone(),
            },
        )?;
    }
    Ok(plan)
}

// conformance: CP-5
#[tokio::test]
async fn a_localizer_failure_is_carried_in_meta_federation_and_on_every_endpoint() -> TestResult {
    let snapshot = federation(&[
        ("node_a", "http://127.0.0.1:9/"),
        ("node_b", "http://127.0.0.1:10/"),
    ])?;
    let plan = not_localized(Some(failure()))?.localization_failed(failure());
    assert!(
        !plan.has_no_destination(),
        "an empty candidate set is no 404 (§14.1)"
    );

    let answer = run(&snapshot, plan, budget(1_000, 2_000)?).await?;
    assert_eq!(
        StatusCode::OK,
        answer.status(),
        "§14.1: the status cannot carry it"
    );
    assert!(answer.federation().complete(), "§14.1: complete stays true");
    assert!(
        statuses(&answer)
            .values()
            .all(|status| *status == EndpointStatus::NotLocalized)
    );
    let carried = answer
        .federation()
        .extra()
        .get(LOCALIZATION_MEMBER)
        .ok_or("meta.federation carries the failure (§14.1 SHOULD)")?
        .get()
        .to_owned();
    assert_eq!(
        r#"{"error":"the localizer could not answer: synthetic outage"}"#,
        carried
    );
    validated_body(answer)?;
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_localizer_that_answered_leaves_no_localization_member() -> TestResult {
    let snapshot = federation(&[
        ("node_a", "http://127.0.0.1:9/"),
        ("node_b", "http://127.0.0.1:10/"),
    ])?;
    let answer = run(&snapshot, not_localized(None)?, budget(1_000, 2_000)?).await?;
    assert!(
        answer
            .federation()
            .extra()
            .get(LOCALIZATION_MEMBER)
            .is_none()
    );
    validated_body(answer)?;
    Ok(())
}
