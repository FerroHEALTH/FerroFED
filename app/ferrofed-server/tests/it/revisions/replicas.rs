// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two replicas on one revision store: a revision one applies reaches the
//! other through its watch of the running mark within the poll bound; a
//! replica that refuses it keeps its running configuration, reports the
//! drift on `GET /admin/config` and on the metrics, and does not try it
//! again until a reload; `config.ui = "off"` starts no watch; and
//! `config.poll_ms` is read within its bounds. No specification governs
//! this: our own design.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::revisions::RevisionsFault;
use ferrofed_server::reload::Reloader;
use ferrofed_server::revision::RevisionId;
use ferrofed_server::revision::service::Followed;
use ferrofed_server::revision::watch;
use ferrofed_server::state::AppState;
use http::StatusCode;

use super::{Gateway, Replicas, Standing, TestResult, rest};
use crate::metrics::{count, parse};

/// How often the watch reads the mark in these tests.
const POLL: Duration = Duration::from_millis(50);

/// How long a replica may take to follow, far past [`POLL`].
const BOUND: Duration = Duration::from_secs(10);

/// Waits until `done` holds, or fails once [`BOUND`] elapses.
async fn within_bound(
    what: &str,
    mut done: impl FnMut() -> Result<bool, Box<dyn std::error::Error>>,
) -> TestResult {
    let deadline = tokio::time::Instant::now() + BOUND;
    while !done()? {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("{what} within {BOUND:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

/// What `GET /admin/config` on `gateway` answers.
async fn stands(gateway: &Gateway) -> Result<Standing, Box<dyn std::error::Error>> {
    let (status, text) = gateway.get("/admin/config").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// Saves the registry of members A and B on replica one as revision 2, and
/// applies it there.
async fn apply_two_members(replicas: &Replicas) -> TestResult {
    let toml = rest(&replicas.two_members(), "");
    let (status, text) = replicas.one.save(Some(1), "add member B", &toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = replicas
        .one
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        2,
        replicas.one.members()?,
        "member B serves on the first replica"
    );
    Ok(())
}

/// The value of the metric `name` with `labels` on `gateway`.
fn metric(
    gateway: &Gateway,
    name: &str,
    labels: &[(&str, &str)],
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let samples = parse(&gateway.state.metrics().render()?)?;
    Ok(count(&samples, name, labels))
}

#[tokio::test]
async fn both_replicas_start_on_the_same_revision() -> TestResult {
    let replicas = Replicas::start()?;
    for gateway in [&replicas.one, &replicas.two] {
        let running = stands(gateway).await?;
        assert_eq!(Some(1), running.running);
        assert!(running.in_step);
        assert_eq!(None, running.refused);
    }
    assert_eq!(
        Some(1),
        replicas.two.revisions.store().head()?.map(RevisionId::get),
        "the second replica's file is the first one's, so it records nothing"
    );
    Ok(())
}

#[tokio::test]
async fn an_apply_on_one_replica_reaches_the_other_within_the_poll_bound() -> TestResult {
    let replicas = Replicas::start()?;
    let watching = watch::start(
        Some(&replicas.two.revisions),
        &replicas.two.reloader,
        &replicas.two.state,
        POLL,
    )
    .ok_or("a managed gateway watches its store")?;
    apply_two_members(&replicas).await?;
    within_bound("the second replica runs member B", || {
        Ok(replicas.two.members()? == 2)
    })
    .await?;
    let running = stands(&replicas.two).await?;
    assert_eq!(Some(2), running.running);
    assert!(running.in_step);
    assert_eq!(None, running.refused);
    assert_eq!(
        Some(2),
        running.current.map(|current| current.revision.get()),
        "the mark is the first replica's apply"
    );
    within_bound("the gauge reads in step", || {
        Ok(metric(&replicas.two, "ferrofed_config_revision_in_step", &[])?.as_deref() == Some("1"))
    })
    .await?;
    assert_eq!(
        Some(String::from("1")),
        metric(
            &replicas.two,
            "ferrofed_config_revision_follows_total",
            &[("result", "applied")]
        )?
    );
    watching.stop(Duration::from_secs(5)).await;
    Ok(())
}

#[tokio::test]
async fn a_replica_that_refuses_the_revision_keeps_its_configuration_and_reports_drift()
-> TestResult {
    let replicas = Replicas::start()?;
    let held = std::fs::read_to_string(replicas.two_members())?;
    apply_two_members(&replicas).await?;
    // The second replica finds the document the revision names missing, as a
    // file absent on its own host would be.
    std::fs::remove_file(replicas.two_members())?;
    let watching = watch::start(
        Some(&replicas.two.revisions),
        &replicas.two.reloader,
        &replicas.two.state,
        POLL,
    )
    .ok_or("a managed gateway watches its store")?;
    within_bound("the second replica reports the refusal", || {
        Ok(metric(
            &replicas.two,
            "ferrofed_config_revision_follows_total",
            &[("result", "refused")],
        )?
        .as_deref()
            == Some("1"))
    })
    .await?;
    watching.stop(Duration::from_secs(5)).await;

    assert_eq!(
        1,
        replicas.two.members()?,
        "the running configuration stays"
    );
    let running = stands(&replicas.two).await?;
    assert_eq!(Some(1), running.running);
    assert!(!running.in_step);
    assert_eq!(Some(2), running.refused);
    assert_eq!(
        Some(String::from("0")),
        metric(&replicas.two, "ferrofed_config_revision_in_step", &[])?
    );
    assert_eq!(
        Followed::Drifting(RevisionId::new(2).ok_or("2")?),
        replicas.two.revisions.follow(&replicas.two.reloader)?,
        "a refused revision is not tried again"
    );

    std::fs::write(replicas.two_members(), held)?;
    replicas.two.reloader.reload()?;
    assert_eq!(
        2,
        replicas.two.members()?,
        "a reload runs the marked revision"
    );
    let running = stands(&replicas.two).await?;
    assert_eq!(Some(2), running.running);
    assert!(running.in_step);
    assert_eq!(None, running.refused);
    Ok(())
}

#[tokio::test]
async fn off_starts_no_watch() -> TestResult {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert!(settings.revisions.is_none());
    let state = Arc::new(AppState::default());
    let reloader = Arc::new(Reloader::new(None, settings, Arc::clone(&state)));
    assert!(watch::start(None, &reloader, &state, POLL).is_none());
    Ok(())
}

#[test]
fn the_poll_interval_is_read_within_its_bounds() -> TestResult {
    let managed =
        "[metrics]\nlisten = \"127.0.0.1:9\"\n\n[config]\nui = \"managed\"\npath = \"r.redb\"\n";
    let resolve = |extra: &str| {
        Config::from_sources(Some(&format!("{managed}{extra}")), &BTreeMap::new())
            .and_then(|config| config.resolve())
    };
    assert_eq!(Duration::from_secs(5), resolve("")?.revision_poll);
    assert_eq!(
        Duration::from_millis(100),
        resolve("poll_ms = 100\n")?.revision_poll
    );
    for value in [0_u64, 99, 600_001] {
        let refused = resolve(&format!("poll_ms = {value}\n"));
        assert!(
            matches!(
                refused,
                Err(Error::Revisions(RevisionsFault::Poll { value: found })) if found == value
            ),
            "{value}: {refused:?}"
        );
    }
    let off = Config::from_sources(Some("[config]\npoll_ms = 1000\n"), &BTreeMap::new())?.resolve();
    assert!(
        matches!(
            off,
            Err(Error::Revisions(RevisionsFault::Off {
                key: "config.poll_ms"
            }))
        ),
        "{off:?}"
    );
    Ok(())
}
