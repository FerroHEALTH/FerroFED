// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token the gateway obtains for itself with the OAuth 2.0 client
//! credentials grant (RFC 6749 §4.4), handed to a node client or an IHE FHIR
//! client as the credentials provider each one takes.
//!
//! `ferrohealth-smart` fetches, caches and renews the token; this module
//! hands it to the generated ITS-REST client in the form it asks for. A
//! token the upstream refuses with `401` is dropped, so the next attempt
//! asks the token endpoint once more.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use ferrohealth_smart::client::cache::TokenSource;
use ferrohealth_smart::client::{ClientCredentials, TokenError};
use http::HeaderValue;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider};
use secrecy::SecretString;

/// The scheme of an RFC 6750 §2.1 bearer credential, with its separator.
const BEARER: &str = "Bearer ";

/// One upstream's token, cheap to share: every client of the upstream reads
/// one cache.
pub struct UpstreamToken {
    /// The cache the client credentials grant fills.
    source: TokenSource,
    /// The `Authorization` value last handed out, which a `401` drops.
    last: Mutex<Option<HeaderValue>>,
}

impl UpstreamToken {
    /// Returns the token of the grant `credentials` describe; nothing is
    /// fetched until a client asks for it.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] when the HTTP engine of the token client cannot
    /// be built.
    pub fn new(credentials: ClientCredentials) -> Result<Self, TokenError> {
        Ok(Self {
            source: TokenSource::new(credentials)?,
            last: Mutex::new(None),
        })
    }

    /// Returns the token as the provider a client takes.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] as [`UpstreamToken::new`] does.
    pub fn provider(credentials: ClientCredentials) -> Result<Arc<Self>, TokenError> {
        Self::new(credentials).map(Arc::new)
    }
}

impl fmt::Debug for UpstreamToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpstreamToken").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl CredentialsProvider for UpstreamToken {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        let token = self.source.token().await.map_err(CredentialsError::new)?;
        let header = token.header().clone();
        // NOTE: RFC 6750 §2.1, the cache renders the token as `Bearer <token>`, and the
        // client takes the token alone, which it renders the same way.
        let bare = header
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix(BEARER))
            .map(|value| SecretString::from(value.to_owned()))
            .ok_or_else(|| CredentialsError::new(TokenError::Unusable))?;
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Some(header);
        Ok(Credentials::bearer(bare))
    }

    fn refused(&self) {
        let stale = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(stale) = stale {
            self.source.invalidate(&stale);
        }
    }
}
