// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one issuer whose tokens are verified by RFC 7662 introspection, in
//! place of a key set, by `ferrohealth-smart`'s introspection check.
//!
//! The check reads the issuer's metadata once, when a token first needs it,
//! so a gate is built without calling out; a read that fails leaves the
//! token unverifiable, which the gate answers `503`, and is tried again on
//! the next token. The check itself asks the endpoint about the token and
//! reads the answer as RFC 7662 §2.2 states it, classified under the
//! issuer's kind and profile. No specification governs when the metadata is
//! first read: our own design.

use std::sync::Arc;

use ferrohealth_smart::resource::introspection::{IntrospectionSettings, Introspector};
use ferrohealth_smart::resource::{Caller as SmartCaller, Rejection};
use tokio::sync::OnceCell;

use crate::config::auth::IssuerSettings;
use crate::security::Event;
use crate::security::TARGET;

/// The introspected issuer, its check built at its first use.
#[derive(Debug)]
pub(super) struct Introspected {
    /// The settings the check is built from.
    settings: IntrospectionSettings,
    /// The check, once the issuer's metadata was read.
    check: OnceCell<Introspector>,
    /// The issuer's trust entry, attached to every caller it admits.
    trusted: Arc<IssuerSettings>,
}

impl Introspected {
    /// Returns the introspected issuer `settings` describe; nothing is asked
    /// yet.
    pub(super) fn new(settings: IntrospectionSettings, trusted: Arc<IssuerSettings>) -> Self {
        Self {
            settings,
            check: OnceCell::new(),
            trusted,
        }
    }

    /// Returns the issuer identifier.
    pub(super) fn issuer(&self) -> &str {
        &self.settings.issuer
    }

    /// Whether the issuer is declared a national contact point.
    pub(super) fn contact_point(&self) -> bool {
        self.trusted.national_contact_point.is_some()
    }

    /// Returns the caller `token` names, as the endpoint answers for it.
    pub(super) async fn introspect(&self, token: &str) -> Result<SmartCaller, Rejection> {
        let check = self
            .check
            .get_or_try_init(|| async {
                Introspector::discover(self.settings.clone())
                    .await
                    .map(|check| check.with_extension(Arc::clone(&self.trusted)))
            })
            .await
            .map_err(|source| {
                Event::IntrospectionUnavailable.record();
                tracing::warn!(
                    target: TARGET,
                    event = "introspection-unavailable",
                    issuer = self.settings.issuer,
                    error = crate::chain(&source),
                    "an issuer's metadata could not be read, so its tokens cannot be introspected"
                );
                Rejection::upstream("introspection endpoint", source)
            })?;
        let outcome = check.authenticate(token).await;
        if let Err(Rejection::Upstream { .. }) = &outcome {
            Event::IntrospectionUnavailable.record();
            tracing::warn!(
                target: TARGET,
                event = "introspection-unavailable",
                issuer = self.settings.issuer,
                "an issuer's introspection endpoint did not answer, so its tokens cannot be verified"
            );
        }
        outcome
    }
}
