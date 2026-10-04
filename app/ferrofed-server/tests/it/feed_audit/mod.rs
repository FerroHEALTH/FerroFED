// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit records of the FHIR profiles' transactions, `[audit]`: each
//! PIXm ITI-83, mCSD ITI-90 and ITI-91, and PMIR ITI-93 and ITI-94
//! transaction is recorded as its profile's BALP `AuditEvent` (PIXm
//! §2:3.83.5.1, mCSD §2:3.90.5.1 and §2:3.91.5.1, PMIR §2:3.93.5.1 and
//! §2:3.94.5.1) and posted to the testkit's harness Audit Record Repository
//! over the FHIR Feed of ITI-20 (the `RESTful` ATNA supplement, ITI TF-2
//! §3.20.4.2). The patient reaches the repository and no log line, metric
//! or node request (§5.4, N33); a repository that is down holds the records
//! in the spool (§3.20.4.1.1); and a record the spool cannot take fails the
//! transaction closed, as the ITI-55 audit trail does.

mod config;
mod log;
mod mcsd;
mod pixm;
mod pmir;

use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::atna_feed::FeedRepository;
use http::{Request, StatusCode};
use serde::Deserialize;

use crate::support::call;

/// The longest a test waits for the forwarder to reach a state it reaches
/// within one retry of `retry_max_ms`: far past any stall of a loaded host,
/// so only a forwarder that never gets there fails the wait.
pub(crate) const SETTLE: Duration = Duration::from_secs(15);

/// The `[audit]` tables that post every record to `repository`, with the
/// `[audit.repository]` keys `extra`.
pub(crate) fn audit_tables(repository: &FeedRepository, extra: &str) -> String {
    format!(
        "\n[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"{}\"\nhostname = \"gateway.example.org\"\nretry_max_ms = 400\n{extra}\n",
        repository.base()
    )
}

/// The `spool_dir` key of a spool in `dir`, with the path it names.
pub(crate) fn spool_key(dir: &Path) -> (String, std::path::PathBuf) {
    let spool = dir.join("audit-feed-spool");
    (
        format!(
            "spool_dir = {}",
            toml::Value::String(spool.display().to_string())
        ),
        spool,
    )
}

/// The records in the spool directory `spool`, its quarantine left out.
pub(crate) fn spooled(spool: &Path) -> Result<usize, Box<dyn Error>> {
    let mut files = 0_usize;
    for entry in std::fs::read_dir(spool)? {
        if entry?.path().is_file() {
            files = files.saturating_add(1);
        }
    }
    Ok(files)
}

/// The state `GET /health/dependencies` reports of the FHIR Feed audit
/// repository.
pub(crate) async fn feed_state(app: &Router) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        audit_feed: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    if status != StatusCode::OK {
        return Err(format!("/health/dependencies answered {status}: {text}").into());
    }
    Ok(serde_json::from_str::<Report>(&text)?.audit_feed)
}

/// Waits up to [`SETTLE`] until the health report shows the FHIR Feed
/// repository `expected`, and returns the last state it showed.
pub(crate) async fn await_feed_state(
    app: &Router,
    expected: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let until = Instant::now() + SETTLE;
    loop {
        let state = feed_state(app).await?;
        if state.as_deref() == Some(expected) || Instant::now() >= until {
            return Ok(state);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The IHE transaction codes of `record`, an `AuditEvent` as JSON.
pub(crate) fn transactions(record: &str) -> Result<Vec<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Coding {
        system: Option<String>,
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct Event {
        subtype: Vec<Coding>,
    }
    let event: Event = serde_json::from_str(record)?;
    Ok(event
        .subtype
        .into_iter()
        .filter(|coding| coding.system.as_deref() == Some("urn:ihe:event-type-code"))
        .filter_map(|coding| coding.code)
        .collect())
}
