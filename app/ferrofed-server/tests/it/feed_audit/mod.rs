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

/// What an `AuditEvent` names of the user it was made for: its `agent:user`
/// (BALP `IRCP`) and the client application of the user's token (DICOM
/// `110150`), as BALP 1.1.4 §3:5.7.5.4 maps the token.
#[derive(Debug, Default)]
pub(crate) struct NamedUser {
    /// Each `IRCP` agent.
    pub(crate) users: Vec<UserAgent>,
    /// The `who.identifier.value` of each Application agent.
    pub(crate) clients: Vec<Option<String>>,
}

/// One `agent:user` of a record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UserAgent {
    /// `who.identifier.system`: the token's `iss`.
    pub(crate) issuer: Option<String>,
    /// `who.identifier.value`: the token's `sub`.
    pub(crate) subject: Option<String>,
    /// `requestor`.
    pub(crate) requestor: bool,
    /// Whether it has a `network`.
    pub(crate) networked: bool,
    /// Its purpose-of-use codes.
    pub(crate) purposes: Vec<String>,
}

/// The user `record`, an `AuditEvent` as JSON, names.
pub(crate) fn named_user(record: &str) -> Result<NamedUser, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Coding {
        system: Option<String>,
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct Concept {
        #[serde(default)]
        coding: Vec<Coding>,
    }
    #[derive(Deserialize)]
    struct Identifier {
        system: Option<String>,
        value: Option<String>,
    }
    #[derive(Deserialize)]
    struct Who {
        identifier: Option<Identifier>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Agent {
        r#type: Option<Concept>,
        who: Option<Who>,
        requestor: bool,
        network: Option<serde::de::IgnoredAny>,
        #[serde(default)]
        purpose_of_use: Vec<Concept>,
    }
    #[derive(Deserialize)]
    struct Event {
        agent: Vec<Agent>,
    }
    let event: Event = serde_json::from_str(record)?;
    let mut named = NamedUser::default();
    for agent in event.agent {
        let typed = |system: &str, code: &str| {
            agent.r#type.as_ref().is_some_and(|concept| {
                concept.coding.iter().any(|coding| {
                    coding.system.as_deref() == Some(system) && coding.code.as_deref() == Some(code)
                })
            })
        };
        let identifier = agent.who.and_then(|who| who.identifier);
        if typed(
            "http://terminology.hl7.org/CodeSystem/v3-ParticipationType",
            "IRCP",
        ) {
            let (system, value) = identifier.map_or((None, None), |id| (id.system, id.value));
            let purposes = agent
                .purpose_of_use
                .into_iter()
                .flat_map(|concept| concept.coding)
                .filter_map(|coding| coding.code)
                .collect();
            named.users.push(UserAgent {
                issuer: system,
                subject: value,
                requestor: agent.requestor,
                networked: agent.network.is_some(),
                purposes,
            });
        } else if typed("http://dicom.nema.org/resources/ontology/DCM", "110150") {
            named.clients.push(identifier.and_then(|id| id.value));
        }
    }
    Ok(named)
}

/// Fails unless `record` names the suite's default caller as BALP 1.1.4
/// §3:5.7.5.4 maps their token: one `agent:user` with the token's `iss` and
/// `sub`, `requestor` and no `network` (BALP Query `agent:user`), the
/// purpose of use `TREAT`, and one Application agent with its `client_id`.
pub(crate) fn names_the_default_caller(record: &str) -> Result<(), Box<dyn Error>> {
    let claims = crate::support::claims();
    let named = named_user(record)?;
    let expected = UserAgent {
        issuer: Some(claims.iss.clone()),
        subject: Some(claims.sub.clone()),
        requestor: true,
        networked: false,
        purposes: vec!["TREAT".to_owned()],
    };
    if named.users != vec![expected] {
        return Err(format!("one agent:user naming the caller, got {named:?}").into());
    }
    if named.clients != vec![Some(claims.client_id)] {
        return Err(format!("one Application agent naming the client, got {named:?}").into());
    }
    Ok(())
}

/// Fails unless `record` names no user and no client application, as a
/// record the gateway makes on its own behalf (the BALP `NoUser` examples).
pub(crate) fn names_no_caller(record: &str) -> Result<(), Box<dyn Error>> {
    let named = named_user(record)?;
    if !named.users.is_empty() {
        return Err(format!("no agent:user, got {named:?}").into());
    }
    let claims = crate::support::claims();
    if record.contains(&claims.sub) || record.contains(&claims.client_id) {
        return Err("the gateway's own record names no caller".into());
    }
    Ok(())
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
