// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The references the security log names a caller by: an HMAC-SHA256 of the
//! caller's subject or client under a key the process draws when the gate is
//! built.
//!
//! A reference is stable for the life of the process, so the log correlates
//! one caller's events, and cannot be reversed without the key, which never
//! leaves memory: the log names no caller (§5.4.1, N33, as the gateway holds
//! it for every surface but the audit record). No specification governs the
//! reference: our own design.

use std::fmt;
use std::fmt::Write as _;

use aws_lc_rs::hmac::{self, HMAC_SHA256, Key};
use aws_lc_rs::rand::SystemRandom;

use crate::auth::caller::Caller;
use crate::facade::security::TARGET;

/// What the log writes in place of a reference when the process could draw
/// no key.
pub(crate) const UNAVAILABLE: &str = "unavailable";

/// How many bytes of the HMAC a reference shows, as hexadecimal.
const SHOWN: usize = 12;

/// The key the references of one process are made under.
pub(crate) struct References(Option<Key>);

impl References {
    /// Draws a fresh key from the system's random source.
    #[must_use]
    pub(crate) fn fresh() -> Self {
        // NOTE: no specification governs this: our own design; a random source that
        // fails leaves no key, and every reference is then written as UNAVAILABLE.
        Self(Key::generate(HMAC_SHA256, &SystemRandom::new()).ok())
    }

    /// Returns the reference of `value`, a caller's `kind` (`subject` or
    /// `client`), or [`UNAVAILABLE`] when the process holds no key.
    #[must_use]
    pub(crate) fn of(&self, kind: &str, value: &str) -> String {
        let Some(key) = &self.0 else {
            return UNAVAILABLE.to_owned();
        };
        let mut message =
            Vec::with_capacity(kind.len().saturating_add(value.len()).saturating_add(1));
        message.extend_from_slice(kind.as_bytes());
        message.push(0);
        message.extend_from_slice(value.as_bytes());
        let tag = hmac::sign(key, &message);
        let mut shown = String::with_capacity(SHOWN.saturating_mul(2));
        for byte in tag.as_ref().iter().take(SHOWN) {
            let _written: fmt::Result = write!(shown, "{byte:02x}");
        }
        shown
    }
}

impl References {
    /// Logs the security event of a caller the edge asserted, under the
    /// gateway's `request_id`: the edge's issuer, and the caller's subject
    /// and client by their references.
    pub(crate) fn edge_asserted(&self, caller: &Caller, request_id: Option<&str>) {
        tracing::info!(
            target: TARGET,
            event = "edge-identity-asserted",
            issuer = caller.issuer(),
            subject_ref = self.of("subject", caller.subject()),
            client_ref = self.of("client", caller.client_id()),
            request_id,
            "the edge asserted the caller's identity, and the gateway verified the assertion"
        );
    }
}

impl fmt::Debug for References {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("References")
            .field("keyed", &self.0.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::References;

    #[test]
    fn a_reference_is_stable_distinct_and_names_no_value() {
        let references = References::fresh();
        let one = references.of("subject", "Qz7-sub-81");
        assert_eq!(one, references.of("subject", "Qz7-sub-81"));
        assert_ne!(one, references.of("subject", "Qz7-sub-82"));
        assert_ne!(one, references.of("client", "Qz7-sub-81"));
        assert_ne!(one, References::fresh().of("subject", "Qz7-sub-81"));
        assert!(!one.contains("Qz7"), "{one}");
        assert_eq!(24, one.len(), "{one}");
    }
}
