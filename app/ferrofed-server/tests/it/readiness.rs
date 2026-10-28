// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Readiness over the indicator registry: every indicator is reported, one
//! down indicator takes the gateway out of rotation, and a wedged one is cut
//! off at the check timeout. The public route names each indicator and its
//! state and never its detail, which the registry keeps for the gateway.

use crate::support::{self, call};
use axum::body::Body;
use ferrofed_server::state::AppState;
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::{CHECK_TIMEOUT, Check, HealthIndicator, Outcome, Registry};
use http::{Request, StatusCode};
use serde::Deserialize;
use std::error::Error as StdError;
use std::sync::Arc;
use std::time::Duration;

/// An indicator that answers what it was built with, after `delay`.
#[derive(Debug)]
struct Stub {
    name: &'static str,
    up: bool,
    delay: Duration,
}

impl HealthIndicator for Stub {
    fn name(&self) -> &'static str {
        self.name
    }

    fn check(&self) -> Check<'_> {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            if self.up {
                Outcome::up()
            } else {
                Outcome::down("the synthetic subsystem is down")
            }
        })
    }
}

/// The readiness document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    state: String,
    phase: String,
    indicators: Vec<Indicator>,
}

impl Report {
    /// The indicator named `name`.
    fn indicator(&self, name: &str) -> Option<&Indicator> {
        self.indicators
            .iter()
            .find(|indicator| indicator.name == name)
    }
}

/// One indicator in the readiness document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Indicator {
    name: String,
    state: String,
}

/// Asks readiness of a gateway whose registry holds `stubs`.
fn registry(stubs: Vec<Stub>) -> Registry {
    let mut indicators: Vec<Arc<dyn HealthIndicator>> = Vec::new();
    for stub in stubs {
        indicators.push(Arc::new(stub));
    }
    Registry::new(indicators)
}

async fn readiness(stubs: Vec<Stub>) -> Result<(StatusCode, Report), Box<dyn StdError>> {
    let state = Arc::new(AppState::with_health(registry(stubs)));
    state.lifecycle().booted();
    let app = ferrofed_server::router(state, &support::settings());
    let (status, body) = call(app, Request::get("/health/readiness").body(Body::empty())?).await?;
    Ok((status, serde_json::from_str(&body)?))
}

/// Returns a stub that answers at once.
const fn stub(name: &'static str, up: bool) -> Stub {
    Stub {
        name,
        up,
        delay: Duration::ZERO,
    }
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn with_no_indicator_registered_the_gateway_is_ready() -> Result<(), Box<dyn StdError>> {
    let (status, report) = readiness(Vec::new()).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!("up", report.state);
    assert!(report.indicators.is_empty());
    assert_eq!("serving", report.phase);
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn every_indicator_up_is_two_hundred_and_each_is_named() -> Result<(), Box<dyn StdError>> {
    let (status, report) = readiness(vec![stub("registry", true), stub("identity", true)]).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!("up", report.state);
    assert_eq!(
        vec!["identity", "registry"],
        report
            .indicators
            .iter()
            .map(|indicator| indicator.name.as_str())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn one_indicator_down_is_five_hundred_and_three_and_its_reason_stays_inside()
-> Result<(), Box<dyn StdError>> {
    let (status, report) =
        readiness(vec![stub("registry", true), stub("node:hospital-a", false)]).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!("down", report.state);
    let down = report
        .indicator("node:hospital-a")
        .ok_or("the down indicator is reported")?;
    assert_eq!("down", down.state);
    let up = report.indicator("registry").ok_or("reported")?;
    assert_eq!("up", up.state);
    let lifecycle = Lifecycle::default();
    lifecycle.booted();
    let inside = registry(vec![stub("registry", true), stub("node:hospital-a", false)])
        .readiness(&lifecycle)
        .await;
    assert_eq!(
        Some("the synthetic subsystem is down"),
        inside
            .indicators
            .iter()
            .find(|indicator| indicator.name == "node:hospital-a")
            .and_then(|indicator| indicator.detail.as_deref()),
        "the registry keeps the reason the public route leaves out"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_wedged_indicator_is_reported_down_at_the_check_timeout() -> Result<(), Box<dyn StdError>>
{
    let wedged = Stub {
        name: "identity",
        up: true,
        delay: CHECK_TIMEOUT + Duration::from_secs(60),
    };
    let (status, report) = readiness(vec![wedged]).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    let identity = report.indicator("identity").ok_or("reported")?;
    assert_eq!("down", identity.state);
    let lifecycle = Lifecycle::default();
    lifecycle.booted();
    let inside = registry(vec![Stub {
        name: "identity",
        up: true,
        delay: CHECK_TIMEOUT + Duration::from_secs(60),
    }])
    .readiness(&lifecycle)
    .await;
    let reason = inside
        .indicators
        .first()
        .and_then(|indicator| indicator.detail.as_deref());
    assert!(
        reason.is_some_and(|detail| detail.contains("did not answer")),
        "{reason:?}"
    );
    Ok(())
}
