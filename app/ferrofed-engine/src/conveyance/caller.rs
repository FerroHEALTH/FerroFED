// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The caller as the gateway verified it, and on whose behalf a request
//! reaches a node.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::conveyance::relayed::{Relayed, Role};

/// How the gateway verified a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verification {
    /// A signed access token, against its issuer's key set (RFC 9068).
    Signature,
    /// An access token its issuer's introspection endpoint called active
    /// (RFC 7662).
    Introspection,
    /// The edge's signed assertion: the edge authenticated the caller, and
    /// the gateway verified what the edge asserted.
    Edge,
}

impl Verification {
    /// The `verified_by` claim's value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signature => "signature",
            Self::Introspection => "introspection",
            Self::Edge => "edge",
        }
    }
}

/// One purpose of use: a code and the system that defines it, the HL7 v3
/// `PurposeOfUse` coding IHE IUA conveys (§13.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Purpose {
    /// The code system, when the caller's token names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

/// The caller as the gateway verified it, the facts a node is told.
///
/// It holds no patient identifier and no credential. `Debug` shows how the
/// caller was verified and no value.
#[derive(Clone, PartialEq, Eq)]
pub struct Caller {
    /// The issuer that vouched for the caller.
    pub issuer: String,
    /// The caller's subject.
    pub subject: String,
    /// The caller's organisation, when its token names one.
    pub organisation: Option<String>,
    /// The caller's purposes of use.
    pub purposes: Vec<Purpose>,
    /// The caller's scopes as granted, space-separated; for a caller whose
    /// grant is confined to one patient, the granted `patient/` scopes that
    /// cover the operation.
    pub scope: String,
    /// How the gateway verified the caller.
    pub verified_by: Verification,
    /// The professional's identification the caller's token states, boxed
    /// so a [`Principal`] stays small.
    pub professional: Box<Professional>,
    /// Who acts behind the caller's token.
    pub acting: Acting,
    /// The assurance level of the caller's authentication, when its issuer
    /// declares how its tokens state one.
    pub assurance_level: Option<AssuranceLevel>,
    /// The professional and the provider a national contact point relays,
    /// as it asserts them, when the caller is one.
    pub relayed: Option<Box<Relayed>>,
}

/// The professional's identification a caller's token states, as IHE IUA
/// names it (ITI TF-2 3.71.4.2.2.1.1), with the agency that issued the
/// identifier.
///
/// They are the professional's attributes of Implementing Regulation (EU)
/// 2026/2099 Annex Table 1 a token of the caller's own issuer can state, so
/// a node can identify the professional of every registration or update
/// (Regulation (EU) 2025/327 Art 13(4)).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Professional {
    /// `subject_name`, the professional's name.
    pub name: Option<String>,
    /// `national_provider_identifier`, the identifier the professional's
    /// national authority issued.
    pub identifier: Option<String>,
    /// The name of the agency that issued `identifier`: the Annex Table 1
    /// `issuing_authority_name`, which IUA defines no claim for.
    pub issuing_authority: Option<String>,
    /// `subject_role`, the professional's roles.
    pub roles: Vec<Role>,
}

impl fmt::Debug for Professional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Professional")
            .field("name", &self.name.is_some())
            .field("identifier", &self.identifier.is_some())
            .field("issuing_authority", &self.issuing_authority.is_some())
            .field("roles", &self.roles.len())
            .finish()
    }
}

/// Who acts behind a caller's token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Acting {
    /// The natural person the caller's `sub` names.
    #[default]
    Person,
    /// A client application: the caller's `sub` is its `client_id` (IHE IUA
    /// ITI TF-2 3.71.4.2.2.1), or only `system/` scopes cover the operation
    /// (SMART on openEHR master08 §Resource Scopes).
    Client,
}

impl Acting {
    /// The `acting` claim's value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Client => "client",
        }
    }
}

/// An assurance level of an electronic identification means, as Regulation
/// (EU) No 910/2014 Art 8(2) names them, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssuranceLevel {
    /// Level low, Art 8(2)(a).
    Low,
    /// Level substantial, Art 8(2)(b).
    Substantial,
    /// Level high, Art 8(2)(c).
    High,
}

impl AssuranceLevel {
    /// The level's name: the `assurance_level` claim's value, and the name
    /// a configuration writes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Substantial => "substantial",
            Self::High => "high",
        }
    }
}

impl fmt::Debug for Caller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Caller")
            .field("verified_by", &self.verified_by)
            .finish_non_exhaustive()
    }
}

/// On whose behalf a request reaches a node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Principal {
    /// A caller the gateway verified.
    Caller(Caller),
    /// The gateway itself, for its operator, with no client caller: the
    /// admission check and the operator's stored-query distribution.
    Gateway,
}
