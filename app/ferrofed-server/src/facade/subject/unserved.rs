// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a read of an EHR by subject is answered by the gateway, and the error
//! answer it gives (§11.2, §5.4.3).

use axum::response::Response;
use ferrofed_identity::role::patient::PatientRefError;
use ferrofed_registry::id::EndpointId;
use openehr_its::rest::runtime::ApiError;

use crate::error::{self, Code};
use crate::facade::owner::{self, Listed};
use crate::facade::security;

/// Why the read of an EHR by subject is answered by the gateway, with no
/// node's answer to pass on.
///
/// Every message names query parameters by position and members by endpoint
/// id, and never the subject (§5.4.3).
#[derive(Debug, thiserror::Error)]
pub(super) enum Unserved {
    /// The query string carries a parameter the operation does not declare.
    #[error(
        "query parameter {position} is not one the ITS-REST operation declares, so nothing is sent (§5.4.1, N33)"
    )]
    Undeclared {
        /// The parameter's position in the query string, counted from 1.
        position: usize,
    },
    /// The query string is no `ehr_get_by_subject` query: `subject_id` or
    /// `subject_namespace` is absent or repeated, or a pair does not
    /// percent-decode to UTF-8 text.
    #[error(
        "GET {{base}}/v1/ehr names its subject by subject_id and subject_namespace, each given once as UTF-8 text (ITS-REST 1.1.0)"
    )]
    Malformed(#[source] ApiError),
    /// The subject cannot form a patient reference.
    #[error("the subject cannot form a patient reference (§5.2)")]
    Patient(#[source] PatientRefError),
    /// The targeting headers name no one registry endpoint.
    #[error(transparent)]
    Target(#[from] owner::Untargeted),
    /// The targeting headers name a suspended endpoint.
    #[error(
        "the targeted endpoint is suspended, so the request can be routed to no destination (§11.1, §11.2)"
    )]
    Suspended,
    /// No candidate knows the subject.
    #[error(
        "no member holds an EHR for this subject, so the request can be routed to no destination (§11.2)"
    )]
    Nowhere,
    /// Several candidates know the subject.
    #[error(
        "the subject resolves at endpoints {}, and the gateway never chooses between them: name one in the openEHR-federation-endpoint header (§8.4, §12.5.2)",
        Listed(.0)
    )]
    Several(Vec<EndpointId>),
    /// The cross-reference could not answer for these candidates.
    #[error(
        "the cross-reference could not answer for endpoints {}, so whether the subject has an EHR there is unknown (§5.2, §11.2)",
        Listed(.0)
    )]
    Unresolved(Vec<EndpointId>),
    /// The consent pre-filter denied these candidates, and no other one
    /// holds the subject's EHR.
    #[error(
        "the consent pre-filter does not permit asking endpoints {} about this subject, and no other member holds an EHR for it (N27a, §13.2.1)",
        Listed(.0)
    )]
    ConsentDenied(Vec<EndpointId>),
    /// No candidate the gateway may ask holds an EHR for the subject, in a
    /// deployment that does not disclose consent exclusions: the one answer
    /// for a subject no member knows and for one only a denied member holds.
    #[error("{}", Code::SubjectUnavailable.message())]
    Unavailable,
    /// The localizer could not answer, the deployment fails closed, and so no
    /// member was asked (§14.1, N4).
    #[error(
        "the localizer could not answer, or its exchange could not be audited, so no member was asked and whether the subject has an EHR is unknown (§14.1, N4)"
    )]
    Unlocalized,
    /// The demographics step named no one master identity for the subject,
    /// or could not answer; `closed` when the outage of an undirected read
    /// fails closed as a localizer's does (Annex A §A.2, §14.1).
    #[error("{reason}, so whether the subject has an EHR is unknown")]
    Unidentified {
        /// Why, as every member would report it; it names no identifier.
        reason: String,
        /// Whether the read failed closed under the localization policy.
        closed: bool,
    },
    /// The request's deadline cannot be represented.
    #[error("the request's deadline cannot be represented")]
    Clock,
}

impl Unserved {
    /// This answer as a deployment gives it that discloses consent
    /// exclusions when `disclosed` is `true`, or does not when it is `false`.
    ///
    /// Not disclosing them, a subject no member knows and a subject only a
    /// denied member holds get one answer, `subject-unavailable`, so neither
    /// the status nor the body shows that a restriction exists (Regulation
    /// (EU) 2025/327 Art 8). It is a `404`, which HTTP defines for a resource
    /// a server did not find "or is not willing to disclose that one exists"
    /// (RFC 9110 §15.5.5), so it claims no absence the gateway does not know.
    pub(super) fn disclosed_as(self, disclosed: bool) -> Self {
        match self {
            Self::Nowhere | Self::ConsentDenied(_) if !disclosed => Self::Unavailable,
            other => other,
        }
    }

    /// The stable code the error body names.
    fn code(&self) -> Code {
        match self {
            Self::Undeclared { .. } => Code::QueryParameterRefused,
            Self::Malformed(_) | Self::Patient(_) => Code::PatientInvalid,
            Self::Target(untargeted) => untargeted.code(),
            Self::Suspended | Self::Nowhere => Code::NoDestination,
            Self::Several(_) => Code::SubjectSeveral,
            Self::Unresolved(_) | Self::Unidentified { closed: false, .. } => {
                Code::ResolutionUnavailable
            }
            Self::ConsentDenied(_) => Code::ConsentDenied,
            Self::Unavailable => Code::SubjectUnavailable,
            Self::Unlocalized | Self::Unidentified { closed: true, .. } => {
                Code::LocalizationUnavailable
            }
            Self::Clock => Code::Internal,
        }
    }

    /// The error answer, naming the client's `request_id`; a refused query
    /// parameter is a security event and a failed resolution is logged, both
    /// under the gateway's `logged` id.
    pub(super) fn respond(self, request_id: &str, logged: &str) -> Response {
        let code = self.code();
        match &self {
            Self::Undeclared { position } => security::query_parameter_refused(*position, logged),
            Self::Unresolved(_) | Self::Unlocalized | Self::Unidentified { .. } | Self::Clock => {
                tracing::error!(
                    code = code.as_str(),
                    error = %self,
                    request_id = logged,
                    "the read of an EHR by subject was not served"
                );
            }
            _ => {}
        }
        if code == Code::Internal {
            return error::fixed(code, request_id);
        }
        error::response(code, crate::chain(&self), request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, Unserved};
    use ferrofed_identity::role::patient::PatientRefError;
    use ferrofed_registry::id::EndpointId;
    use http::StatusCode;
    use openehr_its::rest::runtime::ApiError;

    fn endpoint(id: &str) -> EndpointId {
        EndpointId::new(id).unwrap()
    }

    #[test]
    fn each_refusal_answers_its_code_and_status() {
        let table = [
            (
                Unserved::Undeclared { position: 2 },
                "query-parameter-refused",
                StatusCode::BAD_REQUEST,
            ),
            (
                Unserved::Malformed(ApiError::BadRequest(
                    "the query parameter `subject_id` is given more than once".to_owned(),
                )),
                "patient-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (
                Unserved::Patient(PatientRefError::EmptyValue),
                "patient-invalid",
                StatusCode::BAD_REQUEST,
            ),
            (Unserved::Suspended, "no-destination", StatusCode::NOT_FOUND),
            (Unserved::Nowhere, "no-destination", StatusCode::NOT_FOUND),
            (
                Unserved::Several(vec![endpoint("a"), endpoint("b")]),
                "subject-several",
                StatusCode::CONFLICT,
            ),
            (
                Unserved::Unresolved(vec![endpoint("a")]),
                "resolution-unavailable",
                StatusCode::FAILED_DEPENDENCY,
            ),
            (
                Unserved::ConsentDenied(vec![endpoint("a")]),
                "consent-denied",
                StatusCode::FORBIDDEN,
            ),
            (
                Unserved::Unavailable,
                "subject-unavailable",
                StatusCode::NOT_FOUND,
            ),
            (
                Unserved::Unlocalized,
                "localization-unavailable",
                StatusCode::FAILED_DEPENDENCY,
            ),
            (
                Unserved::Unidentified {
                    reason: String::from("the patient could not be identified"),
                    closed: false,
                },
                "resolution-unavailable",
                StatusCode::FAILED_DEPENDENCY,
            ),
            (
                Unserved::Unidentified {
                    reason: String::from("the demographics service could not answer"),
                    closed: true,
                },
                "localization-unavailable",
                StatusCode::FAILED_DEPENDENCY,
            ),
            (
                Unserved::Clock,
                "internal",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (unserved, code, status) in table {
            let answered: Code = unserved.code();
            assert_eq!(code, answered.as_str(), "{unserved:?}");
            assert_eq!(status, answered.status(), "{unserved:?}");
        }
    }

    #[test]
    fn two_holders_list_their_endpoints_by_id() {
        let message =
            Unserved::Several(vec![endpoint("node-a-pub"), endpoint("node-b-pub")]).to_string();
        assert!(message.contains("[node-a-pub, node-b-pub]"), "{message}");
    }

    #[test]
    fn withheld_consent_folds_nowhere_and_a_denial_into_one_answer() {
        for unserved in [
            Unserved::Nowhere,
            Unserved::ConsentDenied(vec![endpoint("node-b-pub")]),
        ] {
            assert!(matches!(
                unserved.disclosed_as(false),
                Unserved::Unavailable
            ));
        }
        let message = Unserved::Unavailable.to_string();
        assert!(
            !message.to_ascii_lowercase().contains("consent") && !message.contains("node-b"),
            "{message}"
        );
    }

    #[test]
    fn disclosed_consent_changes_no_answer() {
        assert!(matches!(
            Unserved::Nowhere.disclosed_as(true),
            Unserved::Nowhere
        ));
        assert!(matches!(
            Unserved::ConsentDenied(vec![endpoint("node-b-pub")]).disclosed_as(true),
            Unserved::ConsentDenied(_)
        ));
        assert!(matches!(
            Unserved::Unresolved(vec![endpoint("node-b-pub")]).disclosed_as(false),
            Unserved::Unresolved(_)
        ));
    }
}
