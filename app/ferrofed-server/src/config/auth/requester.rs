// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth.issuer.requester]`, the claims that name who asks for the data.

use serde::Deserialize;

use crate::config::error::Error;

/// `[auth.issuer.requester]`: the names of the token claims that carry the
/// professional's UZI number and role and the organisation's URA and type,
/// each a string claim.
///
/// No specification the gateway binds names these claims, so each is
/// configured, with no default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RequesterClaims {
    /// The claim carrying the professional's UZI number.
    pub professional: String,
    /// The claim carrying the professional's UZI role code.
    pub role: String,
    /// The claim carrying the organisation's URA.
    pub organisation: String,
    /// The claim carrying the organisation's care provider type.
    pub organisation_type: String,
}

impl RequesterClaims {
    /// Checks that this table at `key` names all four claims.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Missing`] for a claim name not set.
    pub(super) fn check(&self, key: &str) -> Result<(), Error> {
        for (name, claim) in [
            ("professional", &self.professional),
            ("role", &self.role),
            ("organisation", &self.organisation),
            ("organisation_type", &self.organisation_type),
        ] {
            if claim.is_empty() {
                return Err(Error::Missing {
                    key: format!("{key}.{name}"),
                });
            }
        }
        Ok(())
    }
}
