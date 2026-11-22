// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A harness ATNA Audit Record Repository with the ATX: FHIR Feed Option.
//!
//! It answers the Send Audit Resource Request of ITI-20, a FHIR `create` of one
//! `AuditEvent` at `[base]/AuditEvent` (the `RESTful` ATNA supplement, ITI
//! TF-2 §3.20.4.2), and keeps every record it accepted, for a test to
//! inspect (#486).
//!
//! It also answers ITI-81 Retrieve ATNA Audit Event, `GET [base]/AuditEvent`
//! (§3.81), over the records it accepted, and keeps the `Audit Log Used`
//! event each search records ([`FeedRepository::log_used`]).
//!
//! It answers the Send Audit Bundle Request too, a FHIR `batch` Bundle of
//! `AuditEvent` creates posted to `[base]` (ITI TF-2 §3.20.4.4), with a
//! `batch-response` whose every entry answers `201 Created` (§3.20.4.5),
//! keeping each record of the batch in its entry order.
//!
//! It answers `201` and keeps the record while it is
//! [up](FeedRepository::set_up), and `503`, which a sender retries, while it
//! is down. Once [silent](FeedRepository::go_silent), it takes each request
//! and holds its answer past the life of any test. It speaks plain `http`,
//! which the gateway admits under the development profile alone. No
//! specification governs the harness: our own design.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

mod search;

/// The FHIR base the harness serves under.
const BASE: &str = "/arr/";

/// How long a silent repository holds its answer: past the life of any test.
const SILENT: Duration = Duration::from_hours(1);

/// What the repository holds and how it answers.
#[derive(Debug, Default)]
struct Held {
    down: AtomicBool,
    silent: AtomicBool,
    asked: AtomicUsize,
    refused: AtomicUsize,
    records: Mutex<Vec<String>>,
    used: Mutex<Vec<String>>,
}

/// A running harness FHIR Feed repository.
#[derive(Debug)]
pub struct FeedRepository {
    server: Server,
    held: Arc<Held>,
}

/// The answer of the repository: `201` while up, keeping the record, `503`
/// while down, and none within the test while silent.
struct Answer(Arc<Held>);

impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.0.asked.fetch_add(1, Ordering::SeqCst);
        if self.0.silent.load(Ordering::SeqCst) {
            return ResponseTemplate::new(201).set_delay(SILENT);
        }
        if self.0.down.load(Ordering::SeqCst) {
            self.0.refused.fetch_add(1, Ordering::SeqCst);
            return ResponseTemplate::new(503);
        }
        if request.url.path() == BASE {
            return self.batch(&request.body);
        }
        self.keep([String::from_utf8_lossy(&request.body).into_owned()]);
        ResponseTemplate::new(201)
    }
}

/// A Send Audit Bundle Request: the entries of a `batch` Bundle, each
/// resource kept as the text the sender wrote.
#[derive(serde::Deserialize)]
struct Batch<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(borrow)]
    entry: Vec<BatchEntry<'a>>,
}

/// One entry of a [`Batch`].
#[derive(serde::Deserialize)]
struct BatchEntry<'a> {
    #[serde(borrow)]
    resource: &'a serde_json::value::RawValue,
}

impl Answer {
    /// Keeps `records`, in order.
    fn keep(&self, records: impl IntoIterator<Item = String>) {
        self.0
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(records);
    }

    /// The answer to a Send Audit Bundle Request: every record kept and a
    /// `batch-response` of one `201 Created` per entry (ITI TF-2
    /// §3.20.4.5.2), or `400` for a body that is no `batch` Bundle.
    fn batch(&self, body: &[u8]) -> ResponseTemplate {
        let Ok(batch) = serde_json::from_slice::<Batch<'_>>(body) else {
            return ResponseTemplate::new(400);
        };
        if batch.kind != "batch" {
            return ResponseTemplate::new(400);
        }
        let entries = batch.entry.len();
        self.keep(
            batch
                .entry
                .into_iter()
                .map(|entry| entry.resource.get().to_owned()),
        );
        let response = serde_json::json!({
            "resourceType": "Bundle",
            "type": "batch-response",
            "entry": vec![serde_json::json!({"response": {"status": "201 Created"}}); entries],
        });
        ResponseTemplate::new(200).set_body_json(response)
    }
}

impl FeedRepository {
    /// Starts a repository on a free loopback port, up.
    pub async fn start() -> Self {
        let server = Server::start().await;
        let held = Arc::new(Held::default());
        Mock::given(method("POST"))
            .and(path(format!("{BASE}AuditEvent")))
            .respond_with(Answer(Arc::clone(&held)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(BASE))
            .respond_with(Answer(Arc::clone(&held)))
            .mount(&server)
            .await;
        let base = format!("{}{BASE}", server.uri());
        Mock::given(method("GET"))
            .and(path(format!("{BASE}AuditEvent")))
            .respond_with(search::Search {
                held: Arc::clone(&held),
                base,
            })
            .mount(&server)
            .await;
        Self { server, held }
    }

    /// The repository's FHIR base, `http://127.0.0.1:<port>/arr/`.
    #[must_use]
    pub fn base(&self) -> String {
        format!("{}{BASE}", self.server.uri())
    }

    /// Brings the repository up, or takes it down.
    pub fn set_up(&self, up: bool) {
        self.held.down.store(!up, Ordering::SeqCst);
    }

    /// Makes the repository take each connection and request and hold its
    /// answer past the life of any test, keeping no record.
    pub fn go_silent(&self) {
        self.held.silent.store(true, Ordering::SeqCst);
    }

    /// How many requests reached the repository, answered or not.
    #[must_use]
    pub fn asked(&self) -> usize {
        self.held.asked.load(Ordering::SeqCst)
    }

    /// Waits up to `within` until `count` requests have reached the
    /// repository, and returns how many had by then.
    pub async fn wait_asked(&self, count: usize, within: Duration) -> usize {
        let until = Instant::now() + within;
        loop {
            let asked = self.asked();
            if asked >= count || Instant::now() >= until {
                return asked;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Every record the repository accepted, in arrival order, as the JSON
    /// text the sender posted.
    #[must_use]
    pub fn records(&self) -> Vec<String> {
        self.held
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many records the repository answered `503` while it was down.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.held.refused.load(Ordering::SeqCst)
    }

    /// Waits up to `within` until `count` records have been accepted, and
    /// returns every record accepted by then.
    ///
    /// It returns once this repository has stored the records, which can be
    /// before the sender has read the `201` and freed its spool. A test that
    /// needs the spool drained also waits for the `audit_feed` state the
    /// gateway reports on `GET /operator/dependencies` to read `up`.
    pub async fn wait_for(&self, count: usize, within: Duration) -> Vec<String> {
        let until = Instant::now() + within;
        loop {
            let records = self.records();
            if records.len() >= count || Instant::now() >= until {
                return records;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Every `Audit Log Used` event the repository recorded for an ITI-81
    /// search of its log, in the order of the searches, as FHIR JSON text
    /// (`RESTful` ATNA §3.81.5.1).
    #[must_use]
    pub fn log_used(&self) -> Vec<String> {
        self.held
            .used
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
