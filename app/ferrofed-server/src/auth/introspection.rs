// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one issuer whose tokens are verified by RFC 7662 introspection, in
//! place of a key set: FerroFED's own check beside the issuers
//! `ferrohealth-smart` verifies, which hands the gate the same caller.
//!
//! The gateway asks the endpoint about the token, authenticating with its
//! client credentials in the Basic scheme as RFC 6749 §2.3.1 encodes them
//! (RFC 7662 §2.1), and reads the answer as §2.2 states it: an inactive
//! token, another issuer, an audience that is not the gateway's, or a
//! lifetime outside the clock skew is a token that does not hold. A call
//! that does not answer in time, answers with anything but `200`, or
//! answers more than [`MAX_BODY`] bytes leaves the token unverifiable, which
//! the gate answers `503`. No specification governs the bounds: our own
//! design.

use std::sync::Arc;

use ferrohealth_smart::metadata::DiscoveryError;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::{Caller as SmartCaller, Rejection};
use http::StatusCode;
use tokio::sync::OnceCell;

use crate::auth::claims::Introspected;
use crate::config::auth::{Introspection, IssuerSettings};
use crate::facade::security::TARGET;
use crate::metrics::security::Event;

/// The largest introspection answer the gateway reads.
pub(super) const MAX_BODY: usize = 256 * 1024;

/// Returns `caller` carrying every member of the introspection answer
/// `bytes`, or `None` for an answer that is no JSON object.
#[expect(
    clippy::disallowed_types,
    reason = "the token-claims seam: ferrohealth-smart's caller carries its claims as a JSON map"
)]
fn claims(caller: SmartCaller, bytes: &[u8]) -> Option<SmartCaller> {
    // NOTE: RFC 7662 §2.2, the answer is a JSON object, which the checked members
    // were already read from; one that is not was refused before this.
    let members: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(bytes).ok()?;
    Some(caller.with_claims(members))
}

/// The introspected issuer and the HTTP client its endpoint is asked with.
#[derive(Debug)]
pub(super) struct Introspector {
    /// The issuer identifier.
    issuer: String,
    /// The endpoint, the gateway's credentials there, and the answer's
    /// rules.
    settings: Introspection,
    /// The client, built at its first use.
    client: OnceCell<reqwest::Client>,
    /// The issuer's trust entry, attached to every caller it admits.
    trusted: Arc<IssuerSettings>,
}

impl Introspector {
    /// Returns the introspector of `issuer` at the endpoint `settings`
    /// names; nothing is asked yet.
    pub(super) fn new(
        issuer: String,
        settings: Introspection,
        trusted: Arc<IssuerSettings>,
    ) -> Self {
        Self {
            issuer,
            settings,
            client: OnceCell::new(),
            trusted,
        }
    }

    /// Returns the issuer identifier.
    pub(super) fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the caller `token` names, as the endpoint answers for it.
    pub(super) async fn introspect(&self, token: &str) -> Result<SmartCaller, Rejection> {
        let (bytes, answer) = match self.ask(token).await {
            Ok(read) => read,
            Err(source) => {
                Event::IntrospectionUnavailable.record();
                tracing::warn!(
                    target: TARGET,
                    event = "introspection-unavailable",
                    issuer = self.issuer,
                    error = crate::chain(&source),
                    "an issuer's introspection endpoint did not answer, so its tokens cannot be verified"
                );
                return Err(Rejection::upstream("introspection endpoint", source));
            }
        };
        let invalid = |reason: &'static str| Rejection::Invalid {
            reason: Reason::Other(reason),
        };
        if !answer.active {
            return Err(invalid("inactive"));
        }
        if answer.iss.as_deref().is_some_and(|iss| iss != self.issuer) {
            return Err(Rejection::Invalid {
                reason: Reason::Issuer,
            });
        }
        let names_us = answer.aud.as_ref().is_some_and(|aud| {
            self.settings
                .audience
                .iter()
                .any(|audience| aud.names(audience))
        });
        if !names_us {
            return Err(Rejection::Invalid {
                reason: Reason::Audience,
            });
        }
        let now = jiff::Timestamp::now().as_second();
        let skew = i64::try_from(self.settings.clock_skew.as_secs()).unwrap_or(i64::MAX);
        let exp = answer.exp.ok_or(Rejection::Invalid {
            reason: Reason::MissingClaim,
        })?;
        if exp.saturating_add(skew) < now {
            return Err(Rejection::Invalid {
                reason: Reason::Expired,
            });
        }
        if answer.nbf.is_some_and(|nbf| nbf.saturating_sub(skew) > now) {
            return Err(Rejection::Invalid {
                reason: Reason::NotYetValid,
            });
        }
        let (Some(subject), Some(client_id)) = (answer.sub, answer.client_id) else {
            return Err(Rejection::Invalid {
                reason: Reason::MissingClaim,
            });
        };
        let scope = answer.scope.unwrap_or_default();
        let caller = SmartCaller::new(subject, Some(client_id), &scope, None, false)
            .with_issuer(self.issuer.clone())
            .with_extension(Arc::clone(&self.trusted));
        claims(caller, &bytes).ok_or_else(|| invalid("answer"))
    }

    /// Asks the endpoint about `token` (RFC 7662 §2.1) and returns the
    /// answer's bytes and its checked members.
    async fn ask(&self, token: &str) -> Result<(Vec<u8>, Introspected), DiscoveryError> {
        let endpoint = self.settings.endpoint.clone();
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", token)
            .append_pair("token_type_hint", "access_token")
            .finish();
        let encoded =
            |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let transport = |source| DiscoveryError::Transport {
            url: endpoint.clone(),
            source,
        };
        let client = self
            .client
            .get_or_try_init(|| async {
                // NOTE: no specification governs this: our own design; an answer is read
                // where it was asked, never from where a redirect points.
                reqwest::Client::builder()
                    .timeout(self.settings.timeout)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
            })
            .await
            .map_err(transport)?;
        let mut response = client
            .post(endpoint.clone())
            .basic_auth(
                encoded(&self.settings.client_id),
                Some(encoded(self.settings.client_secret.expose())),
            )
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
            .body(form)
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if body.len().saturating_add(chunk.len()) > MAX_BODY {
                return Err(DiscoveryError::TooLarge { url: endpoint });
            }
            body.extend_from_slice(&chunk);
        }
        if status != StatusCode::OK {
            return Err(DiscoveryError::Status {
                url: endpoint,
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
        let answer = serde_json::from_slice(&body).map_err(|source| DiscoveryError::Metadata {
            url: endpoint,
            source,
        })?;
        Ok((body, answer))
    }
}
