// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A harness ATNA Audit Record Repository with the ATX: FHIR Feed Option.
//!
//! It answers the Send Audit Resource Request of ITI-20, a FHIR `create` of one
//! `AuditEvent` at `[base]/AuditEvent` (the `RESTful` ATNA supplement, ITI
//! TF-2 §3.20.4.2), and keeps every record it accepted, for a test to
//! inspect (#486).
//!
//! It answers `201` and keeps the record while it is
//! [up](FeedRepository::set_up), and `503`, which a sender retries, while it
//! is down. It speaks plain `http`, which the gateway admits under the
//! development profile alone. No specification governs the harness: our own
//! design.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

/// The FHIR base the harness serves under.
const BASE: &str = "/arr/";

/// What the repository holds and how it answers.
#[derive(Debug, Default)]
struct Held {
    down: AtomicBool,
    refused: AtomicUsize,
    records: Mutex<Vec<String>>,
}

/// A running harness FHIR Feed repository.
#[derive(Debug)]
pub struct FeedRepository {
    server: Server,
    held: Arc<Held>,
}

/// The answer of the repository: `201` while up, keeping the record, and
/// `503` while down.
struct Answer(Arc<Held>);

impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        if self.0.down.load(Ordering::SeqCst) {
            self.0.refused.fetch_add(1, Ordering::SeqCst);
            return ResponseTemplate::new(503);
        }
        self.0
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8_lossy(&request.body).into_owned());
        ResponseTemplate::new(201)
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
}
