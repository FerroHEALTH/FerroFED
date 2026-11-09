// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the gateway authenticates to a node (§13.1, N25, CP-17).
//!
//! Every OAuth 2.0 grant the gateway uses at a node's or a service's token
//! endpoint is `ferrohealth-smart`'s client: the client-credentials grant
//! (RFC 6749 §4.4) through its `TokenSource`, with a client secret, a
//! `private_key_jwt` assertion (RFC 7523 §2.2) or the TLS client
//! certificate (RFC 8705 §2), its tokens bound with `DPoP` (RFC 9449) or to
//! that certificate (RFC 8705 §3), under the FAPI 2.0 Security Profile and
//! with RFC 9396 `authorization_details` where configured. A node's onward
//! lane gives each verified caller a token of its own through the crate's
//! onward client ([`grant::lane`]). An onward token that cannot be obtained
//! fails that node; the gateway never dispatches unauthenticated. What a
//! node is told about the caller is a token of the gateway's own, signed
//! with its key ring ([`conveyance`](crate::conveyance); N24).
//!
//! An endpoint on the Dutch Generic Functions' Nuts track (Annex B §B.4)
//! obtains its token with a Verifiable Presentation of the gateway's
//! credentials instead, bound with `DPoP` to its own `DpopKey`
//! (`grant::nl::nuts`).
//!
//! This module holds what those grants share on the gateway's side: the
//! clock the Nuts token cache reads, and the scope a node's grant is
//! checked against.

use std::fmt;
use std::time::Instant;

use ferrohealth_smart::openehr::smart_scopes::{Compartment, SmartScope};

pub mod grant;
pub mod upstream;

/// The monotonic clock the Nuts token cache reads.
///
/// The gateway reads [`SystemClock`]; a test supplies one it moves itself.
pub trait Clock: Send + Sync + fmt::Debug {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The process's monotonic clock, [`Instant::now`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The scope a token is requested with.
///
/// A node's scope is read with [`Scope::parse`] in the SMART on openEHR
/// grammar (ITS-REST SMART App Launch, master08 §Resource Scopes): every
/// scope a resource scope of the `system` compartment, the one a
/// client-credentials grant to the gateway names, as `ferrohealth-smart` reads it.
/// An identity service's scope is read with [`Scope::tokens`], held to RFC
/// 6749 §3.3 alone, since its authorization server defines its own scopes
/// (IHE IUA ITI-71 §3.71.4.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    scopes: Vec<SmartScope>,
    text: String,
}

/// A scope that cannot be requested.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ScopeError {
    /// The scope names no scope at all.
    #[error("the scope is empty")]
    Empty,
    /// A scope is not a resource scope of the `system` compartment
    /// (master08 §Resource Scopes).
    #[error(
        "{scope:?} is not a system resource scope (system/<template-|composition-|aql-><id>.<cruds>)"
    )]
    NotSystemResource {
        /// The scope as written.
        scope: String,
    },
    /// A scope holds a character RFC 6749 §3.3 admits in no `scope-token`.
    #[error("{scope:?} holds a character no RFC 6749 §3.3 scope-token admits")]
    NotScopeToken {
        /// The scope as written.
        scope: String,
    },
}

impl Scope {
    /// Reads `text`, the space-delimited scopes of RFC 6749 §3.3, each with
    /// [`SmartScope::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`ScopeError::Empty`] when `text` names no scope, and
    /// [`ScopeError::NotSystemResource`] for a scope that is not a resource
    /// scope of the `system` compartment.
    pub fn parse(text: &str) -> Result<Self, ScopeError> {
        let mut scopes = Vec::new();
        for raw in text.split_whitespace() {
            let scope = SmartScope::parse(raw);
            match &scope {
                SmartScope::Resource(resource) if resource.compartment == Compartment::System => {}
                _ => {
                    return Err(ScopeError::NotSystemResource {
                        scope: raw.to_owned(),
                    });
                }
            }
            scopes.push(scope);
        }
        if scopes.is_empty() {
            return Err(ScopeError::Empty);
        }
        let text = SmartScope::format_all(&scopes);
        Ok(Self { scopes, text })
    }

    /// Reads `text`, the space-delimited scopes of RFC 6749 §3.3, each held
    /// to the `scope-token` grammar alone, as an identity service's
    /// authorization server defines them.
    ///
    /// # Errors
    ///
    /// Returns [`ScopeError::Empty`] when `text` names no scope, and
    /// [`ScopeError::NotScopeToken`] for a scope with a character outside
    /// `%x21 / %x23-5B / %x5D-7E`.
    pub fn tokens(text: &str) -> Result<Self, ScopeError> {
        let mut tokens = Vec::new();
        for raw in text.split_whitespace() {
            // NOTE: RFC 6749 §3.3, a scope-token is 1*( %x21 / %x23-5B / %x5D-7E ),
            // so a quote, a backslash, a control or a non-ASCII character is refused.
            let admitted = raw
                .bytes()
                .all(|byte| matches!(byte, 0x21 | 0x23..=0x5B | 0x5D..=0x7E));
            if !admitted {
                return Err(ScopeError::NotScopeToken {
                    scope: raw.to_owned(),
                });
            }
            tokens.push(raw);
        }
        if tokens.is_empty() {
            return Err(ScopeError::Empty);
        }
        Ok(Self {
            scopes: Vec::new(),
            text: tokens.join(" "),
        })
    }

    /// The scopes, as `ferrohealth-smart` read them; none for a scope read with
    /// [`Scope::tokens`].
    #[must_use]
    pub fn scopes(&self) -> &[SmartScope] {
        &self.scopes
    }

    /// The `scope` parameter the token request carries, one space apart
    /// (RFC 6749 §3.3): each scope in the canonical form of the SMART on
    /// openEHR grammar, as `ferrohealth-smart` prints it, or as written for a
    /// scope read with [`Scope::tokens`].
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
