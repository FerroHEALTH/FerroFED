// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why the gate refused a request, and the answer it sends: the error body
//! with its stable code, and the RFC 6750 §3 challenge.
//!
//! A credential that is missing, malformed, invalid or cannot be checked is
//! answered as `ferrohealth-smart` answers it, with its status and its
//! challenge: one `401 invalid_token` for every token that does not hold,
//! naming no check, and a `403 insufficient_scope` naming no scope (RFC 6750
//! §3.1). Which check refused a token reaches the security log alone. The
//! refusals of the gateway's own policy (the purpose of use, the patient
//! context, the natural person, the assurance, the national contact point)
//! keep their own codes.

use axum::response::{IntoResponse, Response};
use ferrohealth_smart::refusal::{Denied, Reason, Refusal as SmartRefusal, Schemes, StepUp};
use http::{HeaderValue, StatusCode, header};

use crate::auth::contact_point::MissingAnnex;
use crate::error::{self, Code};

/// Why a request was refused at the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The request carries no credential of a scheme the gate admits.
    Missing,
    /// The request carries two credentials, or a credential field that is
    /// no credential at all (RFC 6750 §3.1, `invalid_request`).
    Malformed,
    /// The credential does not hold (RFC 6750 §3.1, `invalid_token`).
    Invalid,
    /// The issuer's metadata, key set or introspection endpoint cannot be
    /// had, so the token cannot be verified.
    Unavailable,
    /// No caller is admitted to the operation: an admin operation, or one
    /// the gateway does not know.
    Operation,
    /// No granted scope covers the operation.
    Scope,
    /// The client is not one the issuer's entry lists as a demographic
    /// client.
    Demographic,
    /// The token carries no purpose of use (§13.4).
    PurposeOfUse,
    /// Only a `patient/` grant covers the operation, and the token carries
    /// no `ehrId` that reads as an openEHR `HIER_OBJECT_ID` to confine it to.
    PatientContext,
    /// The token's grant is a patient grant, which reaches its patient's
    /// own EHR alone, and the request addresses the DEMOGRAPHIC API.
    PatientDemographic,
    /// The request reaches patient data, and the token names no natural
    /// person: its client acts, and the issuer's entry does not declare its
    /// client tokens as acting for the professional they name, or the token
    /// names none.
    NaturalPerson,
    /// The request reaches patient data, and the token states no
    /// authentication assurance at or above the least level its issuer's
    /// entry requires.
    Assurance,
    /// The request reaches patient data, and the token of a national contact
    /// point does not carry every attribute of Implementing Regulation (EU)
    /// 2026/2099 Annex Tables 1 and 2.
    ContactPoint,
    /// The national contact point's correlation header is repeated, empty,
    /// longer than [`MAX_CORRELATION`](crate::auth::contact_point::MAX_CORRELATION)
    /// bytes, or not visible ASCII.
    Correlation,
}

impl Refusal {
    /// Every refusal, in declaration order.
    pub const ALL: [Self; 14] = [
        Self::Missing,
        Self::Malformed,
        Self::Invalid,
        Self::Unavailable,
        Self::Operation,
        Self::Scope,
        Self::Demographic,
        Self::PurposeOfUse,
        Self::PatientContext,
        Self::PatientDemographic,
        Self::NaturalPerson,
        Self::Assurance,
        Self::ContactPoint,
        Self::Correlation,
    ];

    /// The reason the security log and the security metrics name.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::Invalid => "invalid",
            Self::Unavailable => "unavailable",
            Self::Operation => "operation",
            Self::Scope => "scope",
            Self::Demographic => "demographic-client",
            Self::PurposeOfUse => "purpose-of-use",
            Self::PatientContext => "patient-context",
            Self::PatientDemographic => "patient-demographic",
            Self::NaturalPerson => "natural-person",
            Self::Assurance => "assurance",
            Self::ContactPoint => "contact-point-attributes",
            Self::Correlation => "correlation",
        }
    }

    /// What the error body's `message` says.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Missing => "the request carries no credential",
            Self::Malformed => {
                "the request carries two credentials, or a credential field that is no credential"
            }
            Self::Invalid => "the credential is not valid",
            Self::Unavailable => "the access token cannot be verified now",
            Self::Operation => "this gateway admits no caller to this operation",
            Self::Scope | Self::Demographic => "the access token does not grant this operation",
            Self::PurposeOfUse => "the access token carries no purpose of use",
            Self::PatientContext => {
                "only a patient/ scope grants this operation, and the access token carries no ehrId to confine it to"
            }
            Self::PatientDemographic => {
                "a patient/ grant reaches its patient's own EHR alone, never the DEMOGRAPHIC API"
            }
            Self::NaturalPerson => {
                "the access token names no natural person, and patient data is released to a person or for a professional the token names"
            }
            Self::Assurance => {
                "the access token states no authentication assurance at the level patient data requires"
            }
            Self::ContactPoint => {
                "the national contact point's access token does not carry every attribute of Implementing Regulation (EU) 2026/2099 Annex Tables 1 and 2"
            }
            Self::Correlation => {
                "the correlation header is sent once, as 1 to 128 visible ASCII characters"
            }
        }
    }

    /// The error code the body carries.
    #[must_use]
    pub const fn code(self) -> Code {
        match self {
            Self::Missing | Self::Invalid => Code::Unauthenticated,
            Self::Malformed => Code::CredentialMalformed,
            Self::Unavailable => Code::AuthenticationUnavailable,
            Self::Operation => Code::OperationRefused,
            Self::Scope | Self::Demographic => Code::ScopeInsufficient,
            Self::PurposeOfUse => Code::PurposeOfUseRequired,
            Self::PatientContext => Code::PatientContextMissing,
            Self::PatientDemographic => Code::PatientConfinement,
            Self::NaturalPerson => Code::NaturalPersonRequired,
            Self::Assurance => Code::AuthenticationAssuranceInsufficient,
            Self::ContactPoint => Code::ContactPointAttributesRequired,
            Self::Correlation => Code::CorrelationInvalid,
        }
    }

    /// The refusal of `ferrohealth-smart` whose challenge this one carries,
    /// or `None` for one of the gateway's own.
    #[must_use]
    pub fn smart(self) -> Option<SmartRefusal> {
        match self {
            Self::Missing => Some(SmartRefusal::Missing(Schemes {
                bearer: true,
                basic: false,
            })),
            Self::Malformed => Some(SmartRefusal::InvalidRequest),
            Self::Invalid => Some(SmartRefusal::InvalidToken),
            Self::Unavailable => Some(SmartRefusal::Unavailable),
            Self::Operation => Some(SmartRefusal::Unclassified),
            Self::Scope
            | Self::Demographic
            | Self::PurposeOfUse
            | Self::PatientContext
            | Self::PatientDemographic
            | Self::ContactPoint => Some(SmartRefusal::Forbidden),
            // NOTE: RFC 9470 §3: a token whose authentication does not meet the resource's
            // requirements is a 401 with the error `insufficient_user_authentication`.
            Self::NaturalPerson | Self::Assurance => Some(StepUp::new().into_refusal()),
            Self::Correlation => None,
        }
    }

    /// The answer to a refused request: the error body, with the challenge
    /// naming `realm`.
    #[must_use]
    pub fn response(self, realm: &str, request_id: &str) -> Response {
        let mut response =
            error::response(self.code(), self.description(), request_id).into_response();
        let challenge = self
            .smart()
            .and_then(|refusal| refusal.challenge(Some(realm)));
        if let Some(value) = challenge.and_then(|text| HeaderValue::from_str(&text).ok()) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}

/// A refusal at the gate, with the check that refused a credential when one
/// did, which the security log names and the answer never carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    /// What the request is answered with.
    pub refusal: Refusal,
    /// The check that refused the credential, when one did.
    pub check: Option<Reason>,
    /// The Annex attributes a contact point's token lacks, which the answer
    /// names, empty for every other refusal.
    pub missing: MissingAnnex,
}

impl Refused {
    /// The refusal of a contact point's token that lacks the Annex
    /// attributes `missing`.
    #[must_use]
    pub fn missing(missing: MissingAnnex) -> Self {
        Self {
            refusal: Refusal::ContactPoint,
            check: None,
            missing,
        }
    }

    /// The answer to the refused request: the error body, naming the
    /// missing Annex attributes of a contact point's token, with the
    /// challenge naming `realm`.
    #[must_use]
    pub fn response(self, realm: &str, request_id: &str) -> Response {
        if self.refusal != Refusal::ContactPoint || self.missing.is_empty() {
            return self.refusal.response(realm, request_id);
        }
        let names: Vec<&str> = self.missing.names().collect();
        let message = format!(
            "{}; missing or not admitted: {}",
            self.refusal.description(),
            names.join(", ")
        );
        let mut response =
            error::response(self.refusal.code(), message, request_id).into_response();
        let challenge = self
            .refusal
            .smart()
            .and_then(|refusal| refusal.challenge(Some(realm)));
        if let Some(value) = challenge.and_then(|text| HeaderValue::from_str(&text).ok()) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}

impl From<Refusal> for Refused {
    fn from(refusal: Refusal) -> Self {
        Self {
            refusal,
            check: None,
            missing: MissingAnnex::default(),
        }
    }
}

impl From<&Denied> for Refused {
    fn from(denied: &Denied) -> Self {
        let refusal = match denied.refusal() {
            SmartRefusal::Missing(_) => Refusal::Missing,
            SmartRefusal::InvalidRequest => Refusal::Malformed,
            SmartRefusal::Unavailable => Refusal::Unavailable,
            SmartRefusal::Forbidden => Refusal::Scope,
            SmartRefusal::Unclassified => Refusal::Operation,
            _ => Refusal::Invalid,
        };
        Self {
            refusal,
            check: denied.reason(),
            missing: MissingAnnex::default(),
        }
    }
}

/// Why a Basic credential was not admitted where only named Basic users
/// are (RFC 7617).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BasicRefusal {
    /// The request carries no Basic credential.
    Missing,
    /// The Basic credential does not verify.
    Invalid,
    /// The Basic credential verifies, and its user is not one admitted here.
    Unlisted,
}

impl BasicRefusal {
    /// The reason the security log names.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Unlisted => "unlisted",
        }
    }

    /// The `WWW-Authenticate` challenge naming `realm` the refusal carries:
    /// one on a `401`, none on a `403` (RFC 7617 §2, §2.1).
    #[must_use]
    pub fn challenge(self, realm: &str) -> Option<HeaderValue> {
        let refusal = match self {
            Self::Missing => SmartRefusal::Missing(Schemes {
                bearer: false,
                basic: true,
            }),
            Self::Invalid => SmartRefusal::InvalidBasic,
            Self::Unlisted => return None,
        };
        refusal
            .challenge(Some(realm))
            .and_then(|text| HeaderValue::from_str(&text).ok())
    }

    /// The status of the refusal: `401` for a credential that is missing or
    /// does not verify, `403` for a user not admitted here.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::Missing | Self::Invalid => StatusCode::UNAUTHORIZED,
            Self::Unlisted => StatusCode::FORBIDDEN,
        }
    }

    /// The answer to the refused request: the error body, with the Basic
    /// challenge naming `realm` on a `401` (RFC 7617 §2, §2.1).
    #[must_use]
    pub fn response(self, realm: &str, request_id: &str) -> Response {
        let code = match self {
            Self::Missing | Self::Invalid => Code::Unauthenticated,
            Self::Unlisted => Code::OperationRefused,
        };
        let mut response = error::fixed(code, request_id);
        if let Some(value) = self.challenge(realm) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::Refusal;

    /// The declaration position of `refusal`; no wildcard, so a new variant
    /// fails to compile here until [`Refusal::ALL`] lists it.
    fn ordinal(refusal: Refusal) -> usize {
        match refusal {
            Refusal::Missing => 0,
            Refusal::Malformed => 1,
            Refusal::Invalid => 2,
            Refusal::Unavailable => 3,
            Refusal::Operation => 4,
            Refusal::Scope => 5,
            Refusal::Demographic => 6,
            Refusal::PurposeOfUse => 7,
            Refusal::PatientContext => 8,
            Refusal::PatientDemographic => 9,
            Refusal::NaturalPerson => 10,
            Refusal::Assurance => 11,
            Refusal::ContactPoint => 12,
            Refusal::Correlation => 13,
        }
    }

    #[test]
    fn all_lists_every_refusal_once_in_declaration_order() {
        let ordinals: Vec<usize> = Refusal::ALL.into_iter().map(ordinal).collect();
        assert_eq!((0..Refusal::ALL.len()).collect::<Vec<_>>(), ordinals);
    }

    // NOTE: RFC 6750 §3.1: an invalid token is `invalid_token` and a missing one carries no
    // error code; ferrohealth-smart's challenge names no check and no scope.
    #[test]
    fn the_challenges_name_no_check_and_no_scope() {
        for (refusal, expected) in [
            (Refusal::Missing, Some("Bearer realm=\"ferrofed\"")),
            (
                Refusal::Invalid,
                Some("Bearer realm=\"ferrofed\", error=\"invalid_token\""),
            ),
            (
                Refusal::Malformed,
                Some("Bearer realm=\"ferrofed\", error=\"invalid_request\""),
            ),
            (
                Refusal::Scope,
                Some("Bearer realm=\"ferrofed\", error=\"insufficient_scope\""),
            ),
            (Refusal::Unavailable, None),
            (Refusal::Operation, None),
            (Refusal::Correlation, None),
        ] {
            let response = refusal.response("ferrofed", "corr-1");
            let challenge = response
                .headers()
                .get(http::header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok());
            assert_eq!(expected, challenge, "{refusal:?}");
            assert_eq!(refusal.code().status(), response.status(), "{refusal:?}");
        }
    }
}
