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
//! set are read when a token first needs them, so a gate is built without
//! calling out; a read that fails answers `503` and is tried again no sooner
//! than the issuer's `jwks_refresh_interval_seconds`. No specification
//! governs when the key set is first read: our own design.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use base64::Engine as _;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::{
    Caller as SmartCaller, Rejection, ResourceError, ResourceServer, Settings as KeySetSettings,
    Verifier,
};
use tokio::sync::OnceCell;

use crate::auth::introspection::Introspector;
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
    /// yet.
    pub(super) fn new(
        keyed: Vec<KeySetSettings>,
        introspected: Option<Introspector>,
        edge: bool,
    ) -> Self {
        Self {
            keyed: keyed
                .into_iter()
                .map(|settings| (settings.issuer.clone(), Keyed::new(settings)))
                .collect(),
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
            return keyed.server().await?.authenticate(token).await;
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

/// One issuer verified by its key set, its resource server built when a
/// token first needs it.
#[derive(Debug)]
struct Keyed {
    /// What the deployment configured.
    settings: KeySetSettings,
    /// The resource server, once its metadata and key set were read.
    server: OnceCell<Arc<ResourceServer>>,
    /// When the last read failed and why, so the next is not made inside
    /// the refresh interval and a token meanwhile meets the same failure.
    failed: Mutex<Option<(Instant, Arc<ResourceError>)>>,
}

impl Keyed {
    /// Returns the issuer `settings` names; nothing is read yet.
    fn new(settings: KeySetSettings) -> Self {
        Self {
            settings,
            server: OnceCell::new(),
            failed: Mutex::new(None),
        }
    }

    /// Returns the resource server, reading the issuer's metadata and key
    /// set when no read has succeeded yet.
    async fn server(&self) -> Result<&Arc<ResourceServer>, Rejection> {
        if let Some(server) = self.server.get() {
            return Ok(server);
        }
        let interval = self.settings.jwks_refresh_interval;
        let recent = self
            .failed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|(at, _)| at.elapsed() < interval)
            .map(|(_, source)| Arc::clone(source));
        if let Some(source) = recent {
            return Err(Rejection::Unavailable { source });
        }
        let read = self
            .server
            .get_or_try_init(|| discover(self.settings.clone()))
            .await;
        read.map_err(|source| {
            *self.failed.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((Instant::now(), Arc::clone(&source)));
            Event::KeySetUnavailable.record();
            tracing::warn!(
                target: TARGET,
                event = "key-set-unavailable",
                issuer = self.settings.issuer,
                error = crate::chain(source.as_ref()),
                "an issuer's metadata or key set could not be read, so its tokens cannot be verified"
            );
            Rejection::Unavailable { source }
        })
    }
}

/// Returns the resource server `settings` name, its metadata and key set
/// read.
///
/// The crate's discovery future holds a value that is not `Send` across an
/// await, so it runs to completion on a blocking thread of the runtime, and
/// the request's own future stays `Send`.
async fn discover(settings: KeySetSettings) -> Result<Arc<ResourceServer>, Arc<ResourceError>> {
    let runtime = tokio::runtime::Handle::current();
    let discovered = tokio::task::spawn_blocking(move || {
        runtime
            .block_on(ResourceServer::discover(settings))
            .map(Arc::new)
            .map_err(Arc::new)
    })
    .await;
    match discovered {
        Ok(read) => read,
        // NOTE: no specification governs this: our own design; a read that never
        // finished leaves the key set unread, which the gate answers as unavailable.
        Err(_unfinished) => Err(Arc::new(ResourceError::MissingMember {
            member: "key set, whose read did not finish",
        })),
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
