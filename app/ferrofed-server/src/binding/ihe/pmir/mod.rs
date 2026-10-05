// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PMIR identity feed (track 8 of §16.3, Annex A.4).
//!
//! It is the ITI-94 subscription the gateway keeps at the Patient Identity
//! Registry ([`subscription`]), and the route its ITI-93 messages arrive at
//! ([`route`]), which drives the identity-lifecycle hook (§5.2, "PMIR for
//! identity lifecycle"; PMIR 1.6.0 §2:3.93, §2:3.94).
//!
//! The route sits under `{base}` at the configured path, outside the ITS-REST
//! surface and its client authentication. A message is applied only when it
//! carries the configured feed token as its bearer token, compared in
//! constant time; one without it is answered `401`, one that does not hold to
//! the PMIR profiles `400` (`415` for another media type), and either changes
//! nothing. An applied message drops the resolution bindings it could have
//! made stale ([`ferrofed_identity::ihe::pmir::change_of`]) and is answered
//! with the ITI-93 response (§2:3.93.4.2). The message carries Patient Master
//! Identities: the log, the metrics and every answer name the change kinds
//! and their counts, never an identifier. The bindings' time-to-live bounds
//! every change the gateway never hears of.

pub mod config;
pub mod route;
pub mod subscription;

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ferrofed_identity::ihe::pmir::{self, PmirConfigError};
use ferrofed_registry::secret::Secret;
use http::HeaderMap;
use http::header::AUTHORIZATION;
use std::sync::Arc;

use ihe_iti::balp::{AuditError, AuditRecorder};
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::audit::received;
use ihe_iti::pmir::error::InvalidInput;
use ihe_iti::pmir::feed::Feed;
use ihe_iti::pmir::subscription::{Criteria, SubscriptionRequest};
use url::Url;

use crate::binding::ihe::pmir::config::{OnDrain, PmirSettings};
use crate::service::{self, GrantRefused, TlsRefused};
use ferrofed_registry::health::Observed;
use subscription::{RegistryFault, Watch};

/// The identity feed cannot be built from its settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityFeedError {
    /// The Registry's subscriber cannot be built.
    #[error("the PMIR subscriber cannot be built")]
    Subscriber(#[from] PmirConfigError),
    /// The subscription cannot be described.
    #[error("the PMIR subscription cannot be described")]
    Request(#[source] InvalidInput),
    /// The Registry's base URL does not parse.
    #[error("the PMIR Registry base URL is not a URL")]
    Registry(#[source] url::ParseError),
    /// `[pmir.credentials]` names a grant, which only a node takes.
    #[error("the PMIR credentials cannot be used")]
    Grant(#[source] GrantRefused),
    /// The TLS material of `[pmir]` does not read.
    #[error("the PMIR TLS material cannot be used")]
    Tls(#[source] TlsRefused),
}

/// The identity feed a running gateway keeps: its subscription and the
/// route's credential.
pub struct IdentityFeed {
    subscriber: PmirSubscriber,
    registry: Url,
    audit: Option<Arc<dyn AuditRecorder>>,
    request: SubscriptionRequest,
    token: Secret,
    domains: BTreeSet<String>,
    source: String,
    path: String,
    timeout: Duration,
    check_interval: Duration,
    on_drain: OnDrain,
    watch: Mutex<Watch>,
}

impl fmt::Debug for IdentityFeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentityFeed")
            .field("subscriber", &self.subscriber)
            .field("request", &self.request)
            .field("path", &self.path)
            .field("domains", &self.domains.len())
            .field("on_drain", &self.on_drain)
            .finish_non_exhaustive()
    }
}

impl IdentityFeed {
    /// The feed `settings` describe, whose changes are scoped by the members'
    /// `ehr_id` `domains` (Annex A.1). Nothing is sent yet.
    ///
    /// Every ITI-94 exchange and every ITI-93 message received is recorded
    /// through `audit`, when it is given (PMIR §2:3.93.5.1, §2:3.94.5.1).
    ///
    /// # Errors
    /// [`IdentityFeedError`] when the subscriber or the subscription cannot
    /// be built.
    pub fn new(
        settings: &PmirSettings,
        domains: BTreeSet<String>,
        audit: Option<Arc<dyn AuditRecorder>>,
    ) -> Result<Self, IdentityFeedError> {
        let auth = service::authentication("pmir.credentials", settings.credentials.as_ref())
            .map_err(IdentityFeedError::Grant)?;
        let tls = service::tls_of("pmir", &settings.tls).map_err(IdentityFeedError::Tls)?;
        let subscriber = pmir::subscriber(&settings.url, &auth, &tls)?;
        // NOTE: PMIR §2:3.94.5.1: each ITI-94 exchange is audited, and one whose
        // record is refused fails like a Registry that did not answer.
        let subscriber = match &audit {
            Some(recorder) => subscriber.audited(Arc::clone(recorder)),
            None => subscriber,
        };
        let registry = Url::parse(settings.url.expose()).map_err(IdentityFeedError::Registry)?;
        let criteria = match &settings.identifier_system {
            Some(system) => {
                Criteria::identifier_system(system).map_err(IdentityFeedError::Request)?
            }
            None => Criteria::AllPatients,
        };
        let request = SubscriptionRequest::new(criteria, settings.callback_url.clone())
            .map_err(IdentityFeedError::Request)?;
        Ok(Self {
            subscriber,
            registry,
            audit,
            request,
            token: settings.feed_token.clone(),
            domains,
            source: settings.callback_url.to_string(),
            path: settings.path.clone(),
            timeout: settings.timeout,
            check_interval: settings.check_interval,
            on_drain: settings.on_drain,
            watch: Mutex::new(Watch::default()),
        })
    }

    /// The path under `{base}` the feed is served at.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Records the ITI-93 message `feed`, as read, or a message refused
    /// before it could be read, as the Feed audit profile fixes it (PMIR
    /// §2:3.93.5.1), when the feed is audited.
    ///
    /// The Registry is named by its configured base: the message carried the
    /// feed token, which only the subscription's Registry holds.
    ///
    /// # Errors
    /// The [`AuditError`] of a record the recorder refused.
    pub async fn record(&self, feed: Option<&Feed>) -> Result<(), AuditError> {
        let Some(recorder) = &self.audit else {
            return Ok(());
        };
        recorder
            .record(received(&self.registry, self.request.endpoint(), feed))
            .await
    }

    /// The members' `ehr_id` domains a change is scoped by.
    #[must_use]
    pub fn domains(&self) -> &BTreeSet<String> {
        &self.domains
    }

    /// The endpoint the gateway answers the feed from, the `source` of each
    /// ITI-93 response.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The last state observed of the Registry, which
    /// `GET /health/dependencies` reports as `identity_registry`.
    #[must_use]
    pub fn observed(&self) -> Observed {
        self.watch().observed
    }

    /// Why the Registry is not up, which `GET /health/dependencies` reports
    /// as `identity_registry_fault`; `None` while it is up or not yet asked.
    #[must_use]
    pub fn fault(&self) -> Option<RegistryFault> {
        self.watch().fault
    }

    /// How many checks in a row have failed, which the wait before the next
    /// one grows with ([`subscription::backoff`]).
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.watch().failures
    }

    /// The subscription state, under its lock; the guard is never held
    /// across an `.await`.
    fn watch(&self) -> MutexGuard<'_, Watch> {
        // NOTE: no specification governs this: our own design; a panic while the
        // lock was held leaves a state that the next check corrects.
        self.watch.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `headers` carry the feed token as their one bearer token,
    /// compared in constant time (RFC 6750 §2.1).
    fn authenticated(&self, headers: &HeaderMap) -> bool {
        let mut values = headers.get_all(AUTHORIZATION).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return false;
        };
        let Some((scheme, token)) = value.to_str().ok().and_then(|text| text.split_once(' '))
        else {
            return false;
        };
        scheme.eq_ignore_ascii_case("bearer")
            && aws_lc_rs::constant_time::verify_slices_are_equal(
                token.trim_start_matches(' ').as_bytes(),
                self.token.expose().as_bytes(),
            )
            .is_ok()
    }
}
