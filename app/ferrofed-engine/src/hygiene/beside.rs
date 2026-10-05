// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Text the gateway sends beside a node request rather than in it, read for
//! a withheld identifier: the caller's token a token exchange sends to the
//! node's authorization server (§5.4.1, N33; RFC 8693 §2.1).

use secrecy::ExposeSecret;

use super::{Withheld, decode};

impl Withheld {
    /// Whether `text`, raw or percent-decoded, carries a withheld
    /// identifier.
    #[must_use]
    pub fn carried_by(&self, text: &str) -> bool {
        self.0.iter().any(|value| {
            let value = value.expose_secret();
            text.contains(value) || decode::percent_decoded(text).contains(value)
        })
    }
}
