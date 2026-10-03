// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The verified caller: who a request comes from, as the gateway verified it
//! (§13.1, N25, §13.4).
//!
//! A request the gateway admits carries one [`Caller`] in its extensions. It
//! holds what the verified token or edge assertion says about the caller and
//! never anything about a patient: an IHE IUA `person_id` claim is never read
//! (§5.4.1, N33).

use openehr_sdt::smart_scopes::SmartScope;

/// A caller the gateway verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The issuer that vouched for the caller (`iss`).
    issuer: String,
    /// The caller (`sub`).
    subject: String,
    /// The client the caller used (`client_id`).
    client_id: String,
    /// The caller's organisation: the IHE IUA `subject_organization_id`,
    /// when the token carries one.
    organisation: Option<String>,
    /// The `scope` claim as granted, space-separated.
    granted: String,
    /// The granted scopes, read in the SMART on openEHR grammar.
    scopes: Vec<SmartScope>,
    /// Every purpose of use the token declares (§13.4).
    purposes: Vec<PurposeOfUse>,
    /// How the gateway verified the caller.
    verified_by: VerifiedBy,
}

/// The facts a verified credential states about its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stated {
    /// `iss`.
    pub issuer: String,
    /// `sub`.
    pub subject: String,
    /// `client_id`.
    pub client_id: String,
    /// The IHE IUA `subject_organization_id`.
    pub organisation: Option<String>,
    /// The `scope` claim, or empty.
    pub granted: String,
    /// The purposes of use.
    pub purposes: Vec<PurposeOfUse>,
}

impl Caller {
    /// Returns the caller `stated` describes, verified as `verified_by`
    /// says, its scopes read with
    /// [`SmartScope::parse_all`](openehr_sdt::smart_scopes::SmartScope::parse_all).
    #[must_use]
    pub fn new(stated: Stated, verified_by: VerifiedBy) -> Self {
        let scopes = SmartScope::parse_all(&stated.granted);
        Self {
            issuer: stated.issuer,
            subject: stated.subject,
            client_id: stated.client_id,
            organisation: stated.organisation,
            granted: stated.granted,
            scopes,
            purposes: stated.purposes,
            verified_by,
        }
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

    /// Returns the caller's organisation, when the token names one.
    #[must_use]
    pub fn organisation(&self) -> Option<&str> {
        self.organisation.as_deref()
    }

    /// Returns the `scope` claim as granted.
    #[must_use]
    pub fn granted(&self) -> &str {
        &self.granted
    }

    /// Returns the granted scopes.
    #[must_use]
    pub fn scopes(&self) -> &[SmartScope] {
        &self.scopes
    }

    /// Returns every purpose of use the token declares.
    #[must_use]
    pub fn purposes(&self) -> &[PurposeOfUse] {
        &self.purposes
    }

    /// Returns how the gateway verified the caller.
    #[must_use]
    pub const fn verified_by(&self) -> VerifiedBy {
        self.verified_by
    }
}

/// How the gateway verified a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifiedBy {
    /// A signed access token, against its issuer's key set (RFC 9068).
    Signature,
    /// An access token its issuer's introspection endpoint called active
    /// (RFC 7662).
    Introspection,
    /// The edge's signed assertion: the edge authenticated the caller, and
    /// the gateway verified what the edge asserted.
    Edge,
}

impl VerifiedBy {
    /// The name the security log records.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Signature => "signature",
            Self::Introspection => "introspection",
            Self::Edge => "edge",
        }
    }
}

/// One purpose of use: a code and the system that defines it, the HL7 v3
/// `PurposeOfUse` coding of IHE IUA (§13.4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PurposeOfUse {
    /// The code system, when the token names one.
    pub system: Option<String>,
    /// The code.
    pub code: String,
}
