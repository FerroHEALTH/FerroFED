// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a token is verified: the issuers `ferrohealth-smart` checks against
//! their key sets, and the one introspected issuer, behind the crate's
//! `Verifier` seam.
//!
//! A token that is a JWS is routed by its `iss`, read before its signature
//! as RFC 7519 §7.2 lets a recipient do to select the key; the issuer it
//! names then checks everything, the issuer included (RFC 8725 §3.10). A
//! token that is no JWS goes to the introspected issuer, except in the edge
//! mode, which verifies one signed assertion. Each issuer's metadata and key
//! set are read when a token first needs them (`ResourceServer::lazy`), so a
//! gate is built without calling out; a read that fails answers `503` and is
//! tried again no sooner than the issuer's `jwks_refresh_interval_seconds`.
//! No specification governs when the key set is first read: our own design.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::{
    Caller as SmartCaller, LazyResourceServer, Rejection, ResourceError, ResourceServer,
    Settings as KeySetSettings, Verifier,
};

use crate::auth::introspection::Introspector;
use crate::config::auth::IssuerSettings;
use crate::facade::security::TARGET;
use crate::metrics::security::Event;

/// Every way the gate's issuers verify a token.
#[derive(Debug)]
pub(super) struct Verifiers {
    /// The issuers verified by their key sets, by issuer identifier.
    keyed: BTreeMap<String, Keyed>,
    /// The introspected issuer, when one is trusted.
    introspected: Option<Introspector>,
    /// Whether the credential is the edge's signed assertion, which is
    /// always a JWS.
    edge: bool,
}

impl Verifiers {
    /// Returns the verifiers of `keyed` and `introspected`; nothing is read
    /// yet. Each keyed issuer's trust entry rides on every caller it admits.
    #[expect(
        clippy::expect_used,
        reason = "every key-set issuer's settings passed Settings::validate when [auth] resolved, the one check ResourceServer::lazy repeats"
    )]
    pub(super) fn new(
        keyed: Vec<(KeySetSettings, Arc<IssuerSettings>)>,
        introspected: Option<Introspector>,
        edge: bool,
    ) -> Self {
        let keyed = keyed
            .into_iter()
            .map(|(settings, trusted)| {
                let issuer = settings.issuer.clone();
                let server = ResourceServer::lazy(settings)
                    .expect("the issuer's settings should have been validated when [auth] resolved")
                    .with_extension(trusted);
                (issuer, Keyed::new(server))
            })
            .collect();
        Self {
            keyed,
            introspected,
            edge,
        }
    }

    /// Returns the caller `token` authenticates.
    async fn caller(&self, token: &str) -> Result<SmartCaller, Rejection> {
        let malformed = Rejection::Invalid {
            reason: Reason::Malformed,
        };
        let untrusted = Rejection::Invalid {
            reason: Reason::UntrustedIssuer,
        };
        if token.split('.').count() != 3 {
            if self.edge {
                return Err(malformed);
            }
            return match &self.introspected {
                Some(introspector) => introspector.introspect(token).await,
                None => Err(untrusted),
            };
        }
        let issuer = unverified_issuer(token).ok_or(malformed)?;
        if let Some(keyed) = self.keyed.get(&issuer) {
            return keyed.authenticate(token).await;
        }
        match &self.introspected {
            Some(introspector) if introspector.issuer() == issuer => {
                introspector.introspect(token).await
            }
            _ => Err(untrusted),
        }
    }
}

impl Verifier for Verifiers {
    fn verify<'a>(
        &'a self,
        token: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<SmartCaller, Rejection>> + Send + 'a>> {
        Box::pin(self.caller(token))
    }
}

/// One issuer verified by its key set, which `ferrohealth-smart` reads when
/// a token first needs it and reads again no sooner than the refresh
/// interval after a failure.
#[derive(Debug)]
struct Keyed {
    /// The issuer's resource server.
    server: LazyResourceServer,
    /// The failure last logged, so each failed read is logged once.
    logged: Mutex<Option<Arc<ResourceError>>>,
}

impl Keyed {
    /// Returns the keyed issuer `server` verifies.
    fn new(server: LazyResourceServer) -> Self {
        Self {
            server,
            logged: Mutex::new(None),
        }
    }

    /// Returns the caller `token` authenticates, logging a failed read of the
    /// issuer's metadata or key set once.
    async fn authenticate(&self, token: &str) -> Result<SmartCaller, Rejection> {
        let outcome = self.server.authenticate(token).await;
        if let Err(Rejection::Unavailable { source }) = &outcome
            && self.server.get().is_none()
        {
            let mut logged = self.logged.lock().unwrap_or_else(PoisonError::into_inner);
            // NOTE: no specification governs this: our own design; inside the refresh
            // interval the crate hands back the same failure, which is logged once.
            if !logged.as_ref().is_some_and(|held| Arc::ptr_eq(held, source)) {
                *logged = Some(Arc::clone(source));
                Event::KeySetUnavailable.record();
                tracing::warn!(
                    target: TARGET,
                    event = "key-set-unavailable",
                    issuer = self.server.settings().issuer,
                    error = crate::chain(source.as_ref()),
                    "an issuer's metadata or key set could not be read, so its tokens cannot be verified"
                );
            }
        }
        outcome
    }
}

/// The `iss` of a token not yet verified.
#[derive(Debug, serde::Deserialize)]
struct Issued {
    /// `iss`.
    iss: String,
}

/// Returns the `iss` claim of `token`, read without checking anything.
fn unverified_issuer(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    // NOTE: RFC 7519 §7.2, a payload that names no string `iss` names no issuer,
    // which refuses the token as malformed rather than passing it on.
    serde_json::from_slice::<Issued>(&bytes)
        .ok()
        .map(|issued| issued.iss)
}
