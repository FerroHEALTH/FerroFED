// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness Patient Identity Registry, held to the PMIR subscriber and
//! feed reader of `ihe-iti`: a subscription is created, read and deleted as
//! ITI-94 lays out, and the message it writes reads as one merge
//! (§2:3.94.4, §2:3.93.4.1.2.4).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::time::Duration;

use ferrofed_testkit::pmir::{PatientIdentityRegistry, merge_message};
use http::StatusCode;
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::feed::{Event, Feed};
use ihe_iti::pmir::subscription::{Criteria, SubscriptionRequest, SubscriptionStatus};
use secrecy::ExposeSecret;
use url::Url;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROMPT: Duration = Duration::from_secs(5);

fn subscriber(
    registry: &PatientIdentityRegistry,
) -> Result<PmirSubscriber, Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(PmirSubscriber::new(
        Url::parse(&registry.base_url())?,
        http,
    )?)
}

fn request() -> Result<SubscriptionRequest, Box<dyn std::error::Error>> {
    Ok(SubscriptionRequest::new(
        Criteria::identifier_system("urn:oid:2.999.1.900")?,
        Url::parse("http://127.0.0.1:9/pmir/feed")?,
    )?)
}

#[tokio::test]
async fn a_subscription_is_created_read_and_deleted() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let client = subscriber(&registry)?;
    let subscribed = client.subscribe(&request()?, PROMPT).await?;
    let [held] = registry
        .subscriptions()
        .try_into()
        .map_err(|_held| "one subscription")?;
    assert_eq!("Patient?identifier=urn:oid:2.999.1.900|", held.criteria);
    assert_eq!("application/fhir+json", held.payload);
    assert_eq!(
        SubscriptionStatus::Active,
        client.status(&subscribed, PROMPT).await?
    );
    registry.set_status("error");
    assert_eq!(
        SubscriptionStatus::Error,
        client.status(&subscribed, PROMPT).await?
    );
    client.unsubscribe(&subscribed, PROMPT).await?;
    assert_eq!(1, registry.deleted());
    assert!(registry.subscriptions().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_refusing_registry_answers_the_status_it_was_given() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.refuse_subscriptions(Some(StatusCode::FORBIDDEN));
    let error = subscriber(&registry)?
        .subscribe(&request()?, PROMPT)
        .await
        .err()
        .ok_or("refused")?;
    assert_eq!(Some(StatusCode::FORBIDDEN), error.status());
    Ok(())
}

#[test]
fn the_merge_message_reads_as_one_merge() -> TestResult {
    let body = merge_message(
        "patient-old",
        &[(
            "urn:oid:2.999.1.900",
            "7a7a7a7a-7a7a-4a7a-8a7a-7a7a7a7a7a7a",
        )],
        "patient-new",
    )?;
    let feed = Feed::read(Some("application/fhir+json"), body.as_bytes())?;
    let [
        Event::Merged {
            subsumed,
            surviving,
        },
    ] = feed.events()
    else {
        return Err("one merge".into());
    };
    assert_eq!("Patient/patient-new", surviving.expose_secret());
    assert_eq!(1, subsumed.identifiers().len());
    Ok(())
}
