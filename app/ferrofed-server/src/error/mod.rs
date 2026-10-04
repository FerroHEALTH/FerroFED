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

mod code;

use std::collections::BTreeMap;

use axum::Json;
use axum::response::{IntoResponse, Response};
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
    /// A stored-query `PUT` at a read-only registry, whose definitions are
    /// loaded from files at start; nothing is stored (§12.7, N44).
    StoredQueryReadOnly,
    /// The consent pre-filter does not permit asking the members it names
    /// about the subject of `GET {base}/v1/ehr`, and no other member holds
    /// an EHR for it (N27a, §13.2.1). The body names the endpoints, never the
    /// subject.
    ConsentDenied,
    /// The request carries no access token this gateway accepts (§13.1, N25).
    Unauthenticated,
    /// No scope the access token grants covers the operation (RFC 6750 §3.1).
    ScopeInsufficient,
    /// The access token carries no purpose of use, which is required (§13.4).
    PurposeOfUseRequired,
    /// The issuer's key set or introspection endpoint cannot be had (§13.1).
    AuthenticationUnavailable,
    /// No caller is admitted to the operation: an admin or an unknown one.
    OperationRefused,
    /// The configured localizer could not answer while localizing the
    /// subject of `GET {base}/v1/ehr`, and the deployment fails closed, so
    /// no member was asked and whether the subject has an EHR is unknown
    /// (§14.1, N4). The body never names the subject.
    LocalizationUnavailable,
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
    use super::{CODE_MEMBER, Code, Error, REQUEST_ID_MEMBER, body};

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
