// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity feed across several gateway replicas, against the harness
//! Patient Identity Registry: replicas behind one `callback_url` share one
//! subscription, which a drain keeps for the others; replicas that created
//! one each at once settle on one; and replicas with a `callback_url` of
//! their own each hold a subscription, so every one of them applies every
//! change (PMIR §2:3.94.4, §2:3.94.4.5, §2:3.93.4).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::health::Observed;
use ferrofed_server::binding::ihe::pmir::IdentityFeed;
use ferrofed_server::binding::ihe::pmir::subscription::Running;
use ferrofed_testkit::pmir::{PatientIdentityRegistry, merge_message};
use http::StatusCode;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::{DOMAIN_A, EHR_A, EHR_A2, Gateway, TOKEN, gateway, text};

type TestResult = Result<(), Box<dyn Error>>;

/// One replica: its configuration directory and its gateway.
struct Replica {
    _dir: tempfile::TempDir,
    gateway: Gateway,
}

impl Replica {
    /// A replica subscribing at `registry_url`, its feed sent to `callback`.
    fn new(registry_url: &str, callback: &str, extra: &str) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let gateway = gateway(&text(dir.path(), registry_url, callback, extra)?)?;
        Ok(Self { _dir: dir, gateway })
    }

    fn feed(&self) -> Result<&Arc<IdentityFeed>, Box<dyn Error>> {
        Ok(self.gateway.state.identity_feed().ok_or("[pmir] is set")?)
    }

    /// Serves the replica's feed route on `listener`.
    fn serve(&self, listener: TcpListener) -> JoinHandle<std::io::Result<()>> {
        let app = self.gateway.app.clone();
        tokio::spawn(async move { axum::serve(listener, app).await })
    }

    /// Starts the subscription loop and waits for its first check.
    async fn started(&self) -> Result<Running, Box<dyn Error>> {
        let running = self.feed()?.start();
        for _ in 0..200 {
            if self.feed()?.observed() == Observed::Up {
                return Ok(running);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Err("the first check did not succeed".into())
    }
}

/// A loopback listener and the feed URL it serves.
async fn callback() -> Result<(TcpListener, String), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/pmir/feed", listener.local_addr()?);
    Ok((listener, url))
}

/// The statuses of the Registry's answers.
fn statuses(answers: &[(StatusCode, String)]) -> Vec<StatusCode> {
    answers.iter().map(|(status, _)| *status).collect()
}

/// Two replicas behind one `callback_url` share one subscription. One of
/// them drains, and the subscription stays: the Registry still sends the
/// feed, which the other replica applies.
#[tokio::test]
async fn a_drain_keeps_the_subscription_another_replica_relies_on() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let (listener, balancer) = callback().await?;
    let first = Replica::new(&registry.base_url(), &balancer, "")?;
    let second = Replica::new(&registry.base_url(), &balancer, "")?;
    let first_running = first.started().await?;
    let second_running = second.started().await?;
    assert_eq!(
        1,
        registry.creates(),
        "the second replica adopts the first's"
    );
    assert_eq!(1, registry.subscriptions().len());

    first_running.drain().await;
    assert_eq!(0, registry.deleted(), "the drain deletes nothing");
    assert_eq!(1, registry.subscriptions().len(), "the subscription stays");

    let server = second.serve(listener);
    second.gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    let merge = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let applied = registry.send(&merge, Some(TOKEN)).await?;
    assert_eq!(vec![StatusCode::OK], statuses(&applied));
    assert_eq!(
        1,
        second.gateway.bound()?,
        "the replica left applied the merge"
    );

    second_running.drain().await;
    assert_eq!(1, registry.subscriptions().len(), "kept for the next start");
    server.abort();
    Ok(())
}

/// A replica restarted after a drain adopts the subscription it kept, and
/// creates none.
#[tokio::test]
async fn a_restart_adopts_the_kept_subscription() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let callback = "http://127.0.0.1:9/pmir/feed";
    let before = Replica::new(&registry.base_url(), callback, "")?;
    before.started().await?.drain().await;
    let after = Replica::new(&registry.base_url(), callback, "")?;
    assert_eq!(Observed::Up, after.feed()?.check().await);
    assert_eq!(1, registry.creates());
    assert_eq!(1, registry.subscriptions().len());
    Ok(())
}

/// Two replicas that each created a subscription for the one
/// `callback_url` settle on one: the next to search adopts the one whose
/// location sorts first and deletes the other, and the replica that held
/// the other adopts the same one when it finds its own gone.
#[tokio::test]
async fn replicas_that_each_created_one_settle_on_one() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let callback = "http://127.0.0.1:9/pmir/feed";
    registry.refuse_search(true);
    let first = Replica::new(&registry.base_url(), callback, "")?;
    let second = Replica::new(&registry.base_url(), callback, "")?;
    assert_eq!(Observed::Up, first.feed()?.check().await);
    assert_eq!(Observed::Up, second.feed()?.check().await);
    assert_eq!(2, registry.subscriptions().len(), "one each, made at once");
    registry.refuse_search(false);

    let restarted = Replica::new(&registry.base_url(), callback, "")?;
    assert_eq!(Observed::Up, restarted.feed()?.check().await);
    assert_eq!(1, registry.deleted(), "the duplicate is deleted");
    assert_eq!(1, registry.subscriptions().len());
    assert_eq!(Observed::Up, first.feed()?.check().await);
    assert_eq!(Observed::Up, second.feed()?.check().await);
    assert_eq!(2, registry.creates(), "none is created again");
    assert_eq!(1, registry.subscriptions().len());
    Ok(())
}

/// Replicas with a `callback_url` of their own each hold a subscription, so
/// the Registry sends every change to every replica and each drops the
/// bindings it made stale. A replica that drains with
/// `on_drain = "unsubscribe"` deletes its own subscription and no other.
#[tokio::test]
async fn replicas_with_their_own_callback_each_apply_every_change() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let (first_listener, first_callback) = callback().await?;
    let (second_listener, second_callback) = callback().await?;
    let unsubscribe = "on_drain = \"unsubscribe\"\n";
    let first = Replica::new(&registry.base_url(), &first_callback, unsubscribe)?;
    let second = Replica::new(&registry.base_url(), &second_callback, unsubscribe)?;
    let first_running = first.started().await?;
    let second_running = second.started().await?;
    assert_eq!(2, registry.subscriptions().len(), "one per callback_url");
    let servers = [first.serve(first_listener), second.serve(second_listener)];
    first.gateway.bind("caller-1", &[EHR_A, EHR_A2])?;
    second.gateway.bind("caller-2", &[EHR_A, EHR_A2])?;

    let merge = merge_message("patient-old", &[(DOMAIN_A, EHR_A)], "patient-new")?;
    let applied = registry.send(&merge, Some(TOKEN)).await?;
    assert_eq!(vec![StatusCode::OK, StatusCode::OK], statuses(&applied));
    assert_eq!(1, first.gateway.bound()?, "the first replica applied it");
    assert_eq!(1, second.gateway.bound()?, "the second replica applied it");

    first_running.drain().await;
    let [left] = <[_; 1]>::try_from(registry.subscriptions()).map_err(|_held| "one left")?;
    assert_eq!(second_callback, left.endpoint, "only its own was deleted");
    second_running.drain().await;
    assert!(registry.subscriptions().is_empty());
    for server in servers {
        server.abort();
    }
    Ok(())
}
