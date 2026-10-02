// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The correlation id the gateway sends to a node, and the inventory of every
//! header a node request carries.
//!
//! A node is located by its `ehr_id` alone, and no directly identifying
//! identifier travels in a header the gateway composes (§5.4.1, N33). The
//! gateway cannot tell whether a client's free text names a patient, so no
//! header value toward a node comes from client input. The correlation id is
//! an [`OutboundId`], which only [`OutboundId::mint`] can make: a version 4
//! UUID with no client input, one per client request and the same for every
//! node that request reaches (CP-26).
//!
//! Every header of `POST {base}/v1/query/aql` to a node, and where its value
//! comes from:
//!
//! | Header | Value | Source |
//! |---|---|---|
//! | `Accept` | `application/json` | `openehr-its`'s client runtime, its default when the call sets none |
//! | `Content-Type` | `application/json` | `openehr-its`'s client runtime, for the JSON body |
//! | `Authorization` | `Basic` or `Bearer` | the endpoint's onward credential from the gateway's configuration, only when one is configured |
//! | `X-Request-Id` | a version 4 UUID | [`OutboundId::mint`], only when the caller passes one |
//! | `Host`, `Content-Length` | the endpoint's authority, the body length | the HTTP engine, from the registry URL and the composed body |
//! | `Accept-Encoding` | the codings the engine decodes | the HTTP engine, from its compression features |
//!
//! No other header is set, and none is copied from the client request. No
//! specification governs the correlation header itself: our own design, under
//! the name every proxy already uses.

use std::fmt;

use uuid::Uuid;

/// The correlation id one client request carries to every node it reaches.
///
/// It is minted by the gateway and never parsed or built from text, so no
/// client value can become one. `Display` writes the hyphenated UUID, which
/// is always a legal header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundId(Uuid);

impl OutboundId {
    /// Mints a fresh id: a random version 4 UUID.
    #[must_use]
    pub fn mint() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for OutboundId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.hyphenated(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::OutboundId;
    use http::HeaderValue;

    #[test]
    fn a_minted_id_is_a_fresh_hyphenated_v4_uuid_and_a_legal_header_value() {
        let first = OutboundId::mint();
        let second = OutboundId::mint();
        assert_ne!(first, second, "every request gets its own id");
        let text = first.to_string();
        let parsed = uuid::Uuid::parse_str(&text).expect("a minted id is a UUID");
        assert_eq!(Some(uuid::Version::Random), parsed.get_version());
        assert_eq!(36, text.len(), "the hyphenated form: {text}");
        assert!(HeaderValue::from_str(&text).is_ok());
    }
}
