// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why the gate refused a request, and the answer it sends: the error body
//! with its stable code, and the RFC 6750 §3 challenge.

use axum::response::{IntoResponse, Response};
use http::{HeaderValue, header};

use crate::error::{self, Code};

/// The `realm` of every `WWW-Authenticate` challenge (RFC 6750 §3).
pub(crate) const REALM: &str = "ferrofed";

/// Why a request was refused at the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The request carries no credential.
    Missing,
    /// The credential is not a token the gateway can read, or lacks a claim
    /// RFC 9068 §2.2 requires.
    Malformed,
    /// The token is signed with an algorithm not on [`ALGORITHMS`](crate::auth::ALGORITHMS).
    Algorithm,
    /// The token's `typ` is not `at+jwt` (RFC 9068 §4).
    Type,
    /// The token's issuer is not on the trust list.
    Issuer,
    /// The issuer's key set holds no key the token names.
    Key,
    /// The signature does not verify.
    Signature,
    /// The token has expired.
    Expired,
    /// The token is not valid yet.
    NotYetValid,
    /// The token does not name this gateway in `aud`.
    Audience,
    /// The introspection endpoint calls the token inactive.
    Inactive,
    /// The key set or the introspection endpoint cannot be had, so the
    /// token cannot be verified.
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
    pub const ALL: [Self; 22] = [
        Self::Missing,
        Self::Malformed,
        Self::Algorithm,
        Self::Type,
        Self::Issuer,
        Self::Key,
        Self::Signature,
        Self::Expired,
        Self::NotYetValid,
        Self::Audience,
        Self::Inactive,
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

    /// The reason the security log and the challenge name.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::Algorithm => "algorithm",
            Self::Type => "type",
            Self::Issuer => "issuer",
            Self::Key => "key",
            Self::Signature => "signature",
            Self::Expired => "expired",
            Self::NotYetValid => "not-yet-valid",
            Self::Audience => "audience",
            Self::Inactive => "inactive",
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

    /// What the challenge's `error_description` says.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Missing => "the request carries no access token",
            Self::Malformed => "the access token cannot be read as an RFC 9068 access token",
            Self::Algorithm => "the access token is signed with an algorithm the gateway refuses",
            Self::Type => "the access token is not typed at+jwt",
            Self::Issuer => "the access token's issuer is not trusted",
            Self::Key => "the access token names a key its issuer does not publish",
            Self::Signature => "the access token's signature does not verify",
            Self::Expired => "the access token has expired",
            Self::NotYetValid => "the access token is not valid yet",
            Self::Audience => "the access token is not issued for this gateway",
            Self::Inactive => "the access token is not active",
            Self::Unavailable => "the access token cannot be verified now",
            Self::Operation => "this gateway admits no caller to this operation",
            Self::Scope => "no scope of the access token grants this operation",
            Self::Demographic => "the client is not admitted to the DEMOGRAPHIC API",
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
            Self::Missing
            | Self::Malformed
            | Self::Algorithm
            | Self::Type
            | Self::Issuer
            | Self::Key
            | Self::Signature
            | Self::Expired
            | Self::NotYetValid
            | Self::Audience
            | Self::Inactive => Code::Unauthenticated,
        }
    }

    /// The answer to a refused request: the error body, with the RFC 6750 §3
    /// challenge on a `401` and a `403`.
    #[must_use]
    pub fn response(self, request_id: &str) -> Response {
        let mut response =
            error::response(self.code(), self.description(), request_id).into_response();
        let challenge = match self {
            Self::Missing => Some(format!("Bearer realm=\"{REALM}\"")),
            Self::Unavailable | Self::Operation | Self::Correlation => None,
            Self::Scope
            | Self::Demographic
            | Self::PurposeOfUse
            | Self::PatientContext
            | Self::PatientDemographic
            | Self::ContactPoint => Some(format!(
                "Bearer realm=\"{REALM}\", error=\"insufficient_scope\", error_description=\"{}\"",
                self.description()
            )),
            // NOTE: RFC 9470 §3: a token whose authentication does not meet the resource's
            // requirements is a 401 with the error `insufficient_user_authentication`.
            Self::NaturalPerson | Self::Assurance => Some(format!(
                "Bearer realm=\"{REALM}\", error=\"insufficient_user_authentication\", error_description=\"{}\"",
                self.description()
            )),
            _ => Some(format!(
                "Bearer realm=\"{REALM}\", error=\"invalid_token\", error_description=\"{}\"",
                self.description()
            )),
        };
        if let Some(value) = challenge.and_then(|text| HeaderValue::from_str(&text).ok()) {
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
            Refusal::Algorithm => 2,
            Refusal::Type => 3,
            Refusal::Issuer => 4,
            Refusal::Key => 5,
            Refusal::Signature => 6,
            Refusal::Expired => 7,
            Refusal::NotYetValid => 8,
            Refusal::Audience => 9,
            Refusal::Inactive => 10,
            Refusal::Unavailable => 11,
            Refusal::Operation => 12,
            Refusal::Scope => 13,
            Refusal::Demographic => 14,
            Refusal::PurposeOfUse => 15,
            Refusal::PatientContext => 16,
            Refusal::PatientDemographic => 17,
            Refusal::NaturalPerson => 18,
            Refusal::Assurance => 19,
            Refusal::ContactPoint => 20,
            Refusal::Correlation => 21,
        }
    }

    #[test]
    fn all_lists_every_refusal_once_in_declaration_order() {
        let ordinals: Vec<usize> = Refusal::ALL.into_iter().map(ordinal).collect();
        assert_eq!((0..Refusal::ALL.len()).collect::<Vec<_>>(), ordinals);
    }
}
