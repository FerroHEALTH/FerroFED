// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How an adapter's configuration renders the `ferrohealth-smart` outbound
//! client it is built over: the service's URL with its userinfo and query
//! masked, and whether a credential rides with it, never the credential.
//! No specification governs the rendering: our own design.

use std::fmt;

use ferrohealth_secret::secret_url::RedactedUrl;
use ferrohealth_smart::client::outbound::OutboundClient;

/// `client` as a configuration's `Debug` shows it.
pub(crate) struct Shown<'a>(pub(crate) &'a OutboundClient);

impl fmt::Debug for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutboundClient")
            .field("url", &RedactedUrl(self.0.url().as_str()))
            .field("authorization", &self.0.authorization().is_some())
            .finish_non_exhaustive()
    }
}
