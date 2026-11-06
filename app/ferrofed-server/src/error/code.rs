// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each [`Code`] says: its text on the wire and its HTTP status
//! (§11.2). Its fixed message is [`super::message`]'s.

use http::StatusCode;
use openehr_federation::aql::refusal::Refusal;

use super::{Code, RefusalCode};

impl Code {
    /// Every code that is not a refusal, in declaration order.
    pub const GATEWAY: [Self; 78] = [
        Self::BodyInvalid,
        Self::CompletenessInvalid,
        Self::PartialUnsupported,
        Self::DedupInvalid,
        Self::ParameterInvalid,
        Self::PatientInvalid,
        Self::NoDestination,
        Self::EhrIdCollision,
        Self::ControllingSystemUnreachable,
        Self::Internal,
        Self::NotFound,
        Self::NotImplemented,
        Self::EndpointUnknown,
        Self::OrganisationUnknown,
        Self::TargetRequired,
        Self::EndpointSeveral,
        Self::QueryParameterRefused,
        Self::NodeTimeout,
        Self::NodeUnreachable,
        Self::NodeRefused,
        Self::TargetingConflict,
        Self::EhrIdInvalid,
        Self::NodeError,
        Self::ProbeRequiresUuid,
        Self::QueryNameInvalid,
        Self::QueryVersionInvalid,
        Self::QueryTypeUnsupported,
        Self::SubjectLiteral,
        Self::StoredQueryHeld,
        Self::StoredQueryUnknown,
        Self::PrecedingVersionInvalid,
        Self::ParameterValueInvalid,
        Self::MediaTypeNotAcceptable,
        Self::MediaTypeUnsupported,
        Self::SubjectSeveral,
        Self::ResolutionUnavailable,
        Self::EhrIdHeld,
        Self::DefinitionEndpointTargeted,
        Self::StoredQueryFanOutUnsupported,
        Self::StoredQueryReadOnly,
        Self::StoredQueryReserved,
        Self::ConsentDenied,
        Self::Unauthenticated,
        Self::ScopeInsufficient,
        Self::PurposeOfUseRequired,
        Self::AuthenticationUnavailable,
        Self::OperationRefused,
        Self::LocalizationUnavailable,
        Self::PatientContextMissing,
        Self::PatientConfinement,
        Self::PatientContextUnavailable,
        Self::SubjectUnavailable,
        Self::Overloaded,
        Self::RateLimited,
        Self::AccessUnrecorded,
        Self::NaturalPersonRequired,
        Self::AuthenticationAssuranceInsufficient,
        Self::ContactPointAttributesRequired,
        Self::CorrelationInvalid,
        Self::ConfigurationInvalid,
        Self::RevisionStale,
        Self::RevisionUnknown,
        Self::RollbackUnavailable,
        Self::ResourceFileOwned,
        Self::ResourceUnknown,
        Self::ConfigurationUnmanaged,
        Self::ApprovalRequired,
        Self::ApprovalOwn,
        Self::ApprovalNotRequired,
        Self::OperatorUnverified,
        Self::ConfirmationUnavailable,
        Self::ConfirmationPending,
        Self::SecretStoreUnavailable,
        Self::CredentialMalformed,
        Self::EmergencyLimitReached,
        Self::EmergencyUncounted,
        Self::TrcMissing,
        Self::TrcUnbound,
    ];

    /// Every code: [`Code::GATEWAY`], then one per [`Refusal::KINDS`].
    pub fn every() -> impl Iterator<Item = Self> {
        Self::GATEWAY.into_iter().chain(
            Refusal::KINDS
                .iter()
                .map(|kind| Self::Refused(RefusalCode(kind))),
        )
    }

    /// The codes of the gateway's own conditions it once answered and
    /// answers no more.
    ///
    /// A client may still match on one, so a retired code is kept here and
    /// never given to another condition. `query-version-required`: a
    /// definition `PUT` with no version is stored at a version the registry
    /// assigns (§12.7 stored-query-versioning; ITS-REST Definition API,
    /// `definition_query_store`).
    pub const RETIRED: &[&str] = &["query-version-required"];

    /// Every code the gateway once answered and answers no more
    /// ([`Code::RETIRED`], then [`Refusal::RETIRED`]), kept so no other
    /// condition is ever given one.
    pub fn retired() -> impl Iterator<Item = &'static str> {
        Self::RETIRED.iter().chain(Refusal::RETIRED).copied()
    }

    /// The code as the error body's `code` member carries it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BodyInvalid => "body-invalid",
            Self::CompletenessInvalid => "completeness-invalid",
            Self::PartialUnsupported => "partial-unsupported",
            Self::DedupInvalid => "dedup-invalid",
            Self::ParameterInvalid => "parameter-invalid",
            Self::PatientInvalid => "patient-invalid",
            Self::Refused(RefusalCode(kind)) => kind,
            Self::NoDestination => "no-destination",
            Self::EhrIdCollision => "ehr-id-collision",
            Self::ControllingSystemUnreachable => "controlling-system-unreachable",
            Self::Internal => "internal",
            Self::NotFound => "not-found",
            Self::NotImplemented => "not-implemented",
            Self::EndpointUnknown => "endpoint-unknown",
            Self::OrganisationUnknown => "organisation-unknown",
            Self::TargetRequired => "target-required",
            Self::EndpointSeveral => "endpoint-several",
            Self::QueryParameterRefused => "query-parameter-refused",
            Self::NodeTimeout => "node-timeout",
            Self::NodeUnreachable => "node-unreachable",
            Self::NodeRefused => "node-refused",
            Self::TargetingConflict => "targeting-conflict",
            Self::EhrIdInvalid => "ehr-id-invalid",
            Self::NodeError => "node-error",
            Self::ProbeRequiresUuid => "probe-requires-uuid",
            Self::QueryNameInvalid => "query-name-invalid",
            Self::QueryVersionInvalid => "query-version-invalid",
            Self::QueryTypeUnsupported => "query-type-unsupported",
            Self::SubjectLiteral => "subject-literal",
            Self::StoredQueryHeld => "stored-query-held",
            Self::StoredQueryUnknown => "stored-query-unknown",
            Self::PrecedingVersionInvalid => "preceding-version-invalid",
            Self::ParameterValueInvalid => "parameter-value-invalid",
            Self::MediaTypeNotAcceptable => "media-type-not-acceptable",
            Self::MediaTypeUnsupported => "media-type-unsupported",
            Self::SubjectSeveral => "subject-several",
            Self::ResolutionUnavailable => "resolution-unavailable",
            Self::EhrIdHeld => "ehr-id-held",
            Self::DefinitionEndpointTargeted => "definition-endpoint-targeted",
            Self::StoredQueryFanOutUnsupported => "stored-query-fan-out-unsupported",
            Self::StoredQueryReadOnly => "stored-query-read-only",
            Self::StoredQueryReserved => "stored-query-reserved",
            Self::ConsentDenied => "consent-denied",
            Self::Unauthenticated => "unauthenticated",
            Self::ScopeInsufficient => "scope-insufficient",
            Self::PurposeOfUseRequired => "purpose-of-use-required",
            Self::AuthenticationUnavailable => "authentication-unavailable",
            Self::OperationRefused => "operation-refused",
            Self::LocalizationUnavailable => "localization-unavailable",
            Self::PatientContextMissing => "patient-context-missing",
            Self::PatientConfinement => "patient-confinement",
            Self::PatientContextUnavailable => "patient-context-unavailable",
            Self::SubjectUnavailable => "subject-unavailable",
            Self::Overloaded => "overloaded",
            Self::RateLimited => "rate-limited",
            Self::AccessUnrecorded => "access-unrecorded",
            Self::NaturalPersonRequired => "natural-person-required",
            Self::AuthenticationAssuranceInsufficient => "authentication-assurance-insufficient",
            Self::ContactPointAttributesRequired => "contact-point-attributes-required",
            Self::CorrelationInvalid => "correlation-invalid",
            Self::ConfigurationInvalid => "configuration-invalid",
            Self::RevisionStale => "revision-stale",
            Self::RevisionUnknown => "revision-unknown",
            Self::RollbackUnavailable => "rollback-unavailable",
            Self::ResourceFileOwned => "resource-file-owned",
            Self::ResourceUnknown => "resource-unknown",
            Self::ConfigurationUnmanaged => "configuration-unmanaged",
            Self::ApprovalRequired => "approval-required",
            Self::ApprovalOwn => "approval-own-revision",
            Self::ApprovalNotRequired => "approval-not-required",
            Self::OperatorUnverified => "operator-unverified",
            Self::ConfirmationUnavailable => "confirmation-unavailable",
            Self::ConfirmationPending => "confirmation-pending",
            Self::SecretStoreUnavailable => "secret-store-unavailable",
            Self::CredentialMalformed => "credential-malformed",
            Self::EmergencyLimitReached => "emergency-limit-reached",
            Self::EmergencyUncounted => "emergency-uncounted",
            Self::TrcMissing => "trc-missing",
            Self::TrcUnbound => "trc-unbound",
        }
    }

    /// The HTTP status this code answers with (§11.2).
    #[must_use]
    pub fn status(self) -> StatusCode {
        match self {
            Self::BodyInvalid
            | Self::CompletenessInvalid
            | Self::PartialUnsupported
            | Self::DedupInvalid
            | Self::ParameterInvalid
            | Self::PatientInvalid
            | Self::Refused(_)
            | Self::EndpointUnknown
            | Self::OrganisationUnknown
            | Self::TargetRequired
            | Self::EndpointSeveral
            | Self::QueryParameterRefused
            | Self::TargetingConflict
            | Self::EhrIdInvalid
            | Self::ProbeRequiresUuid
            | Self::QueryNameInvalid
            | Self::QueryVersionInvalid
            | Self::QueryTypeUnsupported
            | Self::SubjectLiteral
            | Self::PrecedingVersionInvalid
            | Self::ParameterValueInvalid
            | Self::DefinitionEndpointTargeted
            | Self::StoredQueryFanOutUnsupported
            | Self::SubjectSeveral
            | Self::CorrelationInvalid
            | Self::CredentialMalformed => StatusCode::BAD_REQUEST,
            Self::NoDestination
            | Self::NotFound
            | Self::StoredQueryUnknown
            | Self::SubjectUnavailable
            | Self::RevisionUnknown
            | Self::ResourceUnknown
            | Self::ConfigurationUnmanaged => StatusCode::NOT_FOUND,
            Self::EhrIdCollision
            | Self::ControllingSystemUnreachable
            | Self::StoredQueryHeld
            | Self::StoredQueryReserved
            | Self::EhrIdHeld
            | Self::RevisionStale
            | Self::RollbackUnavailable
            | Self::ResourceFileOwned
            | Self::ApprovalRequired
            | Self::ApprovalNotRequired
            | Self::ConfirmationUnavailable
            | Self::ConfirmationPending
            | Self::SecretStoreUnavailable => StatusCode::CONFLICT,
            Self::ConfigurationInvalid => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::NodeTimeout | Self::NodeUnreachable => StatusCode::GATEWAY_TIMEOUT,
            Self::NodeRefused
            | Self::NodeError
            | Self::ResolutionUnavailable
            | Self::LocalizationUnavailable
            | Self::PatientContextUnavailable => StatusCode::FAILED_DEPENDENCY,
            Self::MediaTypeNotAcceptable => StatusCode::NOT_ACCEPTABLE,
            Self::MediaTypeUnsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::StoredQueryReadOnly => StatusCode::METHOD_NOT_ALLOWED,
            Self::Unauthenticated
            | Self::NaturalPersonRequired
            | Self::AuthenticationAssuranceInsufficient => StatusCode::UNAUTHORIZED,
            Self::ConsentDenied
            | Self::ScopeInsufficient
            | Self::PurposeOfUseRequired
            | Self::OperationRefused
            | Self::PatientContextMissing
            | Self::PatientConfinement
            | Self::ContactPointAttributesRequired
            | Self::TrcMissing
            | Self::TrcUnbound
            | Self::ApprovalOwn
            | Self::OperatorUnverified
            | Self::EmergencyLimitReached => StatusCode::FORBIDDEN,
            Self::AuthenticationUnavailable
            | Self::Overloaded
            | Self::AccessUnrecorded
            | Self::EmergencyUncounted => StatusCode::SERVICE_UNAVAILABLE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use http::StatusCode;
    use openehr_federation::aql::refusal::Refusal;

    use crate::error::{Code, RefusalCode};

    /// The declaration position of `code`'s variant; no wildcard, so a new
    /// variant fails to compile here until [`Code::GATEWAY`] lists it.
    fn ordinal(code: Code) -> Option<usize> {
        match code {
            Code::BodyInvalid => Some(0),
            Code::CompletenessInvalid => Some(1),
            Code::PartialUnsupported => Some(2),
            Code::DedupInvalid => Some(3),
            Code::ParameterInvalid => Some(4),
            Code::PatientInvalid => Some(5),
            Code::Refused(_) => None,
            Code::NoDestination => Some(6),
            Code::EhrIdCollision => Some(7),
            Code::ControllingSystemUnreachable => Some(8),
            Code::Internal => Some(9),
            Code::NotFound => Some(10),
            Code::NotImplemented => Some(11),
            Code::EndpointUnknown => Some(12),
            Code::OrganisationUnknown => Some(13),
            Code::TargetRequired => Some(14),
            Code::EndpointSeveral => Some(15),
            Code::QueryParameterRefused => Some(16),
            Code::NodeTimeout => Some(17),
            Code::NodeUnreachable => Some(18),
            Code::NodeRefused => Some(19),
            Code::TargetingConflict => Some(20),
            Code::EhrIdInvalid => Some(21),
            Code::NodeError => Some(22),
            Code::ProbeRequiresUuid => Some(23),
            Code::QueryNameInvalid => Some(24),
            Code::QueryVersionInvalid => Some(25),
            Code::QueryTypeUnsupported => Some(26),
            Code::SubjectLiteral => Some(27),
            Code::StoredQueryHeld => Some(28),
            Code::StoredQueryUnknown => Some(29),
            Code::PrecedingVersionInvalid => Some(30),
            Code::ParameterValueInvalid => Some(31),
            Code::MediaTypeNotAcceptable => Some(32),
            Code::MediaTypeUnsupported => Some(33),
            Code::SubjectSeveral => Some(34),
            Code::ResolutionUnavailable => Some(35),
            Code::EhrIdHeld => Some(36),
            Code::DefinitionEndpointTargeted => Some(37),
            Code::StoredQueryFanOutUnsupported => Some(38),
            Code::StoredQueryReadOnly => Some(39),
            Code::StoredQueryReserved => Some(40),
            Code::ConsentDenied => Some(41),
            Code::Unauthenticated => Some(42),
            Code::ScopeInsufficient => Some(43),
            Code::PurposeOfUseRequired => Some(44),
            Code::AuthenticationUnavailable => Some(45),
            Code::OperationRefused => Some(46),
            Code::LocalizationUnavailable => Some(47),
            Code::PatientContextMissing => Some(48),
            Code::PatientConfinement => Some(49),
            Code::PatientContextUnavailable => Some(50),
            Code::SubjectUnavailable => Some(51),
            Code::Overloaded => Some(52),
            Code::RateLimited => Some(53),
            Code::AccessUnrecorded => Some(54),
            Code::NaturalPersonRequired => Some(55),
            Code::AuthenticationAssuranceInsufficient => Some(56),
            Code::ContactPointAttributesRequired => Some(57),
            Code::CorrelationInvalid => Some(58),
            Code::ConfigurationInvalid => Some(59),
            Code::RevisionStale => Some(60),
            Code::RevisionUnknown => Some(61),
            Code::RollbackUnavailable => Some(62),
            Code::ResourceFileOwned => Some(63),
            Code::ResourceUnknown => Some(64),
            Code::ConfigurationUnmanaged => Some(65),
            Code::ApprovalRequired => Some(66),
            Code::ApprovalOwn => Some(67),
            Code::ApprovalNotRequired => Some(68),
            Code::OperatorUnverified => Some(69),
            Code::ConfirmationUnavailable => Some(70),
            Code::ConfirmationPending => Some(71),
            Code::SecretStoreUnavailable => Some(72),
            Code::CredentialMalformed => Some(73),
            Code::EmergencyLimitReached => Some(74),
            Code::EmergencyUncounted => Some(75),
            Code::TrcMissing => Some(76),
            Code::TrcUnbound => Some(77),
        }
    }

    #[test]
    fn gateway_lists_every_code_that_is_not_a_refusal_once() {
        let ordinals: Vec<Option<usize>> = Code::GATEWAY.into_iter().map(ordinal).collect();
        assert_eq!(
            (0..Code::GATEWAY.len()).map(Some).collect::<Vec<_>>(),
            ordinals
        );
    }

    #[test]
    fn every_code_is_distinct_and_lower_kebab_case() {
        let codes: Vec<&str> = Code::every().map(Code::as_str).collect();
        let distinct: BTreeSet<&str> = codes.iter().copied().collect();
        assert_eq!(codes.len(), distinct.len(), "{codes:?}");
        assert_eq!(Code::GATEWAY.len() + Refusal::KINDS.len(), codes.len());
        for retired in Code::retired() {
            assert!(
                !distinct.contains(retired),
                "{retired} is retired, so no condition answers it"
            );
        }
        for code in codes.into_iter().chain(Code::retired()) {
            assert!(
                !code.is_empty()
                    && !code.starts_with('-')
                    && !code.ends_with('-')
                    && code.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{code}"
            );
        }
    }

    #[test]
    fn a_retired_code_is_listed_once_and_the_unversioned_put_code_is_one() {
        let retired: Vec<&str> = Code::retired().collect();
        let distinct: BTreeSet<&str> = retired.iter().copied().collect();
        assert_eq!(retired.len(), distinct.len(), "{retired:?}");
        assert!(
            distinct.contains("query-version-required"),
            "§12.7 stored-query-versioning: an unversioned PUT is stored"
        );
    }

    #[test]
    fn every_code_is_a_client_or_a_server_error() {
        for code in Code::every() {
            let status = code.status();
            assert!(
                status.is_client_error() || status.is_server_error(),
                "{code:?}"
            );
        }
    }

    // NOTE: §11.2, the status of each condition the gateway itself reports.
    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one row per code: the table of every code's status"
    )]
    fn each_code_answers_its_section_11_2_status() {
        let table = [
            (Code::BodyInvalid, StatusCode::BAD_REQUEST),
            (Code::CompletenessInvalid, StatusCode::BAD_REQUEST),
            (Code::PartialUnsupported, StatusCode::BAD_REQUEST),
            (Code::DedupInvalid, StatusCode::BAD_REQUEST),
            (Code::ParameterInvalid, StatusCode::BAD_REQUEST),
            (Code::PatientInvalid, StatusCode::BAD_REQUEST),
            (Code::NoDestination, StatusCode::NOT_FOUND),
            (Code::EhrIdCollision, StatusCode::CONFLICT),
            (Code::ControllingSystemUnreachable, StatusCode::CONFLICT),
            (Code::Internal, StatusCode::INTERNAL_SERVER_ERROR),
            (Code::NotFound, StatusCode::NOT_FOUND),
            (Code::NotImplemented, StatusCode::NOT_IMPLEMENTED),
            (Code::EndpointUnknown, StatusCode::BAD_REQUEST),
            (Code::OrganisationUnknown, StatusCode::BAD_REQUEST),
            (Code::TargetRequired, StatusCode::BAD_REQUEST),
            (Code::EndpointSeveral, StatusCode::BAD_REQUEST),
            (Code::QueryParameterRefused, StatusCode::BAD_REQUEST),
            (Code::NodeTimeout, StatusCode::GATEWAY_TIMEOUT),
            (Code::NodeUnreachable, StatusCode::GATEWAY_TIMEOUT),
            (Code::NodeRefused, StatusCode::FAILED_DEPENDENCY),
            (Code::TargetingConflict, StatusCode::BAD_REQUEST),
            (Code::EhrIdInvalid, StatusCode::BAD_REQUEST),
            (Code::NodeError, StatusCode::FAILED_DEPENDENCY),
            (Code::ProbeRequiresUuid, StatusCode::BAD_REQUEST),
            (Code::QueryNameInvalid, StatusCode::BAD_REQUEST),
            (Code::QueryVersionInvalid, StatusCode::BAD_REQUEST),
            (Code::QueryTypeUnsupported, StatusCode::BAD_REQUEST),
            (Code::SubjectLiteral, StatusCode::BAD_REQUEST),
            (Code::StoredQueryHeld, StatusCode::CONFLICT),
            (Code::StoredQueryUnknown, StatusCode::NOT_FOUND),
            (Code::PrecedingVersionInvalid, StatusCode::BAD_REQUEST),
            (Code::ParameterValueInvalid, StatusCode::BAD_REQUEST),
            (Code::MediaTypeNotAcceptable, StatusCode::NOT_ACCEPTABLE),
            (
                Code::MediaTypeUnsupported,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            (Code::SubjectSeveral, StatusCode::BAD_REQUEST),
            (Code::ResolutionUnavailable, StatusCode::FAILED_DEPENDENCY),
            (Code::EhrIdHeld, StatusCode::CONFLICT),
            (Code::DefinitionEndpointTargeted, StatusCode::BAD_REQUEST),
            (Code::StoredQueryFanOutUnsupported, StatusCode::BAD_REQUEST),
            (Code::StoredQueryReadOnly, StatusCode::METHOD_NOT_ALLOWED),
            (Code::StoredQueryReserved, StatusCode::CONFLICT),
            (Code::ConsentDenied, StatusCode::FORBIDDEN),
            (Code::Unauthenticated, StatusCode::UNAUTHORIZED),
            (Code::ScopeInsufficient, StatusCode::FORBIDDEN),
            (Code::PurposeOfUseRequired, StatusCode::FORBIDDEN),
            (
                Code::AuthenticationUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (Code::OperationRefused, StatusCode::FORBIDDEN),
            (Code::LocalizationUnavailable, StatusCode::FAILED_DEPENDENCY),
            (Code::PatientContextMissing, StatusCode::FORBIDDEN),
            (Code::PatientConfinement, StatusCode::FORBIDDEN),
            (
                Code::PatientContextUnavailable,
                StatusCode::FAILED_DEPENDENCY,
            ),
            (Code::SubjectUnavailable, StatusCode::NOT_FOUND),
            (Code::Overloaded, StatusCode::SERVICE_UNAVAILABLE),
            (Code::RateLimited, StatusCode::TOO_MANY_REQUESTS),
            (Code::AccessUnrecorded, StatusCode::SERVICE_UNAVAILABLE),
            (Code::NaturalPersonRequired, StatusCode::UNAUTHORIZED),
            (
                Code::AuthenticationAssuranceInsufficient,
                StatusCode::UNAUTHORIZED,
            ),
            (Code::ContactPointAttributesRequired, StatusCode::FORBIDDEN),
            (Code::CorrelationInvalid, StatusCode::BAD_REQUEST),
            (Code::ConfigurationInvalid, StatusCode::UNPROCESSABLE_ENTITY),
            (Code::RevisionStale, StatusCode::CONFLICT),
            (Code::RevisionUnknown, StatusCode::NOT_FOUND),
            (Code::RollbackUnavailable, StatusCode::CONFLICT),
            (Code::ResourceFileOwned, StatusCode::CONFLICT),
            (Code::ResourceUnknown, StatusCode::NOT_FOUND),
            (Code::ConfigurationUnmanaged, StatusCode::NOT_FOUND),
            (Code::ApprovalRequired, StatusCode::CONFLICT),
            (Code::ApprovalOwn, StatusCode::FORBIDDEN),
            (Code::ApprovalNotRequired, StatusCode::CONFLICT),
            (Code::OperatorUnverified, StatusCode::FORBIDDEN),
            (Code::ConfirmationUnavailable, StatusCode::CONFLICT),
            (Code::ConfirmationPending, StatusCode::CONFLICT),
            (Code::SecretStoreUnavailable, StatusCode::CONFLICT),
            (Code::CredentialMalformed, StatusCode::BAD_REQUEST),
            (Code::EmergencyLimitReached, StatusCode::FORBIDDEN),
            (Code::EmergencyUncounted, StatusCode::SERVICE_UNAVAILABLE),
            (Code::TrcMissing, StatusCode::FORBIDDEN),
            (Code::TrcUnbound, StatusCode::FORBIDDEN),
        ];
        assert_eq!(Code::GATEWAY.len(), table.len());
        for (code, status) in table {
            assert_eq!(status, code.status(), "{code:?}");
        }
        for kind in Refusal::KINDS {
            assert_eq!(
                StatusCode::BAD_REQUEST,
                Code::Refused(RefusalCode(kind)).status(),
                "a refused query is a 400 (§11.2, §7.1): {kind}"
            );
        }
    }

    #[test]
    fn a_refusal_code_is_the_refusals_kind() {
        let refusal = Refusal::OffsetUnsupported;
        assert_eq!(
            "offset-unsupported",
            Code::Refused(RefusalCode::from(&refusal)).as_str()
        );
    }
}
