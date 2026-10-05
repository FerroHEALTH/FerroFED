// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each [`Code`] says: its text on the wire, its HTTP status (§11.2)
//! and its fixed message.

use http::StatusCode;
use openehr_federation::aql::refusal::Refusal;

use super::{Code, RefusalCode};

impl Code {
    /// Every code that is not a refusal, in declaration order.
    pub const GATEWAY: [Self; 52] = [
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
        Self::QueryVersionRequired,
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
    ];

    /// Every code: [`Code::GATEWAY`], then one per [`Refusal::KINDS`].
    pub fn every() -> impl Iterator<Item = Self> {
        Self::GATEWAY.into_iter().chain(
            Refusal::KINDS
                .iter()
                .map(|kind| Self::Refused(RefusalCode(kind))),
        )
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
            Self::QueryVersionRequired => "query-version-required",
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
            | Self::QueryVersionRequired
            | Self::QueryTypeUnsupported
            | Self::SubjectLiteral
            | Self::PrecedingVersionInvalid
            | Self::ParameterValueInvalid
            | Self::DefinitionEndpointTargeted
            | Self::StoredQueryFanOutUnsupported => StatusCode::BAD_REQUEST,
            Self::NoDestination
            | Self::NotFound
            | Self::StoredQueryUnknown
            | Self::SubjectUnavailable => StatusCode::NOT_FOUND,
            Self::EhrIdCollision
            | Self::ControllingSystemUnreachable
            | Self::StoredQueryHeld
            | Self::SubjectSeveral
            | Self::EhrIdHeld => StatusCode::CONFLICT,
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
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::ConsentDenied
            | Self::ScopeInsufficient
            | Self::PurposeOfUseRequired
            | Self::OperationRefused
            | Self::PatientContextMissing
            | Self::PatientConfinement => StatusCode::FORBIDDEN,
            Self::AuthenticationUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The fixed `message` of a failure that has no more to say than its
    /// code.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per code: the table of every fixed message"
    )]
    pub fn message(self) -> &'static str {
        match self {
            Self::BodyInvalid => "the request body is not the ITS-REST request this route takes",
            Self::CompletenessInvalid => {
                "the openEHR-federation-completeness header takes \"all\" or \"partial\", once"
            }
            Self::PartialUnsupported => "best-effort completion is not offered (§11.4)",
            Self::DedupInvalid => {
                "the openEHR-federation-dedup header takes \"none\" or \"version-identity\", once"
            }
            Self::ParameterInvalid => "a query parameter is not an AQL literal",
            Self::PatientInvalid => "the patient reference cannot be formed (§5.2)",
            Self::Refused(_) => "the query is refused",
            Self::NoDestination => "the request can be routed to no destination (§11.2)",
            Self::EhrIdCollision => "the ehr_id is claimed by more than one node (N42)",
            Self::ControllingSystemUnreachable => {
                "no reachable node is the controlling system of this version (N36)"
            }
            Self::Internal => "the gateway failed on its own side",
            Self::NotFound => "no resource is served at this path",
            Self::NotImplemented => "this part of the ITS-REST API is not served by the gateway",
            Self::EndpointUnknown => {
                "the endpoint directive or header names an endpoint the registry does not know (§8.4.1)"
            }
            Self::OrganisationUnknown => {
                "the organisation directive or header names an organisation the registry does not know (§8.4.1)"
            }
            Self::TargetRequired => {
                "a write that no binding or index routes to one node, the creation of an EHR, a definition request and a DEMOGRAPHIC request name their node in the openEHR-federation-endpoint header (§7a.1, §12.4, §12.5.1, §12.6, N23, N41, N43)"
            }
            Self::EndpointSeveral => {
                "a request routed to one node selects exactly one endpoint through its targeting headers (§7a.1)"
            }
            Self::QueryParameterRefused => {
                "a query parameter the ITS-REST operation does not declare is refused, never forwarded (§5.4.1, N33)"
            }
            Self::NodeTimeout => "the node did not answer in time (§11.2)",
            Self::NodeUnreachable => "the node could not be reached (§11.2)",
            Self::NodeRefused => "the node refused the gateway's onward credentials (§11.2)",
            Self::TargetingConflict => {
                "the request's targeting mechanisms select different node sets (§8.4.1, N35)"
            }
            Self::EhrIdInvalid => "the ehr_id in the path is not an openEHR HIER_OBJECT_ID (§12.5)",
            Self::NodeError => "a node answered with an error (§11.2)",
            Self::ProbeRequiresUuid => {
                "a read of an EHR resource that no header, binding or index routes to one node is probed at every member only when its ehr_id is a UUID, so name its node in the openEHR-federation-endpoint header (§5.4.1, N33, §12.5.1)"
            }
            Self::QueryNameInvalid => {
                "a stored query name is [{namespace}::]{query-name} over a-z, A-Z, 0-9, _, . and -, and never aql"
            }
            Self::QueryVersionInvalid => {
                "a stored query version is major.minor.patch, or {major} or {major}.{minor} where a version is looked up"
            }
            Self::QueryVersionRequired => {
                "the registry stores a definition at a version: PUT {base}/v1/definition/query/{name}/{version} (§12.7)"
            }
            Self::QueryTypeUnsupported => "the registry stores AQL only",
            Self::SubjectLiteral => {
                "a stored query names its patient through a $parameter, never a literal the registry would hold (§5.4.1, N33)"
            }
            Self::StoredQueryHeld => {
                "the registry holds this name and version, and a stored version is immutable: store a new version (§12.7, N44)"
            }
            Self::StoredQueryUnknown => {
                "the registry holds no stored query at this name and version"
            }
            Self::PrecedingVersionInvalid => {
                "a versioned write names the version it amends as one quoted OBJECT_VERSION_ID in If-Match, or in the path of a DELETE, so its controlling CDR can be found (§12.4, N23)"
            }
            Self::ParameterValueInvalid => {
                "a path identifier, query value or header routed to one node matches what its ITS-REST operation declares, or nothing is sent (§5.4.1, N33)"
            }
            Self::MediaTypeNotAcceptable => {
                "the Accept header admits none of the media types the ITS-REST operation answers in"
            }
            Self::MediaTypeUnsupported => {
                "the Content-Type header is not a media type the ITS-REST operation takes"
            }
            Self::SubjectSeveral => {
                "the subject resolves at more than one member, and the gateway never chooses between them: name one in the openEHR-federation-endpoint header (§8.4, §12.5.2)"
            }
            Self::ResolutionUnavailable => {
                "the cross-reference service could not answer, so where the subject has an EHR is unknown (§5.2, §11.2)"
            }
            Self::EhrIdHeld => {
                "the ehr_id is already held at another member, so no EHR is created under it at the endpoint named (§12.4, §12.5.2)"
            }
            Self::DefinitionEndpointTargeted => {
                "a stored query whose AQL carries a FROM ENDPOINT or ORGANISATION directive is never distributed: a node cannot execute it. Nothing was stored; store it without naming members in the targeting headers, and it runs federated (§12.7, §8.1, N44)"
            }
            Self::StoredQueryFanOutUnsupported => {
                "this gateway does not distribute stored-query definitions to members or report their copies, and definition.stored_query_fan_out is false: nothing was stored or read; send the request without openEHR-federation-endpoint or openEHR-federation-organisation (§12.7, N44)"
            }
            Self::StoredQueryReadOnly => {
                "this gateway's stored-query registry is read-only: its definitions are loaded from files when it starts, so nothing was stored; read or run a definition it holds (§12.7, N44)"
            }
            Self::ConsentDenied => {
                "the consent pre-filter does not permit asking the members that might hold this subject's EHR, and no other member holds one (N27a)"
            }
            Self::Unauthenticated => "no access token this gateway accepts (§13.1, N25)",
            Self::ScopeInsufficient => "no scope of the access token grants this operation",
            Self::PurposeOfUseRequired => "the access token declares no purpose of use (§13.4)",
            Self::AuthenticationUnavailable => "the access token cannot be verified now (§13.1)",
            Self::OperationRefused => "this gateway admits no caller to this operation",
            Self::LocalizationUnavailable => {
                "the localizer could not answer, or its exchange could not be audited, so no member was asked and where the subject has an EHR is unknown (§14.1, N4)"
            }
            Self::PatientContextMissing => {
                "only a patient/ grant covers this operation, and the access token carries no ehrId to confine it to"
            }
            Self::PatientConfinement => {
                "the request reaches beyond the patient the access token's patient/ grant is confined to, or names no patient, so nothing was sent (§5.2, §12.5)"
            }
            Self::PatientContextUnavailable => {
                "the cross-reference could not resolve the patient of the access token's patient/ grant, so the grant cannot be confined and nothing was sent (§5.2, §11.2)"
            }
            Self::SubjectUnavailable => {
                "the requested resource is not available to this request (§11.2)"
            }
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
            Code::QueryVersionRequired => Some(26),
            Code::QueryTypeUnsupported => Some(27),
            Code::SubjectLiteral => Some(28),
            Code::StoredQueryHeld => Some(29),
            Code::StoredQueryUnknown => Some(30),
            Code::PrecedingVersionInvalid => Some(31),
            Code::ParameterValueInvalid => Some(32),
            Code::MediaTypeNotAcceptable => Some(33),
            Code::MediaTypeUnsupported => Some(34),
            Code::SubjectSeveral => Some(35),
            Code::ResolutionUnavailable => Some(36),
            Code::EhrIdHeld => Some(37),
            Code::DefinitionEndpointTargeted => Some(38),
            Code::StoredQueryFanOutUnsupported => Some(39),
            Code::StoredQueryReadOnly => Some(40),
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
        for code in codes {
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
            (Code::QueryVersionRequired, StatusCode::BAD_REQUEST),
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
            (Code::SubjectSeveral, StatusCode::CONFLICT),
            (Code::ResolutionUnavailable, StatusCode::FAILED_DEPENDENCY),
            (Code::EhrIdHeld, StatusCode::CONFLICT),
            (Code::DefinitionEndpointTargeted, StatusCode::BAD_REQUEST),
            (Code::StoredQueryFanOutUnsupported, StatusCode::BAD_REQUEST),
            (Code::StoredQueryReadOnly, StatusCode::METHOD_NOT_ALLOWED),
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
