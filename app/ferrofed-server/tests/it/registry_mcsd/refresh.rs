// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Keeping the registry in step with the directory: an ITI-91 refresh
//! applies only what changed since the last read, a refresh that breaks an
//! integrity rule is refused and counted with the running registry kept, and
//! a directory that does not answer keeps the registry and shows on
//! `/health/dependencies` (§15.1, §15.2, N19, N21), as does a change the
//! gateway refused (no specification governs the health report: our own
//! design).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use ferrofed_server::directory::RefreshOutcome;
use ferrofed_server::reload::ReloadError;
use ferrofed_testkit::mcsd::{HarnessDirectory, Outage};
use http::{Request, StatusCode};
use serde::Deserialize;

use super::{Gateway, member, members};
use crate::facade::{
    Answer, EHR_B, body, crossref, node_answering, patient_query, post, received, statuses,
};
use crate::metrics::{count, parse};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The reload counter's Prometheus name.
const RELOADS: &str = "ferrofed_registry_reloads_total";

/// The reload count of `result` on the gateway's metrics surface.
fn reloads(gateway: &Gateway, result: &str) -> Result<Option<String>, Box<dyn Error>> {
    let samples = parse(&gateway.state.metrics().render()?)?;
    Ok(count(&samples, RELOADS, &[("result", result)]))
}

/// The endpoint statuses of the patient query through `gateway`.
async fn asked(gateway: &Gateway) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let (status, text) = call(gateway.router(), post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    Ok(statuses(&answer)
        .into_iter()
        .map(|(id, status)| (id.to_owned(), status.to_owned()))
        .collect())
}

/// What `/health/dependencies` reports of the directory.
async fn directory_state(gateway: &Gateway) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        directory: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(gateway.router(), request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str::<Report>(&text)?.directory)
}

/// What `/health/dependencies` reports of the directory.
#[derive(Debug, PartialEq, Eq, Deserialize)]
struct DirectoryHealth {
    directory: Option<String>,
    directory_fault: Option<String>,
}

impl DirectoryHealth {
    /// The directory in `state`, with `fault` as why it is degraded.
    fn of(state: &str, fault: Option<&str>) -> Self {
        Self {
            directory: Some(state.to_owned()),
            directory_fault: fault.map(str::to_owned),
        }
    }
}

/// What `/health/dependencies` reports of the directory, and the whole body.
async fn directory_health(gateway: &Gateway) -> Result<(DirectoryHealth, String), Box<dyn Error>> {
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(gateway.router(), request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok((serde_json::from_str(&text)?, text))
}

#[tokio::test]
async fn a_since_refresh_applies_only_the_changes() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let moved = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(&a.uri(), &b.uri()))?;
    let gateway = Gateway::boot(&harness, 5_000)?;

    harness.put_endpoint(member("b", &moved.uri()).endpoint()?);
    let RefreshOutcome::Applied(applied) = gateway.directory.refresh(&gateway.reloader).await
    else {
        return Err("the moved endpoint is applied".into());
    };
    assert!(applied.endpoints_added.is_empty() && applied.endpoints_removed.is_empty());
    let addresses = gateway.addresses()?;
    assert!(
        addresses
            .get("node-b-pub")
            .is_some_and(|url| url.starts_with(&moved.uri())),
        "{addresses:?}"
    );
    assert!(
        addresses
            .get("node-a-pub")
            .is_some_and(|url| url.starts_with(&a.uri())),
        "node A did not change: {addresses:?}"
    );

    let requests = harness.requests().await;
    let searches: Vec<&String> = requests
        .iter()
        .filter(|request| !request.contains("/_history"))
        .collect();
    assert_eq!(2, searches.len(), "ITI-90 only at boot: {requests:?}");
    assert!(
        requests
            .iter()
            .filter(|request| request.contains("/_history"))
            .all(|request| request.contains("_since=")),
        "every refresh is an ITI-91 history since an instant: {requests:?}"
    );

    asked(&gateway).await?;
    assert!(
        received(&b).await?.is_empty(),
        "the old address is not asked"
    );
    assert_eq!(1, received(&moved).await?.len(), "the new address is asked");
    assert_eq!(Some("1".to_owned()), reloads(&gateway, "applied")?);
    Ok(())
}

#[tokio::test]
async fn a_refresh_with_nothing_changed_keeps_the_registry_and_counts_no_reload() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Unchanged), "{outcome:?}");
    assert_eq!(before, gateway.addresses()?);
    assert_eq!(Some("0".to_owned()), reloads(&gateway, "applied")?);
    Ok(())
}

/// A refresh whose endpoint relies on `hl7-fhir-rest` breaks §15.2 and is
/// refused: the running registry keeps the endpoint as it was, and the
/// refusal counts as a refused reload. CP-20 is the operator's; the gateway
/// assists by refusing the registry.
#[tokio::test]
async fn a_refresh_that_breaks_the_connection_type_rule_is_refused_and_the_registry_kept()
-> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(&a.uri(), &b.uri()))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;

    harness.put_endpoint(
        member("b", "https://cdr-b2.example.org/openehr").endpoint_with_connection_type(
            "http://terminology.hl7.org/CodeSystem/endpoint-connection-type",
            "hl7-fhir-rest",
        )?,
    );
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("the refresh is refused: {outcome:?}").into());
    };
    assert_eq!("registry-invalid", error.class());
    assert_eq!(before, gateway.addresses()?, "the running registry stays");
    assert_eq!(Some("1".to_owned()), reloads(&gateway, "refused")?);
    assert_eq!(Some("0".to_owned()), reloads(&gateway, "applied")?);
    assert_eq!(
        vec![
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "active".to_owned()),
        ],
        asked(&gateway).await?,
        "both members are asked at the addresses the registry kept"
    );

    let again = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(again, RefreshOutcome::Refused(_)),
        "the next refresh asks from the same instant and refuses again: {again:?}"
    );
    assert_eq!(Some("2".to_owned()), reloads(&gateway, "refused")?);
    Ok(())
}

/// A refresh that names one `system_id` for two nodes breaks the registry's
/// integrity (N21, §12a.1) and is refused with the running registry kept.
#[tokio::test]
async fn a_refresh_that_gives_two_nodes_one_system_id_is_refused_and_the_registry_kept()
-> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;
    let mut clash = member("b", "https://cdr-b.example.org/openehr");
    clash.system_id = "cdr-a.example.org".to_owned();
    harness.put_endpoint(clash.endpoint()?);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(
            &outcome,
            RefreshOutcome::Refused(ReloadError::Federation { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(before, gateway.addresses()?);
    assert_eq!(Some("1".to_owned()), reloads(&gateway, "refused")?);
    Ok(())
}

/// A refresh that deletes a member's endpoint drops it and is applied, while
/// its organisation still lists it: the listing names no member and is
/// ignored, and no request reaches the deleted endpoint again (no
/// specification governs this: our own design).
#[tokio::test]
async fn a_refresh_that_deletes_a_listed_endpoint_drops_it_and_is_applied() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(&a.uri(), &b.uri()))?;
    // The cross-reference resolves the patient at node A alone, so it names
    // no member the deletion removes.
    let text = super::config(&harness.base(), 5_000).replace(&crossref(&[("node-b", EHR_B)]), "");
    let gateway = Gateway::boot_from(&text)?;

    harness.delete_endpoint("node-b-pub");
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Applied(applied) = &outcome else {
        return Err(format!("the deletion is applied: {outcome:?}").into());
    };
    assert_eq!(
        vec!["node-b-pub".to_owned()],
        applied
            .endpoints_removed
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect::<Vec<_>>()
    );
    assert!(
        !gateway.addresses()?.contains_key("node-b-pub"),
        "the deleted endpoint left the running registry"
    );
    assert_eq!(
        vec![("node-a-pub".to_owned(), "active".to_owned())],
        asked(&gateway).await?
    );
    assert!(
        received(&b).await?.is_empty(),
        "the deleted endpoint is not asked"
    );
    assert_eq!(Some("1".to_owned()), reloads(&gateway, "applied")?);
    Ok(())
}

/// A refresh that deletes the endpoint of a member the development
/// cross-reference names leaves a row naming no member, and is refused with
/// the running registry kept: the resolver's rule, not the listing.
#[tokio::test]
async fn a_refresh_that_deletes_a_member_the_cross_reference_names_is_refused() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;
    harness.delete_endpoint("node-b-pub");
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("the refresh is refused: {outcome:?}").into());
    };
    assert_eq!("dev-cross-reference", error.class());
    assert_eq!(before, gateway.addresses()?);
    Ok(())
}

/// A member organisation of a shared directory that starts listing another
/// service's endpoint changes nothing the gateway routes by: the refresh is
/// applied, and the listing is logged once, by its reference alone (no
/// specification governs this: our own design).
#[tokio::test]
async fn a_listing_outside_the_selection_is_applied_and_logged_once() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish_examples()?;
    let [a, b] = members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    );
    harness.publish(&[a.clone(), b])?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;
    harness.put_organization(a.organisation_also_listing(&["Endpoint/ex-endpointXCAquery"])?);

    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let again = gateway.directory.refresh(&gateway.reloader).await;
    drop(guard);

    assert!(matches!(outcome, RefreshOutcome::Applied(_)), "{outcome:?}");
    assert!(matches!(again, RefreshOutcome::Unchanged), "{again:?}");
    assert_eq!(before, gateway.addresses()?, "no endpoint changed");
    let text = logs.text();
    let ignored: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("ex-endpointXCAquery"))
        .collect();
    assert_eq!(1, ignored.len(), "one line for the refresh: {text}");
    assert!(
        ignored
            .iter()
            .all(|line| line.contains("Endpoint/ex-endpointXCAquery")),
        "the line names the reference: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_directory_that_does_not_answer_in_time_keeps_the_snapshot_and_shows_down() -> TestResult
{
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(&a.uri(), &b.uri()))?;
    let gateway = Gateway::boot(&harness, 300)?;
    let before = gateway.addresses()?;
    assert_eq!(Some("up".to_owned()), directory_state(&gateway).await?);

    harness.outage(Some(Outage::Silent));
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("a silent directory runs past the deadline: {outcome:?}").into());
    };
    assert_eq!("registry-budget", error.class());
    assert_eq!(before, gateway.addresses()?, "the running registry stays");
    assert_eq!(Some("down".to_owned()), directory_state(&gateway).await?);
    assert_eq!(
        vec![
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "active".to_owned()),
        ],
        asked(&gateway).await?,
        "a query never waits on the directory"
    );
    assert_eq!(
        Some("1".to_owned()),
        reloads(&gateway, "refused")?,
        "a refresh past its deadline counts as refused"
    );

    harness.outage(Some(Outage::Refusing));
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(outcome, RefreshOutcome::Unreachable(_)),
        "{outcome:?}"
    );
    assert_eq!(Some("failing".to_owned()), directory_state(&gateway).await?);

    harness.outage(None);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Unchanged), "{outcome:?}");
    assert_eq!(Some("up".to_owned()), directory_state(&gateway).await?);
    Ok(())
}

/// A `SIGHUP` reload rebuilds the federation over the registry the directory
/// gave, never over a document, and never asks the directory.
#[tokio::test]
async fn a_reload_keeps_the_directorys_registry() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("ferrofed.toml");
    std::fs::write(&config, super::config(&harness.base(), 5_000))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let reloader = ferrofed_server::reload::Reloader::new(
        Some(config),
        super::settings(&super::config(&harness.base(), 5_000))?,
        Arc::clone(&gateway.state),
    );
    let before = gateway.addresses()?;
    let asked_before = harness.requests().await.len();
    let applied = reloader.reload()?;
    assert_eq!(2, applied.members);
    assert_eq!(before, gateway.addresses()?);
    assert_eq!(
        asked_before,
        harness.requests().await.len(),
        "the reload did not ask the directory"
    );
    Ok(())
}

/// The refresh runs by itself every `refresh_interval_s`, off the clinical
/// path, and a change reaches the running registry without a request.
#[tokio::test]
async fn the_registry_keeps_in_step_every_interval() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let text = super::config(&harness.base(), 5_000)
        .replace("refresh_interval_s = 3600", "refresh_interval_s = 1");
    let gateway = Gateway::boot_from(&text)?;
    let state = Arc::clone(&gateway.state);
    let stepping =
        tokio::spawn(Arc::clone(&gateway.directory).keep_in_step(Arc::new(gateway.reloader)));
    harness.put_endpoint(member("b", "https://cdr-b2.example.org/openehr").endpoint()?);
    let moved = || {
        state.federation().is_some_and(|federation| {
            federation
                .snapshot()
                .endpoints()
                .any(|endpoint| endpoint.url().as_str().starts_with("https://cdr-b2."))
        })
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !moved() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stepping.abort();
    assert!(moved(), "the change reached the running registry");
    Ok(())
}

/// A refresh whose answer runs past a cap is refused like a broken registry:
/// the running registry stays, the refusal is counted, and the directory
/// shows as failing (no specification governs the caps: our own design).
#[tokio::test]
async fn a_refresh_past_a_cap_is_refused_counted_and_shown() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let text = super::config(&harness.base(), 5_000)
        .replace("deadline_ms = 5000", "deadline_ms = 5000\nmax_entries = 4");
    let gateway = Gateway::boot_from(&text)?;
    let before = gateway.addresses()?;
    for round in 0..5 {
        harness.put_endpoint(
            member("b", &format!("https://cdr-b{round}.example.org/openehr")).endpoint()?,
        );
    }
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("five versions pass a cap of four entries: {outcome:?}").into());
    };
    assert_eq!("registry-budget", error.class());
    assert_eq!(before, gateway.addresses()?, "the running registry stays");
    assert_eq!(Some("1".to_owned()), reloads(&gateway, "refused")?);
    assert_eq!(Some("failing".to_owned()), directory_state(&gateway).await?);
    Ok(())
}

/// A change the gateway refuses shows the directory `degraded`, with the
/// class of the refusal and no identifier, while the previous registry is
/// served; a directory that then stops answering shows `down` with no fault,
/// the same change refused again shows `degraded` again, and an accepted
/// change shows `up` (no specification governs this: our own design).
#[tokio::test]
async fn a_refused_change_shows_the_directory_degraded_until_a_change_is_accepted() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let moved = node_answering("uid-b::cdr-b.example.org::1").await;
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(&a.uri(), &b.uri()))?;
    let gateway = Gateway::boot(&harness, 1_000)?;
    let up = DirectoryHealth::of("up", None);

    harness.put_endpoint(member("b", &moved.uri()).endpoint()?);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Applied(_)), "{outcome:?}");
    assert_eq!(up, directory_health(&gateway).await?.0);
    let accepted = gateway.addresses()?;

    harness.put_endpoint(
        member("b", "https://cdr-b2.example.org/openehr").endpoint_with_connection_type(
            "http://terminology.hl7.org/CodeSystem/endpoint-connection-type",
            "hl7-fhir-rest",
        )?,
    );
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("the refresh is refused: {outcome:?}").into());
    };
    assert_eq!("registry-invalid", error.class());
    let refused = DirectoryHealth::of("degraded", Some("registry-invalid"));
    let (health, text) = directory_health(&gateway).await?;
    assert_eq!(refused, health, "{text}");
    for content in ["cdr-b2", "hl7-fhir-rest", "org-b", "cdr-b.example.org"] {
        assert!(!text.contains(content), "no directory content: {text}");
    }
    assert_eq!(
        accepted,
        gateway.addresses()?,
        "the previous registry stays"
    );
    assert_eq!(
        vec![
            ("node-a-pub".to_owned(), "active".to_owned()),
            ("node-b-pub".to_owned(), "active".to_owned()),
        ],
        asked(&gateway).await?
    );
    assert_eq!(
        1,
        received(&moved).await?.len(),
        "the kept address is asked"
    );

    harness.outage(Some(Outage::Silent));
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Refused(_)), "{outcome:?}");
    assert_eq!(
        DirectoryHealth {
            directory: Some("down".to_owned()),
            directory_fault: None,
        },
        directory_health(&gateway).await?.0,
        "a directory that did not answer is down, never refused"
    );

    harness.outage(None);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Refused(_)), "{outcome:?}");
    assert_eq!(
        refused,
        directory_health(&gateway).await?.0,
        "the same change is refused again"
    );

    harness.put_endpoint(member("b", "https://cdr-b3.example.org/openehr").endpoint()?);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(matches!(outcome, RefreshOutcome::Applied(_)), "{outcome:?}");
    assert_eq!(up, directory_health(&gateway).await?.0);
    let addresses = gateway.addresses()?;
    assert!(
        addresses
            .get("node-b-pub")
            .is_some_and(|url| url.starts_with("https://cdr-b3.example.org/")),
        "the accepted change is served: {addresses:?}"
    );
    Ok(())
}

/// A sound registry the rest of the configuration does not fit, a member
/// the development cross-reference names removed, shows the directory
/// `degraded` as `configuration-mismatch` until the directory holds the
/// member again (no specification governs this: our own design).
#[tokio::test]
async fn a_change_the_configuration_does_not_fit_shows_a_configuration_mismatch() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let before = gateway.addresses()?;

    harness.delete_endpoint("node-b-pub");
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    let RefreshOutcome::Refused(error) = &outcome else {
        return Err(format!("the refresh is refused: {outcome:?}").into());
    };
    assert_eq!("dev-cross-reference", error.class());
    assert_eq!(
        DirectoryHealth::of("degraded", Some("configuration-mismatch")),
        directory_health(&gateway).await?.0
    );
    assert_eq!(before, gateway.addresses()?);

    harness.put_endpoint(member("b", "https://cdr-b.example.org/openehr").endpoint()?);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(outcome, RefreshOutcome::Unchanged),
        "the directory holds the registry in place again: {outcome:?}"
    );
    assert_eq!(
        DirectoryHealth::of("up", None),
        directory_health(&gateway).await?.0
    );
    Ok(())
}

/// A registry whose integrity the federation refuses, one `system_id` for
/// two nodes (N21, §12a.1), shows the directory `degraded` as
/// `registry-invalid` (no specification governs the report: our own design).
#[tokio::test]
async fn a_change_that_breaks_the_registry_integrity_shows_registry_invalid() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let gateway = Gateway::boot(&harness, 5_000)?;
    let mut clash = member("b", "https://cdr-b.example.org/openehr");
    clash.system_id = "cdr-a.example.org".to_owned();
    harness.put_endpoint(clash.endpoint()?);
    let outcome = gateway.directory.refresh(&gateway.reloader).await;
    assert!(
        matches!(
            &outcome,
            RefreshOutcome::Refused(ReloadError::Federation { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(
        DirectoryHealth::of("degraded", Some("registry-invalid")),
        directory_health(&gateway).await?.0
    );
    Ok(())
}
