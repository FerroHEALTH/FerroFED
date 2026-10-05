// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-94 subscription against the harness Patient Identity Registry:
//! created as the request profile asks, shown on `/health/dependencies` as
//! `identity_registry` whether the Registry holds it, refuses it or cannot be
//! reached, created again when the Registry loses it, deleted on request, and
//! the feed it brings applied end to end (PMIR §2:3.94.4, §2:3.93.4).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use ferrofed_registry::health::Observed;
use ferrofed_server::binding::ihe::pmir::IdentityFeed;
use ferrofed_server::binding::ihe::pmir::subscription::{RegistryFault, backoff};
use ferrofed_testkit::pmir::{LocationMode, PatientIdentityRegistry, merge_message};
use http::{Request, StatusCode};
use serde::Deserialize;
use tokio::net::TcpListener;

use super::{DOMAIN_A, EHR_A, EHR_A2, Gateway, TOKEN, gateway, text};
use crate::support::send_as_is;

type TestResult = Result<(), Box<dyn Error>>;

/// A gateway subscribing at `registry_url`, its feed sent to `callback`.
fn subscribing(
    registry_url: &str,
    callback: &str,
    extra: &str,
) -> Result<(tempfile::TempDir, Gateway), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let gateway = gateway(&text(dir.path(), registry_url, callback, extra)?)?;
    Ok((dir, gateway))
}

fn feed(gateway: &Gateway) -> Result<&Arc<IdentityFeed>, Box<dyn Error>> {
    Ok(gateway.state.identity_feed().ok_or("[pmir] is set")?)
}

/// What `GET /health/dependencies` reports of the Registry.
async fn reported(gateway: &Gateway) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        identity_registry: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let response = send_as_is(gateway.app.clone(), request).await?;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice::<Report>(&bytes)?.identity_registry)
}

#[tokio::test]
async fn the_gateway_subscribes_as_the_request_profile_asks() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let callback = "http://127.0.0.1:9/pmir/feed";
    let (_dir, gateway) = subscribing(&registry.base_url(), callback, "")?;
    assert_eq!(Some("unknown"), reported(&gateway).await?.as_deref());
    assert_eq!(Observed::Up, feed(&gateway)?.check().await);
    let [held] = <[_; 1]>::try_from(registry.subscriptions()).map_err(|_held| "one")?;
    assert_eq!(callback, held.endpoint);
    assert_eq!("application/fhir+json", held.payload);
    assert_eq!("Patient", held.criteria);
    assert_eq!(Some("up"), reported(&gateway).await?.as_deref());
    assert_eq!(
        Observed::Up,
        feed(&gateway)?.check().await,
        "read, not created again"
    );
    assert_eq!(1, registry.subscriptions().len());
    Ok(())
}

#[tokio::test]
async fn the_feed_can_be_limited_to_an_identifier_system() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let (_dir, gateway) = subscribing(
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        &format!("identifier_system = \"{DOMAIN_A}\"\n"),
    )?;
    feed(&gateway)?.check().await;
    let [held] = <[_; 1]>::try_from(registry.subscriptions()).map_err(|_held| "one")?;
    assert_eq!(format!("Patient?identifier={DOMAIN_A}|"), held.criteria);
    Ok(())
}

#[tokio::test]
async fn an_unreachable_registry_shows_down() -> TestResult {
    let (_dir, gateway) = subscribing(
        "http://127.0.0.1:0/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        "",
    )?;
    assert_eq!(Observed::Down, feed(&gateway)?.check().await);
    assert_eq!(Some("down"), reported(&gateway).await?.as_deref());
    Ok(())
}

#[tokio::test]
async fn a_refused_subscription_shows_failing() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.refuse_subscriptions(Some(StatusCode::FORBIDDEN));
    let (_dir, gateway) = subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
    assert_eq!(Observed::Failing, feed(&gateway)?.check().await);
    assert_eq!(Some("failing"), reported(&gateway).await?.as_deref());
    Ok(())
}

#[tokio::test]
async fn a_subscription_in_error_or_off_is_deleted_and_replaced() -> TestResult {
    for status in ["error", "off"] {
        let registry = PatientIdentityRegistry::start().await?;
        let (_dir, gateway) =
            subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
        let feed = feed(&gateway)?;
        assert_eq!(Observed::Up, feed.check().await);
        registry.set_status(status);
        assert_eq!(
            Observed::Up,
            feed.check().await,
            "{status} is replaced (§2:3.94.4.1.3, §2:3.94.4.4)"
        );
        assert_eq!(1, registry.deleted(), "{status}: the old one is deleted");
        assert_eq!(2, registry.creates(), "{status}: a new one is created");
        let [held] = <[_; 1]>::try_from(registry.subscriptions()).map_err(|_held| "one")?;
        assert_eq!("active", held.status, "{status}");
    }
    Ok(())
}

#[tokio::test]
async fn a_lost_subscription_is_created_again() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let (_dir, gateway) = subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
    let feed = feed(&gateway)?;
    assert_eq!(Observed::Up, feed.check().await);
    registry.forget_subscriptions();
    assert_eq!(Observed::Up, feed.check().await, "created again");
    assert_eq!(1, registry.subscriptions().len());
    assert_eq!(2, registry.creates());
    Ok(())
}

#[tokio::test]
async fn a_create_answered_late_is_adopted_and_not_made_twice() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.delay_creates(Some(Duration::from_millis(600)));
    let (_dir, gateway) = subscribing(
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        "timeout_ms = 200\n",
    )?;
    let feed = feed(&gateway)?;
    assert_eq!(
        Observed::Down,
        feed.check().await,
        "the answer came too late"
    );
    assert_eq!(
        1,
        registry.subscriptions().len(),
        "the Registry made it anyway"
    );
    registry.delay_creates(None);
    assert_eq!(Observed::Up, feed.check().await, "the search finds it");
    assert_eq!(1, registry.creates(), "no second subscription is created");
    assert_eq!(1, registry.subscriptions().len());
    Ok(())
}

#[tokio::test]
async fn without_the_search_the_gateway_creates() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.refuse_search(true);
    let (_dir, gateway) = subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
    assert_eq!(Observed::Up, feed(&gateway)?.check().await);
    assert_eq!(1, registry.creates());
    Ok(())
}

#[tokio::test]
async fn an_unusable_location_stops_every_later_create() -> TestResult {
    for mode in [LocationMode::Missing, LocationMode::Elsewhere] {
        let registry = PatientIdentityRegistry::start().await?;
        registry.locate_creates(mode);
        let (_dir, gateway) =
            subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
        let feed = feed(&gateway)?;
        for _ in 0..3 {
            assert_eq!(Observed::Failing, feed.check().await, "{mode:?}");
        }
        assert_eq!(1, registry.creates(), "{mode:?}: created once, never again");
        assert_eq!(Some(RegistryFault::Unmanageable), feed.fault(), "{mode:?}");
        assert_eq!(
            Some("unmanageable"),
            reported_fault(&gateway).await?.as_deref(),
            "{mode:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn each_failed_check_doubles_the_wait_and_a_success_resets_it() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.refuse_subscriptions(Some(StatusCode::SERVICE_UNAVAILABLE));
    let (_dir, gateway) = subscribing(
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        "check_interval_s = 60\n",
    )?;
    let feed = feed(&gateway)?;
    let mut waits = Vec::new();
    for _ in 0..3 {
        feed.check().await;
        waits.push(backoff(Duration::from_secs(60), feed.failures()).as_secs());
    }
    assert_eq!(vec![120, 240, 480], waits);
    registry.refuse_subscriptions(None);
    assert_eq!(Observed::Up, feed.check().await);
    assert_eq!(0, feed.failures());
    Ok(())
}

#[tokio::test]
async fn an_unsubscribing_drain_with_a_create_in_flight_leaves_no_subscription() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    registry.delay_creates(Some(Duration::from_millis(300)));
    let (_dir, gateway) = subscribing(
        &registry.base_url(),
        "http://127.0.0.1:9/pmir/feed",
        "on_drain = \"unsubscribe\"\n",
    )?;
    let running = feed(&gateway)?.start();
    for _ in 0..200 {
        if registry.creates() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(1, registry.creates(), "the create is in flight");
    running.drain().await;
    assert!(
        registry.subscriptions().is_empty(),
        "the create that landed was deleted after it"
    );
    assert_eq!(1, registry.deleted());
    assert_eq!(1, registry.creates(), "nothing was created after the drain");
    Ok(())
}

/// What `GET /health/dependencies` reports of the Registry's fault.
async fn reported_fault(gateway: &Gateway) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        identity_registry_fault: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let response = send_as_is(gateway.app.clone(), request).await?;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice::<Report>(&bytes)?.identity_registry_fault)
}

#[tokio::test]
async fn the_subscription_is_deleted_on_unsubscribe() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let (_dir, gateway) = subscribing(&registry.base_url(), "http://127.0.0.1:9/pmir/feed", "")?;
    let feed = feed(&gateway)?;
    feed.check().await;
    feed.unsubscribe().await;
    assert_eq!(1, registry.deleted());
    assert!(registry.subscriptions().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_merge_the_registry_sends_drops_the_merged_bindings() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let callback = format!("http://{}/pmir/feed", listener.local_addr()?);
    let (_dir, gateway) = subscribing(&registry.base_url(), &callback, "")?;
    let app = gateway.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    assert_eq!(Observed::Up, feed(&gateway)?.check().await);
    gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    let merge = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let refused = registry.send(&merge, None).await?;
    assert_eq!(
        vec![StatusCode::UNAUTHORIZED],
        refused
            .iter()
            .map(|(status, _)| *status)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        2,
        gateway.bound()?,
        "an unauthenticated feed changes nothing"
    );
    let applied = registry.send(&merge, Some(TOKEN)).await?;
    assert_eq!(
        vec![StatusCode::OK],
        applied
            .iter()
            .map(|(status, _)| *status)
            .collect::<Vec<_>>()
    );
    assert_eq!(1, gateway.bound()?, "the merged ehr_id is no longer bound");
    server.abort();
    Ok(())
}
