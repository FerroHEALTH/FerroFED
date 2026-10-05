// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward grant kinds, each a credentials provider over the shared token
//! request, keys and sender constraints of [`onward`](crate::onward) (§13.1,
//! N25).
//!
//! [`client_credentials`] gives an endpoint one token for every caller (RFC
//! 6749 §4.4), [`exchange`] gives each verified caller a token of its own
//! (RFC 8693), and [`fapi2`] obtains a token from an authorization server
//! that follows the FAPI 2.0 Security Profile. A regional binding's grants
//! sit in a folder of their own behind its feature: `nl` holds the Nuts
//! grant of Annex B §B.4. No specification governs the grouping: our own
//! design.

pub mod client_credentials;
pub mod exchange;
pub mod fapi2;
#[cfg(feature = "nl")]
pub mod nl;
