// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU Requirements Catalogue 10.0.0 §1.25 code a refusal to a
//! national contact point carries, so its national connector maps the
//! refusal to the code NCPeH-A answers its NCP with.
//!
//! A refused token takes the code `ferrohealth-smart`'s refusal carries
//! ([`NcpError::shared`]); the gateway holds no table of its own for it.
//! The gateway's own refusals take theirs from [`NcpError::of`]. No legacy
//! numeric code of the `NCPeH` Components Specification 10.0.0 appendix 6.4
//! is ever issued: "New implementations of MyHealth@EU NCP-A MUST NOT issue
//! these errors". Which §1.25 code each of the gateway's own refusals takes
//! is our own design over the catalogue's rows, each named by its
//! requirement.

use axum::response::Response;
use ferrohealth_smart::federation::contact::EhdsiCode;

use crate::error::{Answered, Code};

/// The member of a problem body, and the text of an access record's outcome
/// description, that carries the §1.25 code name.
pub const MEMBER: &str = "ncp_error";

/// A Requirements Catalogue 10.0.0 §1.25 code name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NcpError(&'static str);

impl NcpError {
    /// `ERROR_REGISTRY_NOT_AVAILABLE`: a national infrastructure that cannot
    /// answer, here a member node or a service it needs.
    pub const REGISTRY_NOT_AVAILABLE: Self = Self("ERROR_REGISTRY_NOT_AVAILABLE");

    /// `ERROR_ED_DISCARD_FAILED`: a discard of a dispensation that is not
    /// admitted (RC 10.0.0 §1.25 requirement 07.04).
    pub const ED_DISCARD_FAILED: Self = Self("ERROR_ED_DISCARD_FAILED");

    /// The code `ferrohealth-smart` gives a refused contact-point token.
    #[must_use]
    pub const fn shared(code: EhdsiCode) -> Self {
        Self(code.as_str())
    }

    /// The code of the gateway's own refusal `code` to a contact point, or
    /// `None` for a refusal of a token or of the professional's attributes,
    /// whose code is `ferrohealth-smart`'s, and for one §1.25 gives no row.
    #[must_use]
    pub const fn of(code: Code) -> Option<Self> {
        match code {
            // NOTE: RC 10.0.0 §1.25 requirement 11: a scope, operation or rights refusal, and
            // the emergency limit, take the generic security code (eHDSI XCA 8.0.1 §2.2.3).
            Code::ScopeInsufficient
            | Code::OperationRefused
            | Code::PatientConfinement
            | Code::PatientContextMissing
            | Code::CorrelationInvalid
            | Code::EmergencyLimitReached
            | Code::TrcUnbound => Some(Self("ERROR_SEC_GENERIC")),
            // NOTE: RC 10.0.0 §1.25 requirement 11.01.02: a contact point's token without the
            // TRC patient misses an assertion attribute, the code ferrohealth-smart gives the rest.
            Code::TrcMissing => Some(Self::shared(EhdsiCode::SecDataIntegrityNotEnsured)),
            // NOTE: RC 10.0.0 §1.25 requirements 05.01, 06.01 and 07.01: a national
            // infrastructure that cannot answer, here a member node or a service it needs.
            Code::NodeTimeout
            | Code::NodeUnreachable
            | Code::NodeRefused
            | Code::NodeError
            | Code::ControllingSystemUnreachable
            | Code::ResolutionUnavailable
            | Code::LocalizationUnavailable
            | Code::PatientContextUnavailable => Some(Self::REGISTRY_NOT_AVAILABLE),
            // NOTE: RC 10.0.0 §1.25 requirements 05.01, 06.01 and 07.01: the patient
            // has not given consent to the service.
            Code::ConsentDenied => Some(Self("ERROR_NO_CONSENT")),
            // NOTE: RC 10.0.0 §1.25 requirements 05.01, 06.01 and 07.01: an unexpected
            // error, here an internal defect or the gateway's own record that failed.
            Code::Internal | Code::AccessUnrecorded | Code::EmergencyUncounted => {
                Some(Self("ERROR_INTERNAL_ERROR"))
            }
            _ => None,
        }
    }

    /// Returns the code name as §1.25 spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Returns `response`, an answer to a national contact point, carrying the
/// §1.25 code of the gateway's own refusal it answers.
///
/// An error answer the gateway built ([`Answered`]) that carries no code yet is answered again
/// with the code in its [`MEMBER`] member and its extensions, its status,
/// headers and extensions kept; any other answer passes as it is.
#[must_use]
pub fn stamped(response: Response) -> Response {
    if response.extensions().get::<NcpError>().is_some() {
        return response;
    }
    let Some(answered) = response.extensions().get::<Answered>().cloned() else {
        return response;
    };
    let Some(ncp) = NcpError::of(answered.code) else {
        return response;
    };
    let (parts, _body) = response.into_parts();
    let rebuilt =
        crate::error::to_contact_point(answered.code, answered.message, &answered.request_id, ncp);
    let (fresh, body) = rebuilt.into_parts();
    let mut kept = parts;
    kept.headers.remove(http::header::CONTENT_LENGTH);
    for (name, value) in &fresh.headers {
        kept.headers.insert(name.clone(), value.clone());
    }
    kept.extensions.insert(ncp);
    Response::from_parts(kept, body)
}

/// Returns the §1.25 code `response`, an answer to a national contact
/// point, carries or the gateway's own refusal it answers takes.
#[must_use]
pub fn of_answer(response: &Response) -> Option<NcpError> {
    response
        .extensions()
        .get::<NcpError>()
        .copied()
        .or_else(|| {
            response
                .extensions()
                .get::<Answered>()
                .and_then(|answered| NcpError::of(answered.code))
        })
}

/// Returns `described`, an access record's outcome description, led by
/// `ncp`'s code name.
#[must_use]
pub fn described(ncp: NcpError, described: Option<String>) -> String {
    match described {
        Some(text) => format!("{}: {text}", ncp.as_str()),
        None => ncp.as_str().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, NcpError};

    #[test]
    fn a_token_or_professional_refusal_has_no_code_of_the_gateways_own() {
        for code in [
            Code::Unauthenticated,
            Code::CredentialMalformed,
            Code::AuthenticationUnavailable,
            Code::NaturalPersonRequired,
            Code::AuthenticationAssuranceInsufficient,
            Code::PurposeOfUseRequired,
            Code::ContactPointAttributesRequired,
        ] {
            assert_eq!(None, NcpError::of(code), "{code:?}");
        }
    }

    #[test]
    fn no_code_is_a_legacy_number() {
        for code in Code::every() {
            if let Some(ncp) = NcpError::of(code) {
                assert!(ncp.as_str().starts_with("ERROR_"), "{code:?}");
                assert!(
                    !ncp.as_str().bytes().all(|byte| byte.is_ascii_digit()),
                    "{code:?}"
                );
            }
        }
    }
}
