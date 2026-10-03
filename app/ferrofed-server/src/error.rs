// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error vocabulary: every condition the gateway answers with an error
//! body, its HTTP status (§11.2) and the stable code the body names.
//!
//! An error body is the ITS-REST `Error`, `message` and `validationErrors`,
//! with two members added: `code`, one of the codes of [`Code`], and
//! `request_id`, the exchange id of [`crate::request_id`]: the client's own
//! when it named the request, otherwise the gateway's, which the log records
//! too. Two
//! failures answer with another body. A fan-out that fails under
//! all-or-nothing (`504`, `424`) answers the `RESULT_SET` of §11.4, whose
//! `meta.federation.endpoints[]` statuses say which node failed and why,
//! and which echoes the client's own `q` (N17, N37). A node's own answer on a
//! single-node route passes through as the node sent it (§11.2).
//!
//! The codes are API: a code is only ever added, never renamed, removed or
//! moved to another status. No error body quotes a parameter value, the query
//! text, a path or a header value (§5.4.3). No specification defines the
//! `code` member or its values: our own design.

use std::collections::BTreeMap;

use axum::Json;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use openehr_federation::aql::refusal::Refusal;
use openehr_its::rest::generated::common::Error;

/// The stable code of a failure the gateway answers with an error body.
///
/// [`Code::status`] is the one table from a code to its HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Code {
    /// The request body is not the ITS-REST request the route takes.
    BodyInvalid,
    /// The `openEHR-federation-completeness` header is repeated, or carries
    /// neither `all` nor `partial` (§11.4).
    CompletenessInvalid,
    /// The request asks for best-effort, which this gateway does not offer
    /// (§11.4, N37).
    PartialUnsupported,
    /// The `openEHR-federation-dedup` header is repeated, or names no dedup
    /// mode the gateway offers (§10, §7a.2).
    DedupInvalid,
    /// A query parameter is not an AQL literal. The body names the parameter
    /// and never its value.
    ParameterInvalid,
    /// The query's patient identifier or namespace cannot form a patient
    /// reference (§5.2), or `GET {base}/v1/ehr` does not carry
    /// `subject_id` and `subject_namespace` once each.
    PatientInvalid,
    /// The query is refused before anything is dispatched (§5.4.1, §7.1,
    /// §11.6): the code is the refusal's kind.
    Refused(RefusalCode),
    /// The request can be routed to no destination at all (§11.2, §11.3).
    NoDestination,
    /// The `ehr_id` is claimed by more than one node (§12.5.2, N42).
    EhrIdCollision,
    /// The node a versioned write's path `ehr_id` routes to is not the
    /// controlling system of the version it amends, and no other node is sent
    /// it (§10.3, §12a.1, N23, N36).
    ControllingSystemUnreachable,
    /// The gateway failed on its own side (§11.2).
    Internal,
    /// The path is outside every surface the gateway serves.
    NotFound,
    /// The path is an ITS-REST area the gateway does not expose (§7a.1, N32).
    NotImplemented,
    /// The `FROM ENDPOINT` directive or the `openEHR-federation-endpoint`
    /// header names an endpoint the registry does not know, or the header
    /// names none (§8.4.1, N19).
    EndpointUnknown,
    /// The `ORGANISATION` directive or the `openEHR-federation-organisation`
    /// header names an organisation the registry does not know, or the
    /// header names none (§8.1, §8.4.1, N20).
    OrganisationUnknown,
    /// A request that only an explicit target can route names no node: a
    /// write to an EHR resource that no held binding or `ehr_id` index entry
    /// routes to exactly one (§12.5.1, N41), the creation of an EHR (§12.4,
    /// N23), a definition request (§12.6, N43), or a DEMOGRAPHIC request
    /// (§7a.1, N32), which only the targeting headers route.
    TargetRequired,
    /// The targeting headers of a request routed to a single node select
    /// more than one endpoint (§7a.1, §12.4).
    EndpointSeveral,
    /// The query string of a request routed to a single node carries a
    /// parameter the ITS-REST operation does not declare (§5.4.1, N33). The
    /// body names the parameter by position, never by name or value.
    QueryParameterRefused,
    /// The node a request was routed to did not answer in time (§11.2).
    NodeTimeout,
    /// The node a request was routed to could not be reached (§11.2).
    NodeUnreachable,
    /// The node a request was routed to refused the gateway's onward
    /// credentials (§11.2).
    NodeRefused,
    /// Two targeting mechanisms of one request, the AQL directive and a
    /// header or the two headers, select different node sets (§8.4.1, N35),
    /// or the headers of a DEMOGRAPHIC request name an endpoint other than
    /// the one the deployment declared for that area (§7a.1, N32).
    /// The body names both sets.
    TargetingConflict,
    /// The `ehr_id` in a request path, or the one a query is scoped to, is
    /// not an openEHR `HIER_OBJECT_ID` (§12.5, N29).
    EhrIdInvalid,
    /// A node answered with an error, so the request cannot be completed
    /// (§11.2): a member answered the ask-all probe of a path `ehr_id` with
    /// neither a success nor `404` (§12.5.1).
    NodeError,
    /// A read of an EHR resource, or a query scoped to one `ehr_id`, that no
    /// targeting header, binding or index routes to one node has an `ehr_id`
    /// that is no bare UUID, so it is never probed at every member (§5.4.1,
    /// N33, §12.5.1).
    ProbeRequiresUuid,
    /// A stored query's name is not `[{namespace}::]{query-name}` over the
    /// ITS-REST characters, or is the reserved `aql`.
    QueryNameInvalid,
    /// A stored query's version is not `major.minor.patch`, or, where a
    /// version is looked up, a `{major}` or `{major}.{minor}` prefix.
    QueryVersionInvalid,
    /// A stored query is `PUT` without a version, which the registry
    /// requires (§12.7, N44).
    QueryVersionRequired,
    /// A stored query's `query_type` is not AQL.
    QueryTypeUnsupported,
    /// A stored-query definition names the patient by a literal, which the
    /// registry would hold at rest; the patient is a `$parameter` (§5.4.1,
    /// N33).
    SubjectLiteral,
    /// The registry already holds the stored query's name and version, and
    /// the held definition stands unchanged (§12.7, N44).
    StoredQueryHeld,
    /// The registry holds no stored query at the name and version (§12.7).
    StoredQueryUnknown,
    /// A versioned write names the version it amends by no single
    /// `OBJECT_VERSION_ID`: none, several, or one that is malformed, in
    /// `If-Match` or in the path of a `DELETE`, so its controlling CDR cannot
    /// be found (§12.4, N23).
    PrecedingVersionInvalid,
    /// A path identifier, query value or header of a request routed to a
    /// single node does not match what the ITS-REST operation declares for it
    /// (§5.4.1, N33). The body names the header, or the parameter by
    /// position, never the value.
    ParameterValueInvalid,
    /// The `Accept` header of a request routed to a single node admits none
    /// of the media types the ITS-REST operation answers in (RFC 9110
    /// §12.5.1).
    MediaTypeNotAcceptable,
    /// The `Content-Type` header of a request routed to a single node, or of
    /// a request the gateway answers itself, names none of the media types
    /// the ITS-REST operation takes, or a parameter other than a `utf-8`
    /// charset (RFC 9110 §8.3, §15.5.16).
    MediaTypeUnsupported,
    /// The subject of `GET {base}/v1/ehr` resolves at more than one member,
    /// and no targeting header names one of them: the gateway never chooses
    /// by where the patient resolved (§12.5.2). The body lists the endpoints.
    SubjectSeveral,
    /// The cross-reference service could not answer for a member, so where
    /// the subject of `GET {base}/v1/ehr` has its EHR is unknown (§5.2,
    /// §11.2). The body names the members, never the subject.
    ResolutionUnavailable,
    /// `PUT {base}/v1/ehr/{ehr_id}` targets one member while a resolution
    /// binding or the `ehr_id` index places the `ehr_id` at another, so the
    /// EHR is created nowhere (§12.4, §12.5.2; ITS-REST 1.1.0
    /// `ehr_create_with_id`). The body names the endpoints, never the
    /// `ehr_id`.
    EhrIdHeld,
    /// A stored-query definition whose AQL carries a `FROM ENDPOINT` or
    /// `ORGANISATION` directive is asked to be distributed to members, which
    /// cannot execute it; nothing is stored (§12.7, §8.1, N44).
    DefinitionEndpointTargeted,
    /// A stored-query `PUT` or version `GET` at the registry carries a
    /// targeting header, asking for distribution or a drift report, which
    /// this gateway does not offer; nothing is stored or read (§12.7, N44).
    StoredQueryFanOutUnsupported,
}

/// The code of a refused query: the refusal's stable kind
/// ([`Refusal::kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefusalCode(&'static str);

impl From<&Refusal> for RefusalCode {
    fn from(refusal: &Refusal) -> Self {
        Self(refusal.kind())
    }
}

impl Code {
    /// Every code that is not a refusal, in declaration order.
    pub const GATEWAY: [Self; 40] = [
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
            Self::NoDestination | Self::NotFound | Self::StoredQueryUnknown => {
                StatusCode::NOT_FOUND
            }
            Self::EhrIdCollision
            | Self::ControllingSystemUnreachable
            | Self::StoredQueryHeld
            | Self::SubjectSeveral
            | Self::EhrIdHeld => StatusCode::CONFLICT,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::NodeTimeout | Self::NodeUnreachable => StatusCode::GATEWAY_TIMEOUT,
            Self::NodeRefused | Self::NodeError | Self::ResolutionUnavailable => {
                StatusCode::FAILED_DEPENDENCY
            }
            Self::MediaTypeNotAcceptable => StatusCode::NOT_ACCEPTABLE,
            Self::MediaTypeUnsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }

    /// The fixed `message` of a failure that has no more to say than its
    /// code.
    #[must_use]
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
        }
    }
}

/// The member of the ITS-REST `Error` that carries the stable [`Code`].
pub const CODE_MEMBER: &str = "code";

/// The member of the ITS-REST `Error` that carries the request id, so a client
/// and an operator name the same request.
pub const REQUEST_ID_MEMBER: &str = "request_id";

/// The ITS-REST `Error` of `code`, with `message` and `request_id`.
///
/// The code and the request id ride in the generated type's
/// `additional_properties`, which the open ITS-REST `Error` schema admits.
/// The body never carries the request's path, query, headers or body, any of
/// which may carry a patient identifier (§5.4.3).
#[must_use]
pub fn body(code: Code, message: impl Into<String>, request_id: &str) -> Error {
    let mut error = Error {
        message: message.into(),
        validation_errors: Vec::new(),
        additional_properties: BTreeMap::new(),
    };
    error
        .additional_properties
        .insert(CODE_MEMBER.to_owned(), code.as_str().into());
    error
        .additional_properties
        .insert(REQUEST_ID_MEMBER.to_owned(), request_id.into());
    error
}

/// Answers `code` with its status and the ITS-REST `Error` carrying `message`
/// and `request_id`.
#[must_use]
pub fn response(code: Code, message: impl Into<String>, request_id: &str) -> Response {
    (code.status(), Json(body(code, message, request_id))).into_response()
}

/// Answers `code` with its status and its fixed [`Code::message`].
#[must_use]
pub fn fixed(code: Code, request_id: &str) -> Response {
    response(code, code.message(), request_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{CODE_MEMBER, Code, Error, REQUEST_ID_MEMBER, RefusalCode, body};
    use http::StatusCode;
    use openehr_federation::aql::refusal::Refusal;

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

    #[test]
    fn the_body_is_the_its_rest_error_with_the_code_and_the_request_id() {
        let answer = body(Code::EhrIdCollision, "two claimants", "corr-1");
        assert_eq!(
            r#"{"message":"two claimants","validationErrors":[],"code":"ehr-id-collision","request_id":"corr-1"}"#,
            serde_json::to_string(&answer).unwrap()
        );
    }

    #[test]
    fn the_body_round_trips_through_the_generated_error_with_both_members() {
        let sent = body(Code::NoDestination, "no member in scope", "corr-2");
        let text = serde_json::to_string(&sent).unwrap();
        let read: Error = serde_json::from_str(&text).unwrap();
        assert_eq!("no member in scope", read.message);
        assert!(read.validation_errors.is_empty());
        let member = |name: &str| {
            read.additional_properties
                .get(name)
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        };
        assert_eq!(Some("no-destination".to_owned()), member(CODE_MEMBER));
        assert_eq!(Some("corr-2".to_owned()), member(REQUEST_ID_MEMBER));
        assert_eq!(2, read.additional_properties.len(), "nothing else is added");
        assert_eq!(
            text,
            serde_json::to_string(&read).unwrap(),
            "byte-identical"
        );
    }
}
