// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PMIR identity feed (track 8 of §16.3, Annex A.4).
//!
//! It is the ITI-94 subscription the gateway keeps at the Patient Identity
//! Registry, and the route its ITI-93 messages arrive at, which drives the
//! identity-lifecycle hook (§5.2, "PMIR for identity lifecycle"; PMIR 1.6.0
//! §2:3.93, §2:3.94).
//!
//! The route sits under `{base}` at the configured path, outside the ITS-REST
//! surface and its client authentication. A message is applied only when it
//! carries the configured feed token as its bearer token, compared in
//! constant time; one without it is answered `401`, one that does not hold to
//! the PMIR profiles `400` (`415` for another media type), and either changes
//! nothing. An applied message drops the resolution bindings it could have
//! made stale ([`ferrofed_identity::lifecycle::change_of`]) and is answered
//! with the ITI-93 response (§2:3.93.4.2). The message carries Patient Master
//! Identities: the log, the metrics and every answer name the change kinds
//! and their counts, never an identifier.
//!
//! The subscription is created when the server starts, and checked every
//! `check_interval_s`: a Registry that cannot be reached, refuses it or
//! reports it in `error` or `off` shows on `GET /health/dependencies` as
//! `identity_registry`, and the gateway subscribes again when the Registry no
//! longer holds it. On a drain the subscription is deleted (§2:3.94.4.5).
//! The bindings' time-to-live bounds every change the gateway never hears
//! of. No specification governs the retry policy: our own design.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_identity::lifecycle::{self, LifecycleConfigError, RegistryAuth};
use ferrofed_registry::secret::Secret;
use http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use http::{HeaderMap, HeaderValue, StatusCode};
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::error::{FeedError, InvalidInput, SubscribeError};
use ihe_iti::pmir::feed::{EventKind, Feed, ResponseId, refusal};
use ihe_iti::pmir::subscription::{Criteria, Subscribed, SubscriptionRequest, SubscriptionStatus};

use crate::config::pmir::PmirSettings;
use crate::config::settings::Scheme;
use crate::facade::security::TARGET;
use crate::health::dependencies::Observed;
use crate::metrics::FeedResult;
use crate::state::AppState;

/// The media type of every FHIR answer the route gives (ITI TF-2 Appendix
/// Z.6).
const FHIR_JSON: &str = "application/fhir+json";

/// The `realm` of the route's `WWW-Authenticate` challenge (RFC 6750 §3).
const REALM: &str = "ferrofed-pmir";

/// The identity feed cannot be built from its settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityFeedError {
    /// The Registry's subscriber cannot be built.
    #[error("the PMIR subscriber cannot be built")]
    Subscriber(#[from] LifecycleConfigError),
    /// The subscription cannot be described.
    #[error("the PMIR subscription cannot be described")]
    Request(#[source] InvalidInput),
}

/// The identity feed a running gateway keeps: its subscription and the
/// route's credential.
pub struct IdentityFeed {
    subscriber: PmirSubscriber,
    request: SubscriptionRequest,
    token: Secret,
    domains: BTreeSet<String>,
    source: String,
    path: String,
    timeout: Duration,
    check_interval: Duration,
    subscribed: Mutex<Option<Subscribed>>,
    observed: Mutex<Observed>,
}

impl fmt::Debug for IdentityFeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentityFeed")
            .field("subscriber", &self.subscriber)
            .field("request", &self.request)
            .field("path", &self.path)
            .field("domains", &self.domains.len())
            .finish_non_exhaustive()
    }
}

impl IdentityFeed {
    /// The feed `settings` describe, whose changes are scoped by the members'
    /// `ehr_id` `domains` (Annex A.1). Nothing is sent yet.
    ///
    /// # Errors
    /// [`IdentityFeedError`] when the subscriber or the subscription cannot
    /// be built.
    pub fn new(
        settings: &PmirSettings,
        domains: BTreeSet<String>,
    ) -> Result<Self, IdentityFeedError> {
        let auth = match &settings.credentials {
            Some(Scheme::Bearer(token)) => RegistryAuth::Bearer(token.to_secret_string()),
            Some(Scheme::Basic { user, password }) => RegistryAuth::Basic {
                user: user.clone(),
                password: password.to_secret_string(),
            },
            // NOTE: no specification governs this: our own design; configuration
            // refuses an OAuth 2.0 grant here, so only the transport remains.
            Some(Scheme::OAuth2(_)) | None => RegistryAuth::None,
        };
        let subscriber = lifecycle::subscriber(&settings.url, &auth)?;
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
            request,
            token: settings.feed_token.clone(),
            domains,
            source: settings.callback_url.to_string(),
            path: settings.path.clone(),
            timeout: settings.timeout,
            check_interval: settings.check_interval,
            subscribed: Mutex::new(None),
            observed: Mutex::new(Observed::Unknown),
        })
    }

    /// The path under `{base}` the feed is served at.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The last state observed of the Registry, which
    /// `GET /health/dependencies` reports as `identity_registry`.
    #[must_use]
    pub fn observed(&self) -> Observed {
        *self.observed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The subscription the Registry holds, when the gateway created one.
    #[must_use]
    pub fn subscribed(&self) -> Option<Subscribed> {
        self.subscribed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Creates the subscription when the gateway holds none, or reads the
    /// status of the one it holds, and records what that showed of the
    /// Registry.
    ///
    /// A subscription the Registry reports `requested` or `active` is up; one
    /// in `error` or `off` is failing (§2:3.94.4.1.3, §2:3.94.4.4); one it no
    /// longer holds is forgotten, so the next check subscribes again.
    pub async fn check(&self) -> Observed {
        let observed = match self.subscribed() {
            None => match self.subscriber.subscribe(&self.request, self.timeout).await {
                Ok(subscribed) => {
                    tracing::info!("the PMIR subscription was created");
                    *self
                        .subscribed
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(subscribed);
                    Observed::Up
                }
                Err(error) => failed("the PMIR subscription could not be created", &error),
            },
            Some(subscribed) => match self.subscriber.status(&subscribed, self.timeout).await {
                Ok(SubscriptionStatus::Requested | SubscriptionStatus::Active) => Observed::Up,
                Ok(status) => {
                    tracing::warn!(
                        status = status.as_str(),
                        "the Patient Identity Registry reports the PMIR subscription inactive"
                    );
                    Observed::Failing
                }
                Err(error) => {
                    if matches!(
                        error.status(),
                        Some(StatusCode::NOT_FOUND | StatusCode::GONE)
                    ) {
                        *self
                            .subscribed
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) = None;
                    }
                    failed("the PMIR subscription could not be read", &error)
                }
            },
        };
        *self.observed.lock().unwrap_or_else(PoisonError::into_inner) = observed;
        observed
    }

    /// Checks the subscription now and every `check_interval_s` after, for as
    /// long as the server runs.
    pub async fn keep_subscribed(self: Arc<Self>) {
        loop {
            self.check().await;
            tokio::time::sleep(self.check_interval).await;
        }
    }

    /// Deletes the subscription the gateway holds, so the Registry stops
    /// sending the feed (§2:3.94.4.5); a failure is logged and the
    /// subscription is left to the Registry.
    pub async fn unsubscribe(&self) {
        let Some(subscribed) = self.subscribed() else {
            return;
        };
        match self.subscriber.unsubscribe(&subscribed, self.timeout).await {
            Ok(()) => tracing::info!("the PMIR subscription was deleted"),
            Err(error) => {
                tracing::warn!(
                    status = error.status().map(|status| status.as_u16()),
                    error = crate::chain(&error),
                    "the PMIR subscription could not be deleted"
                );
            }
        }
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

/// What a failed exchange shows of the Registry, logged with its status and
/// its error chain, which carries no identifier: a refusal or an answer that
/// breaks ITI-94 is failing, and no answer is down.
fn failed(message: &'static str, error: &SubscribeError) -> Observed {
    tracing::warn!(
        status = error.status().map(|status| status.as_u16()),
        error = crate::chain(error),
        "{message}"
    );
    if error.answered() {
        Observed::Failing
    } else {
        Observed::Down
    }
}

/// `POST {base}{pmir.path}`: one ITI-93 message, applied to the resolution
/// bindings when it is authenticated and holds to the PMIR profiles.
pub async fn feed(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(identity_feed) = state.identity_feed() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !identity_feed.authenticated(&headers) {
        tracing::warn!(
            target: TARGET,
            event = "identity-feed-refused",
            reason = "unauthenticated",
            "an ITI-93 message without the feed token was refused, and nothing was applied"
        );
        state.metrics().identity_feed(FeedResult::Unauthenticated);
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        if let Ok(challenge) = HeaderValue::from_str(&format!("Bearer realm=\"{REALM}\"")) {
            response.headers_mut().insert(WWW_AUTHENTICATE, challenge);
        }
        return response;
    }
    let media = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let message = match Feed::read(media, &body) {
        Ok(message) => message,
        Err(error) => return refused(&state, &error),
    };
    let Some(federation) = state.federation() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let change = lifecycle::change_of(&message, &identity_feed.domains);
    let dropped = change
        .as_ref()
        .map_or(0, |change| federation.identity_changed(change));
    tracing::info!(
        creates = message.count(EventKind::Create),
        updates = message.count(EventKind::Update),
        deletes = message.count(EventKind::Delete),
        merges = message.count(EventKind::Merge),
        changed = change.is_some(),
        dropped,
        "an ITI-93 message was applied to the resolution bindings"
    );
    state.metrics().identity_feed(FeedResult::Applied);
    let id = uuid::Uuid::new_v4().hyphenated().to_string();
    let written = ResponseId::new(&id)
        .ok()
        .and_then(|id| message.acknowledgement(&id, &identity_feed.source).ok());
    match written {
        Some(bytes) => fhir(StatusCode::OK, bytes),
        // NOTE: PMIR §2:3.93.4.2.2: the message was processed, so a response that
        // cannot be written leaves a bare 2xx, never a failure the Supplier retries.
        None => StatusCode::OK.into_response(),
    }
}

/// The answer to a message that does not hold to the PMIR profiles: `415`
/// for another media type, `400` otherwise, with the refusal's
/// `OperationOutcome` (§2:3.93.4.2.2).
fn refused(state: &AppState, error: &FeedError) -> Response {
    tracing::warn!(
        target: TARGET,
        event = "identity-feed-refused",
        reason = "malformed",
        error = %error,
        "an ITI-93 message that does not hold to PMIR was refused, and nothing was applied"
    );
    state.metrics().identity_feed(FeedResult::Refused);
    let status = match error {
        FeedError::NotFhirJson => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        _ => StatusCode::BAD_REQUEST,
    };
    match refusal(error) {
        Ok(bytes) => fhir(status, bytes),
        Err(_unwritable) => status.into_response(),
    }
}

/// `bytes` as a FHIR JSON answer of `status`.
fn fhir(status: StatusCode, bytes: Vec<u8>) -> Response {
    (status, [(CONTENT_TYPE, FHIR_JSON)], bytes).into_response()
}
