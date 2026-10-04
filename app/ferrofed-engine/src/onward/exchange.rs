// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Token exchange: an onward token per verified caller (§13.1, N25, N26;
//! RFC 8693).
//!
//! For an endpoint whose [`Grant`] is
//! [`GrantKind::TokenExchange`](crate::onward::GrantKind::TokenExchange), each
//! call made on behalf of a verified caller exchanges the caller's token at
//! the node's token endpoint: the caller's verified access token is the
//! `subject_token`, an RFC 7523 assertion of the gateway the `actor_token`,
//! and the request names the node with `resource` (RFC 8707 §2) and asks for
//! the caller's scopes that cover the operation, never more (N26). A call
//! that no scope of the caller covers is refused with nothing sent
//! ([`ExchangeError::NoScope`]): without `scope` the authorization server
//! would choose one (RFC 8693 §2.1). The issued token names the caller as its subject and the gateway as the
//! actor, so the node's own access decision sees both (RFC 8693 §4.1).
//!
//! Only a caller verified by its token's signature or by introspection has
//! a token to exchange. A caller the edge asserted has none, and a call on
//! its behalf is refused before anything is sent ([`ExchangeError::Edge`]).
//! The gateway's own requests, which have no caller, use the
//! client-credentials grant at the same token endpoint.
//!
//! An issued token is cached per caller's token and scope until
//! [`REFRESH_MARGIN`] before the end of its stated lifetime, at most
//! [`CACHE_CAPACITY`] per endpoint, and dropped when the node answers `401`.
//! The cache holds the SHA-256 of the caller's token as its key, never the
//! token. No specification governs the caching: our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider, Transport};
use secrecy::{ExposeSecret, SecretString};

use crate::dispatch::SharedCredentials;
use crate::hygiene::Withheld;
use crate::onward::conveyance::{Conveyance, Principal, Verification};
use crate::onward::keys::KeyRing;
use crate::onward::provider::{ClientCredentials, MAX_ASSERTION_LIFETIME, REFRESH_MARGIN};
use crate::onward::token::{self, Subject, TokenError};
use crate::onward::{Clock, Grant};

/// The most exchanged tokens one endpoint keeps.
// NOTE: no specification governs this bound: our own design; a full cache
// first drops the tokens that expire soonest.
pub const CACHE_CAPACITY: usize = 1024;

/// The caller's verified access token, and the scope its exchange asks for.
///
/// `Debug` shows neither.
#[derive(Clone)]
pub struct SubjectToken {
    token: SecretString,
    scope: String,
}

impl SubjectToken {
    /// The caller's verified `token`, exchanged for `scope`: the caller's
    /// granted scopes that cover the operation, space-separated. An empty
    /// scope, an operation no scope covers, is never exchanged
    /// ([`ExchangeError::NoScope`]).
    #[must_use]
    pub fn new(token: SecretString, scope: impl Into<String>) -> Self {
        Self {
            token,
            scope: scope.into(),
        }
    }

    /// The caller's token.
    #[must_use]
    pub fn token(&self) -> &SecretString {
        &self.token
    }

    /// The scope the exchange asks for.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

impl fmt::Debug for SubjectToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubjectToken").finish_non_exhaustive()
    }
}

/// A source of onward credentials that depend on whom a call is made for.
pub trait OnBehalf: Send + Sync + fmt::Debug {
    /// The credentials provider of one call conveying `conveyance`, which
    /// refuses to send the caller's token when it carries an identifier of
    /// `withheld` (§5.4.1, N33).
    fn provider(&self, conveyance: &Conveyance, withheld: &Arc<Withheld>) -> SharedCredentials;
}

/// A shared [`OnBehalf`] source of one endpoint.
pub type SharedOnBehalf = Arc<dyn OnBehalf>;

/// Why a call's token could not be exchanged, so nothing was sent.
///
/// No variant carries a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExchangeError {
    /// The caller was asserted by the edge, so the gateway holds no
    /// verified token of the caller to exchange.
    #[error("the caller was asserted by the edge, so the gateway holds no token of it to exchange")]
    Edge,
    /// The call conveys a verified caller but carries no token of it.
    #[error("the call carries no verified token of its caller to exchange")]
    NoSubject,
    /// No granted scope of the caller covers the operation, so an exchange
    /// would ask for none and let the authorization server choose (RFC 8693
    /// §2.1); it is never sent (N26).
    #[error("no scope of the caller covers the operation, so its token was not exchanged")]
    NoScope,
    /// The caller's token or scope carries an identifier the request
    /// withholds, so it was not sent to the token endpoint (§5.4.1, N33).
    #[error("the caller's token carries a withheld patient identifier, so it was not exchanged")]
    Withheld,
    /// The token endpoint gave no token.
    #[error("the token exchange failed")]
    Token(#[source] TokenError),
}

/// The token-exchange grant of one endpoint.
///
/// `Debug` names the endpoint and the grant, never a token.
pub struct Exchange<T>(Arc<Inner<T>>);

/// What every call of one endpoint's exchange shares.
struct Inner<T> {
    endpoint: EndpointId,
    grant: Grant,
    keys: Arc<KeyRing>,
    lifetime: Duration,
    transport: T,
    timeout: Duration,
    clock: Arc<dyn Clock>,
    cache: Mutex<Cache>,
    gateway: SharedCredentials,
}

impl<T: Transport + Clone + 'static> Exchange<T> {
    /// The exchange of `endpoint`'s `grant`, signing each assertion with
    /// the current key of `keys`, valid for `lifetime`, and sending each
    /// token request over `transport`, waiting at most `timeout` for it.
    ///
    /// `lifetime` is held to [`MAX_ASSERTION_LIFETIME`]. The gateway's own
    /// requests use the client-credentials grant of the same endpoint.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        grant: Grant,
        keys: Arc<KeyRing>,
        (lifetime, timeout): (Duration, Duration),
        transport: T,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let gateway: SharedCredentials = Arc::new(ClientCredentials::new(
            endpoint.clone(),
            grant.as_client_credentials(),
            Arc::clone(&keys),
            (lifetime, timeout),
            transport.clone(),
            Arc::clone(&clock),
        ));
        Self(Arc::new(Inner {
            endpoint,
            grant,
            keys,
            lifetime: lifetime.min(MAX_ASSERTION_LIFETIME),
            transport,
            timeout,
            clock,
            cache: Mutex::new(Cache::default()),
            gateway,
        }))
    }

    /// The grant this exchange obtains tokens with.
    #[must_use]
    pub fn grant(&self) -> &Grant {
        &self.0.grant
    }

    /// The number of exchanged tokens cached.
    #[must_use]
    pub fn cached(&self) -> usize {
        self.0.lock().entries.len()
    }
}

impl<T: Transport + Clone + 'static> OnBehalf for Exchange<T> {
    fn provider(&self, conveyance: &Conveyance, withheld: &Arc<Withheld>) -> SharedCredentials {
        match conveyance.principal() {
            Principal::Gateway => Arc::clone(&self.0.gateway),
            Principal::Caller(caller) => Arc::new(OnBehalfOf {
                inner: Arc::clone(&self.0),
                // NOTE: RFC 8693 §2.1, only a token the gateway itself verified is a
                // caller's subject_token; an edge assertion never is.
                edge: !matches!(
                    caller.verified_by,
                    Verification::Signature | Verification::Introspection
                ),
                subject: conveyance.subject().cloned(),
                withheld: Arc::clone(withheld),
            }),
        }
    }
}

impl<T> fmt::Debug for Exchange<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exchange")
            .field("endpoint", &self.0.endpoint)
            .field("grant", &self.0.grant)
            .finish_non_exhaustive()
    }
}

impl<T> Inner<T> {
    fn lock(&self) -> std::sync::MutexGuard<'_, Cache> {
        // NOTE: no specification governs this: our own design; a panic while the
        // lock was held leaves at worst a stale token, which a 401 drops.
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T: Transport> Inner<T> {
    /// The token of `subject`, from the cache or a new exchange.
    async fn credentials(
        &self,
        subject: &SubjectToken,
        key: &Key,
    ) -> Result<Credentials, TokenError> {
        let now = self.clock.now();
        if let Some(fresh) = self.lock().fresh(key, now) {
            return Ok(fresh);
        }
        let started = self.clock.now();
        let key_pair = self.keys.current();
        let assertion = token::assertion(&self.grant, key_pair, self.lifetime)?;
        let actor = token::assertion(&self.grant, key_pair, self.lifetime)?;
        let issued = token::exchange(
            &self.grant,
            Subject {
                token: subject.token(),
                scope: subject.scope(),
            },
            (&assertion, &actor),
            &self.transport,
            self.timeout,
        )
        .await?;
        let refresh_at = issued
            .expires_in
            .and_then(|lifetime| lifetime.checked_sub(REFRESH_MARGIN))
            .filter(|left| !left.is_zero())
            .and_then(|left| started.checked_add(left));
        if let Some(refresh_at) = refresh_at {
            self.lock().insert(
                key.clone(),
                Cached {
                    credentials: issued.credentials.clone(),
                    refresh_at,
                },
                self.clock.now(),
            );
        }
        tracing::debug!(
            endpoint = %self.endpoint,
            cached = refresh_at.is_some(),
            "an onward token was exchanged for a caller"
        );
        Ok(issued.credentials)
    }
}

/// The credentials of one call made on behalf of a verified caller.
struct OnBehalfOf<T> {
    inner: Arc<Inner<T>>,
    edge: bool,
    subject: Option<SubjectToken>,
    withheld: Arc<Withheld>,
}

impl<T> OnBehalfOf<T> {
    /// The subject to exchange and its cache key, or why there is none.
    fn subject(&self) -> Result<(&SubjectToken, Key), ExchangeError> {
        if self.edge {
            return Err(ExchangeError::Edge);
        }
        let subject = self.subject.as_ref().ok_or(ExchangeError::NoSubject)?;
        if subject.scope().split_whitespace().next().is_none() {
            return Err(ExchangeError::NoScope);
        }
        if carries_withheld(subject, &self.withheld) {
            return Err(ExchangeError::Withheld);
        }
        Ok((subject, Key::of(subject)))
    }
}

#[async_trait::async_trait]
impl<T: Transport + 'static> CredentialsProvider for OnBehalfOf<T> {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        let (subject, key) = self.subject().map_err(CredentialsError::new)?;
        // NOTE: no specification governs this: our own design; the token endpoint's
        // own error is the direct cause, so the node's report can name its code.
        self.inner
            .credentials(subject, &key)
            .await
            .map_err(CredentialsError::new)
    }

    fn refused(&self) {
        if let Ok((_, key)) = self.subject() {
            self.inner.lock().forget(&key);
        }
    }
}

impl<T> fmt::Debug for OnBehalfOf<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OnBehalfOf")
            .field("endpoint", &self.inner.endpoint)
            .field("edge", &self.edge)
            .finish_non_exhaustive()
    }
}

/// Whether `subject`'s token or scope carries an identifier of `withheld`,
/// raw or in the decoded payload of a JWS.
fn carries_withheld(subject: &SubjectToken, withheld: &Withheld) -> bool {
    let token = subject.token().expose_secret();
    if withheld.carried_by(token) || withheld.carried_by(subject.scope()) {
        return true;
    }
    // NOTE: RFC 7515 §7.1, the payload is the second segment, base64url; a token
    // that is no JWS is opaque, and its raw text is all there is to read.
    let payload = token
        .split('.')
        .nth(1)
        .and_then(|segment| URL_SAFE_NO_PAD.decode(segment).ok());
    payload.is_some_and(|payload| withheld.carried_by(&String::from_utf8_lossy(&payload)))
}

/// The cache key of one exchanged token: the SHA-256 of the caller's token,
/// and the scope asked for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    subject: Vec<u8>,
    scope: String,
}

impl Key {
    fn of(subject: &SubjectToken) -> Self {
        Self {
            subject: digest(&SHA256, subject.token().expose_secret().as_bytes())
                .as_ref()
                .to_vec(),
            scope: subject.scope().to_owned(),
        }
    }
}

/// One exchanged token, and when it is replaced.
struct Cached {
    credentials: Credentials,
    refresh_at: Instant,
}

/// The exchanged tokens of one endpoint.
#[derive(Default)]
struct Cache {
    entries: BTreeMap<Key, Cached>,
}

impl Cache {
    /// The token cached under `key`, when it is still fresh at `now`.
    fn fresh(&self, key: &Key, now: Instant) -> Option<Credentials> {
        self.entries
            .get(key)
            .filter(|cached| now < cached.refresh_at)
            .map(|cached| cached.credentials.clone())
    }

    /// Caches `cached` under `key`, first dropping every token stale at
    /// `now`, then, when the cache is full, the one replaced soonest.
    fn insert(&mut self, key: Key, cached: Cached, now: Instant) {
        self.entries.retain(|_, entry| now < entry.refresh_at);
        while self.entries.len() >= CACHE_CAPACITY {
            let soonest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.refresh_at)
                .map(|(key, _)| key.clone());
            match soonest {
                Some(soonest) => {
                    self.entries.remove(&soonest);
                }
                None => break,
            }
        }
        self.entries.insert(key, cached);
    }

    /// Drops the token cached under `key`.
    fn forget(&mut self, key: &Key) {
        self.entries.remove(key);
    }
}
