// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whom an identity exchange is made for: the caller the gateway verified,
//! or the gateway itself.
//!
//! Each role is asked on behalf of someone ([`OnBehalfOf`]), and a binding
//! that audits its exchanges names that party in the audit record: the IHE
//! audit records name the user from the OAuth token (PIXm §2:3.83.5.2.1, BALP
//! 1.1.4 §3:5.7.5.4, ITI TF-2 §3.55.5.1.1). The caller's identity goes to an
//! audit record alone: `Debug` shows none of it, and no node, log line or
//! metric is given it here.

use std::fmt;

/// Whom an identity exchange is made for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnBehalfOf {
    /// A caller the gateway verified, who asked for the request the exchange
    /// serves.
    Caller(Caller),
    /// The gateway itself, with no caller: an admission check, a directory
    /// refresh or a subscription the gateway keeps.
    Gateway,
}

impl OnBehalfOf {
    /// Returns the caller, when the exchange is made for one.
    #[must_use]
    pub fn caller(&self) -> Option<&Caller> {
        match self {
            Self::Caller(caller) => Some(caller),
            Self::Gateway => None,
        }
    }
}

/// A verified caller as an audit record names them: the token's issuer,
/// subject and client, the audience the gateway is known by, and every
/// purpose of use the token declares.
///
/// `Debug` shows that a caller is named and none of the values.
#[derive(Clone, PartialEq, Eq)]
pub struct Caller {
    issuer: String,
    subject: String,
    client_id: String,
    audience: Option<String>,
    purposes: Vec<Purpose>,
}

impl Caller {
    /// Returns the caller `subject` (`sub`) the issuer `issuer` (`iss`)
    /// vouched for, who asked through the client `client_id` (`client_id`).
    #[must_use]
    pub fn new(issuer: String, subject: String, client_id: String) -> Self {
        Self {
            issuer,
            subject,
            client_id,
            audience: None,
            purposes: Vec::new(),
        }
    }

    /// Returns this caller, their token naming the gateway as `audience`.
    #[must_use]
    pub fn with_audience(mut self, audience: Option<String>) -> Self {
        self.audience = audience;
        self
    }

    /// Returns this caller, asking for `purposes`.
    #[must_use]
    pub fn with_purposes(mut self, purposes: Vec<Purpose>) -> Self {
        self.purposes = purposes;
        self
    }

    /// Returns the issuer that vouched for the caller.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the caller's subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the client the caller used.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns the audience the caller's token names the gateway by, when
    /// known.
    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }

    /// Returns every purpose of use the caller's token declares.
    #[must_use]
    pub fn purposes(&self) -> &[Purpose] {
        &self.purposes
    }
}

impl fmt::Debug for Caller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Caller(<redacted>)")
    }
}

/// One purpose of use: a code and the system that defines it (§13.4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Purpose {
    /// The code system, when the token names one.
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

#[cfg(test)]
mod tests {
    use super::{Caller, OnBehalfOf, Purpose};

    #[test]
    fn debug_shows_that_a_caller_is_named_and_none_of_the_values() {
        let on_behalf = OnBehalfOf::Caller(
            Caller::new(
                "https://issuer.example.test".to_owned(),
                "Qz7-caller-61".to_owned(),
                "Qz7-app-62".to_owned(),
            )
            .with_audience(Some("urn:example:gateway".to_owned()))
            .with_purposes(vec![Purpose {
                system: None,
                code: "TREAT".to_owned(),
            }]),
        );
        let shown = format!("{on_behalf:?}");
        for value in [
            "issuer.example.test",
            "Qz7-caller-61",
            "Qz7-app-62",
            "urn:example:gateway",
        ] {
            assert!(!shown.contains(value), "{value} in {shown}");
        }
        assert!(shown.contains("Caller"), "{shown}");
    }
}
