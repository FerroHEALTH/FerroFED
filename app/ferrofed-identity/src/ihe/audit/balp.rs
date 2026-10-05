// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit recorders of the FHIR profiles' transactions.
//!
//! PIXm ITI-83, PDQm ITI-78 and ITI-119, mCSD ITI-90 and ITI-91, and PMIR
//! ITI-93 and ITI-94 are each recorded as the BALP `AuditEvent` its profile
//! fixes (PIXm §2:3.83.5.1.1, PDQm §2:3.78.5.1 and §2:3.119.5.1.1, mCSD
//! §2:3.90.5.1 and §2:3.91.5.1, PMIR §2:3.93.5.1 and §2:3.94.5.1).
//!
//! [`FeedAudit`] sends each record to an ATNA Audit Record Repository over
//! the ATX: FHIR Feed Option of ITI-20 (BALP §1:52.1.1.1; the `RESTful` ATNA
//! supplement, ITI TF-2 §3.20.4.2). The record is stored in the spool and
//! delivered from there in order by `ihe_iti`'s forwarder; it counts as
//! recorded once it is stored, a repository that cannot be reached delays
//! its delivery, and a spool that is full, cannot be written, or does not
//! store the record within its write bound refuses the record, which fails
//! the transaction closed, as the ITI-55 audit trail does.
//!
//! [`LogFeedAudit`] writes each record as a structured event at the
//! [`AUDIT_TARGET`] log target, for a deployment
//! that routes its log to its audit repository, without a patient identifier
//! or the request that carries one.
//!
//! A record names the patient, so it leaves this module only for the spool
//! and the repository connection; no log line and no error carries it. A
//! record made for a verified caller names them as its user agent (PIXm
//! §2:3.83.5.2.1), and that identity takes the same path alone:
//! [`LogFeedAudit`] says whose behalf a record was made on, never who.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use ihe_iti::atna::feed::{FeedAddressError, FeedRepository};
use ihe_iti::atna::forwarder::{Forwarder, Status};
use ihe_iti::balp::{AuditError, AuditRecorder, Direction, Entity, Exchange, Observer};
use ihe_iti::user::{PurposeOfUse, User};
use tokio::task::JoinHandle;
use url::Url;

use crate::fhir::{self, Authentication, ClientError, Tls};
use crate::ihe::audit::AUDIT_TARGET;
use crate::role::behalf::OnBehalfOf;

/// Returns `on_behalf` as `ihe_iti`'s audited clients take it: a verified
/// caller is the user of the exchange, and the gateway its own system.
pub(crate) fn audited_as(on_behalf: &OnBehalfOf) -> ihe_iti::user::OnBehalfOf {
    match on_behalf {
        OnBehalfOf::Caller(caller) => ihe_iti::user::OnBehalfOf::User(
            User::new(
                caller.issuer().to_owned(),
                caller.subject().to_owned(),
                caller.client_id().to_owned(),
            )
            .with_audience(caller.audience().map(str::to_owned))
            .with_purposes(
                caller
                    .purposes()
                    .iter()
                    .map(|purpose| PurposeOfUse {
                        system: purpose.system.clone(),
                        code: purpose.code.clone(),
                    })
                    .collect(),
            ),
        ),
        OnBehalfOf::Gateway => ihe_iti::user::OnBehalfOf::System,
    }
}

/// What a log line says of `on_behalf`: whose behalf, never who.
pub(crate) fn logged_as(on_behalf: &ihe_iti::user::OnBehalfOf) -> &'static str {
    match on_behalf {
        ihe_iti::user::OnBehalfOf::System => "gateway",
        ihe_iti::user::OnBehalfOf::User(_) => "caller",
        _ => "other",
    }
}

/// Why a FHIR Feed repository could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FeedConfigError {
    /// The HTTP client could not be built.
    #[error("the HTTP client for the audit repository could not be built")]
    Client(#[source] ClientError),
    /// The repository's FHIR base was refused.
    #[error("the audit repository FHIR base was refused")]
    Address(#[source] FeedAddressError),
}

/// Returns the FHIR Feed repository at `base`.
///
/// It is reached over `https`, or over clear text when `cleartext` is set,
/// which a development deployment alone asks for, with the `tls` material:
/// the ATNA secure channel authenticates both sides (ITI TF-2 §3.19). Each
/// request is bounded by `timeout`, and no redirect is followed, because a
/// record names the patient.
///
/// # Errors
///
/// A [`FeedConfigError`] for an HTTP client that cannot be built or a base
/// the feed refuses.
pub fn feed_repository(
    base: Url,
    cleartext: bool,
    tls: &Tls,
    timeout: Duration,
) -> Result<FeedRepository, FeedConfigError> {
    let http = fhir::http_client(&Authentication::None, tls).map_err(FeedConfigError::Client)?;
    if cleartext {
        FeedRepository::cleartext_for_development(base, http, timeout)
    } else {
        FeedRepository::new(base, http, timeout)
    }
    .map_err(FeedConfigError::Address)
}

/// The recorder over one FHIR Feed repository and its spool.
pub struct FeedAudit {
    forwarder: Arc<Forwarder>,
    observer: Observer,
    running: Mutex<Option<JoinHandle<()>>>,
}

impl fmt::Debug for FeedAudit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedAudit")
            .field("observer", &self.observer)
            .field("status", &self.forwarder.status())
            .finish_non_exhaustive()
    }
}

impl FeedAudit {
    /// The recorder that writes each record as `observer`, delivering
    /// through `forwarder`.
    #[must_use]
    pub fn new(forwarder: Arc<Forwarder>, observer: Observer) -> Self {
        Self {
            forwarder,
            observer,
            running: Mutex::new(None),
        }
    }

    /// Starts delivering the spooled records on the Tokio runtime the caller
    /// runs on, unless a delivery is running already; without a runtime it
    /// does nothing, and the next record starts it.
    ///
    /// A delivery started on a runtime that has since shut down has ended,
    /// so it is started again: a read on a short-lived runtime of its own
    /// never leaves the spool undelivered.
    pub fn start(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.as_ref().is_some_and(|task| !task.is_finished()) {
            return;
        }
        *running = Some(runtime.spawn(Arc::clone(&self.forwarder).run()));
    }

    /// What the spool holds and how the deliveries went.
    #[must_use]
    pub fn status(&self) -> Status {
        self.forwarder.status()
    }
}

#[async_trait]
impl AuditRecorder for FeedAudit {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        self.start();
        let record = exchange
            .audit_event(&self.observer)
            .map_err(|error| AuditError(Box::new(error)))?;
        self.forwarder
            .submit(record.into_bytes())
            .await
            .map_err(|error| AuditError(Box::new(error)))
    }
}

/// The recorder that writes each record as a structured `tracing` event at
/// [`AUDIT_TARGET`].
///
/// The event carries the record's profile, subtypes, action, outcome, the
/// other party's name, whether it was made for a caller or by the gateway on
/// its own behalf, and the count of each kind of entity; it never carries a
/// patient identifier, a patient reference, the request, which may name one,
/// or the caller's identity. It accepts every record.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogFeedAudit;

#[async_trait]
impl AuditRecorder for LogFeedAudit {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        let subtypes: Vec<&str> = exchange
            .kind
            .subtypes
            .iter()
            .map(|subtype| subtype.code)
            .collect();
        let (direction, peer) = match &exchange.direction {
            Direction::Sent { server } => ("sent", server.who.as_str()),
            Direction::Received { client, .. } => ("received", client.who.as_str()),
            _ => ("other", ""),
        };
        let (mut patients, mut queries, mut resources) = (0_usize, 0_usize, 0_usize);
        for entity in &exchange.entities {
            match entity {
                Entity::Patient { .. } | Entity::PatientReference(_) => {
                    patients = patients.saturating_add(1);
                }
                Entity::Query(_) => queries = queries.saturating_add(1),
                _ => resources = resources.saturating_add(1),
            }
        }
        tracing::info!(
            target: AUDIT_TARGET,
            profile = exchange.kind.profile,
            subtypes = ?subtypes,
            action = exchange.kind.action,
            recorded = %exchange.recorded,
            outcome = exchange.outcome.code(),
            direction,
            peer,
            on_behalf = logged_as(&exchange.on_behalf),
            patients,
            queries,
            resources,
            entities = "recorded, not logged",
            "BALP audit record"
        );
        Ok(())
    }
}
