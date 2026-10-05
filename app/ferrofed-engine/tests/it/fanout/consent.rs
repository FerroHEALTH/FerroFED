// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node's own consent refusal (§11.1, §11.3, N27, N40).
//!
//! ITS-REST defines no consent signal, so a node's answer is `consent-denied`
//! only when it is a `403` whose ITS-REST `Error` carries a `code` the
//! registry lists among the endpoint's `consent_refusal_codes`; every other
//! refusal is `node-error`. A refusing node was dispatched to, so its record
//! carries the gateway's latency (N40), contributes no rows, and fails
//! nothing under either completion strategy (§11.3).

use std::error::Error;
use std::fmt::Write as _;

use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::fanout::{CONSENT_MEMBER, Completion, FederatedAnswer, Verdict};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;
use wiremock::ResponseTemplate;

use super::{TestResult, budget, json, node, plan_for, result_set, rows_text, run, validated_body};

/// The consent refusal code the registry lists for the refusing endpoint.
const REFUSAL_CODE: &str = "consent-refused";

/// A node's ITS-REST `Error` carrying `code`.
fn refusal(code: &str) -> String {
    format!(r#"{{"message":"synthetic refusal","validationErrors":[],"code":"{code}"}}"#)
}

/// A federation of node A at `a`, listing no refusal code, and node B at `b`,
/// listing [`REFUSAL_CODE`].
fn federation(a: &str, b: &str) -> Result<RegistrySnapshot, Box<dyn Error>> {
    let mut document = String::from("[[organisation]]\nid = \"org-a\"\n");
    for (index, (id, url, codes)) in [
        ("node-a-pub", a, String::new()),
        (
            "node-b-pub",
            b,
            format!("consent_refusal_codes = [\"{REFUSAL_CODE}\"]\n"),
        ),
    ]
    .iter()
    .enumerate()
    {
        write!(
            document,
            "\n[[node]]\nid = \"node-{index}\"\norganisation = \"org-a\"\nsystem_id = \"cdr-{index}.example.org\"\n\n[[endpoint]]\nid = \"{id}\"\nnode = \"node-{index}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n{codes}"
        )?;
    }
    Ok(RegistrySnapshot::from_toml_str(&document)?)
}

/// The record of `endpoint` in the answer.
fn record_of<'a>(
    answer: &'a FederatedAnswer,
    endpoint: &str,
) -> Result<&'a EndpointOutcome, String> {
    answer
        .federation()
        .endpoints()
        .iter()
        .find(|record| record.id().as_str() == endpoint)
        .ok_or_else(|| format!("{endpoint} is not reported"))
}

/// The answer when node A answers a row and node B answers `b_answer`,
/// under `completion`.
async fn answer_with(
    b_answer: ResponseTemplate,
    completion: Completion,
) -> Result<FederatedAnswer, Box<dyn Error>> {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let b = node(b_answer).await;
    let snapshot = federation(&a.uri(), &b.uri())?;
    let plan = plan_for(&["node-a-pub", "node-b-pub"])?.completing(completion);
    run(&snapshot, plan, budget(2_000, 5_000)?).await
}

/// The status node B was reported with when it answered `b_answer`.
async fn status_of_b(b_answer: ResponseTemplate) -> Result<EndpointStatus, Box<dyn Error>> {
    let answer = answer_with(b_answer, Completion::AllOrNothing).await?;
    Ok(record_of(&answer, "node-b-pub")?.status())
}

// conformance: CP-30
#[tokio::test]
async fn a_403_carrying_a_listed_code_is_consent_denied_and_fails_nothing() -> TestResult {
    for mode in [Completion::AllOrNothing, Completion::BestEffort] {
        let answer = answer_with(json(403, &refusal(REFUSAL_CODE)), mode).await?;
        assert_eq!(answer.verdict(), Verdict::Answered, "§11.3: {mode:?}");
        assert_eq!(answer.status(), StatusCode::OK, "never a 424: {mode:?}");
        assert!(!answer.federation().complete(), "§11.3 clears complete");
        assert_eq!(
            rows_text(&answer)?,
            r#"[["a1::cdr-0.example.org::1"]]"#,
            "node A's row only: a refusing node contributes none"
        );
        let record = record_of(&answer, "node-b-pub")?;
        let Outcome::ConsentDenied {
            refused_by: ConsentRefusal::Node { .. },
            error: Some(ErrorDetail::Text(error)),
        } = record.outcome()
        else {
            return Err(format!("not a node's consent refusal: {record:?}").into());
        };
        assert!(error.contains(REFUSAL_CODE), "{error}");
        assert!(error.contains("synthetic refusal"), "{error}");
        assert!(
            record.outcome().latency_ms().is_some(),
            "N40: the node was dispatched to, so its latency is reported"
        );
        let contacts: Vec<(String, Contact)> = answer
            .contacts()
            .map(|(endpoint, contact)| (endpoint.as_str().to_owned(), contact))
            .collect();
        assert!(
            contacts.contains(&(
                "node-b-pub".to_owned(),
                Contact::Answered(StatusCode::FORBIDDEN)
            )),
            "{contacts:?}"
        );
        validated_body(answer)?;
    }
    Ok(())
}

#[tokio::test]
async fn a_403_carrying_a_code_the_registry_does_not_list_is_a_node_error() -> TestResult {
    let answer = answer_with(
        json(403, &refusal("not-a-listed-code")),
        Completion::AllOrNothing,
    )
    .await?;
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY, "§11.4");
    assert_eq!(
        record_of(&answer, "node-b-pub")?.status(),
        EndpointStatus::NodeError
    );
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn a_403_with_no_its_rest_error_is_a_node_error() -> TestResult {
    assert_eq!(
        status_of_b(ResponseTemplate::new(403)).await?,
        EndpointStatus::NodeError,
        "a bare 403 is never read as consent"
    );
    assert_eq!(
        status_of_b(ResponseTemplate::new(403).set_body_string(REFUSAL_CODE)).await?,
        EndpointStatus::NodeError,
        "the code is read from an ITS-REST Error only, never from free text"
    );
    Ok(())
}

#[tokio::test]
async fn a_code_that_is_not_a_string_is_a_node_error() -> TestResult {
    let body =
        r#"{"message":"synthetic refusal","validationErrors":[],"code":["consent-refused"]}"#;
    assert_eq!(
        status_of_b(json(403, body)).await?,
        EndpointStatus::NodeError
    );
    Ok(())
}

#[tokio::test]
async fn a_listed_code_on_any_status_but_403_is_a_node_error() -> TestResult {
    for status in [401, 404, 500] {
        assert_eq!(
            status_of_b(json(status, &refusal(REFUSAL_CODE))).await?,
            EndpointStatus::NodeError,
            "a {status} is never a consent refusal"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_endpoint_listing_no_code_reports_every_403_as_a_node_error() -> TestResult {
    let a = node(json(403, &refusal(REFUSAL_CODE))).await;
    let b = node(json(200, &result_set(&["b1::cdr-1.example.org::1"]))).await;
    let snapshot = federation(&a.uri(), &b.uri())?;
    let plan = plan_for(&["node-a-pub", "node-b-pub"])?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(
        record_of(&answer, "node-a-pub")?.status(),
        EndpointStatus::NodeError,
        "the codes are per endpoint, and node A lists none"
    );
    assert_eq!(answer.status(), StatusCode::FAILED_DEPENDENCY);
    Ok(())
}

#[tokio::test]
async fn a_prefilter_outage_is_carried_in_meta_federation_and_changes_nothing_else() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let b = node(json(200, &result_set(&["b1::cdr-1.example.org::1"]))).await;
    let snapshot = federation(&a.uri(), &b.uri())?;
    let outage = ErrorDetail::text("the consent pre-filter could not answer: synthetic outage")?;
    let plan = plan_for(&["node-a-pub", "node-b-pub"])?.consent_unavailable(outage);
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert_eq!(
        StatusCode::OK,
        answer.status(),
        "every candidate was asked (N27)"
    );
    assert!(answer.federation().complete(), "both nodes answered");
    let carried = answer
        .federation()
        .extra()
        .get(CONSENT_MEMBER)
        .ok_or("meta.federation carries the pre-filter's failure")?
        .get()
        .to_owned();
    assert_eq!(
        r#"{"error":"the consent pre-filter could not answer: synthetic outage"}"#,
        carried
    );
    validated_body(answer)?;
    Ok(())
}

#[tokio::test]
async fn a_prefilter_that_answered_leaves_no_consent_member() -> TestResult {
    let a = node(json(200, &result_set(&["a1::cdr-0.example.org::1"]))).await;
    let b = node(json(200, &result_set(&["b1::cdr-1.example.org::1"]))).await;
    let snapshot = federation(&a.uri(), &b.uri())?;
    let plan = plan_for(&["node-a-pub", "node-b-pub"])?;
    let answer = run(&snapshot, plan, budget(2_000, 5_000)?).await?;
    assert!(answer.federation().extra().get(CONSENT_MEMBER).is_none());
    Ok(())
}
