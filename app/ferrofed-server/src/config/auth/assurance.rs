// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth.issuer.assurance]`, as written and as resolved.
//!
//! The table says how an issuer's tokens state the assurance of their
//! authentication (Regulation (EU) 2025/327 Annex II 3.1), in the levels of
//! Regulation (EU) No 910/2014 Art 8(2).

use std::collections::BTreeMap;

use ferrofed_engine::conveyance::AssuranceLevel;
use serde::Deserialize;

use crate::config::auth::{AuthFault, fault};
use crate::config::error::Error;

/// `[auth.issuer.assurance]`: the claim that carries the authentication
/// assurance of a token, the values of that claim at each level, and the
/// least level a patient-data request needs.
///
/// The levels are those of Regulation (EU) No 910/2014 Art 8(2), which
/// Implementing Regulation (EU) 2026/2099 Art 6(3) cites. No specification
/// the gateway binds says which claim values stand for which level, so the
/// values are configured, with no default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssuranceClaims {
    /// The string claim that carries the assurance; `acr` by default, the
    /// claim RFC 9068 §2.2.1 lets an access token carry.
    pub claim: String,
    /// The least level a patient-data request needs.
    pub minimum: Option<AssuranceLevel>,
    /// The claim values that stand for level low.
    pub low: Vec<String>,
    /// The claim values that stand for level substantial.
    pub substantial: Vec<String>,
    /// The claim values that stand for level high.
    pub high: Vec<String>,
}

impl Default for AssuranceClaims {
    fn default() -> Self {
        Self {
            claim: String::from("acr"),
            minimum: None,
            low: Vec::new(),
            substantial: Vec::new(),
            high: Vec::new(),
        }
    }
}

/// An issuer's assurance mapping, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assurance {
    /// The string claim that carries the assurance.
    pub claim: String,
    /// The least level a patient-data request needs.
    pub minimum: AssuranceLevel,
    /// Each claim value the deployment declares, and its level.
    pub values: BTreeMap<String, AssuranceLevel>,
}

impl Assurance {
    /// The level `value` stands for, or `None` for a value the deployment
    /// declares at no level.
    #[must_use]
    pub fn level_of(&self, value: &str) -> Option<AssuranceLevel> {
        self.values.get(value).copied()
    }
}

impl AssuranceClaims {
    /// Resolves this table at `key`: a claim and a minimum, both set, each
    /// value declared once and not empty, and some value at the minimum or
    /// above it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Missing`] for a claim or a minimum not set, and
    /// [`Error::Auth`] with [`AuthFault::AssuranceValue`] or
    /// [`AuthFault::AssuranceUnreachable`].
    pub fn resolve(&self, key: &str) -> Result<Assurance, Error> {
        resolve(key, self)
    }
}

/// Resolves `written` at `key`, as [`AssuranceClaims::resolve`] says.
fn resolve(key: &str, written: &AssuranceClaims) -> Result<Assurance, Error> {
    if written.claim.is_empty() {
        return Err(Error::Missing {
            key: format!("{key}.claim"),
        });
    }
    let minimum = written.minimum.ok_or_else(|| Error::Missing {
        key: format!("{key}.minimum"),
    })?;
    let mut values = BTreeMap::new();
    for (level, listed) in [
        (AssuranceLevel::Low, &written.low),
        (AssuranceLevel::Substantial, &written.substantial),
        (AssuranceLevel::High, &written.high),
    ] {
        for value in listed {
            if value.is_empty() || values.insert(value.clone(), level).is_some() {
                return Err(fault(
                    &format!("{key}.{}", level.as_str()),
                    AuthFault::AssuranceValue,
                ));
            }
        }
    }
    if !values.values().any(|level| *level >= minimum) {
        return Err(fault(
            &format!("{key}.minimum"),
            AuthFault::AssuranceUnreachable,
        ));
    }
    Ok(Assurance {
        claim: written.claim.clone(),
        minimum,
        values,
    })
}
