// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error vocabulary: every condition the gateway answers with an error
//! body, its HTTP status (§11.2) and the stable code the body names.
//!
//! An error body is an RFC 9457 problem details document
//! (<https://www.rfc-editor.org/rfc/rfc9457>), `application/problem+json`,
//! written through `ferrohealth_server::problem`. Each [`Code`] is a problem
//! type defined once in [`types`], under [`BASE`], the book page that lists
//! every code (RFC 9457 §4): the `type` of `no-destination` is
//! `{BASE}no-destination`, its `title` the code's fixed message. The
//! `detail` is the sentence of this occurrence, and two extension members
//! (§3.2) follow: `code`, the code itself, and `request_id`, the exchange id
//! of [`crate::request_id`]: the client's own when it named the request,
//! otherwise the gateway's, which the log records too and which is also the
//! `instance` as a `urn:uuid:` URI when it is a UUID. Two failures answer
//! with another body. A fan-out that fails under all-or-nothing (`504`,
//! `424`) answers the `RESULT_SET` of §11.4, whose
//! `meta.federation.endpoints[]` statuses say which node failed and why,
//! and which echoes the client's own `q` (N17, N37). A node's own answer on a
//! single-node route passes through as the node sent it (§11.2).
//!
//! The codes are API: a code is only ever added, never renamed, removed or
//! moved to another status. No error body quotes a parameter value, the query
//! text, a path or a header value (§5.4.3, RFC 9457 §5). No specification
//! defines the `code` member or its values: our own design.

mod code;

use std::sync::LazyLock;

use axum::response::{IntoResponse, Response};
use ferrohealth_server::problem::{Problem, ProblemType, ProblemTypes};
use openehr_federation::aql::refusal::Refusal;

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
    /// by where the patient resolved (§12.5.2). It answers N41's `400`, which
    /// requires an explicit target, and the body lists the endpoints.
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
    /// A stored-query `PUT` names a definition in the namespace the gateway
    /// reserves for its own read-only queries, which no `PUT` stores into;
    /// nothing is stored (§12.7, N44).
    StoredQueryReserved,
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
    /// Only a `patient/` grant covers the operation, and the access token
    /// carries no `ehrId` the grant can be confined to (SMART on openEHR,
    /// master07 §Context Selection).
    PatientContextMissing,
    /// The request reaches an EHR, a member or an area outside the patient
    /// the access token's `patient/` grant is confined to, or names no
    /// patient at all; nothing is sent (§5.2, §12.5).
    PatientConfinement,
    /// The cross-reference could not resolve the patient of the access
    /// token's `patient/` grant at every member, so the grant cannot be
    /// confined and nothing is sent (§5.2, §11.2).
    PatientContextUnavailable,
    /// No member the gateway may ask holds an EHR for the subject of
    /// `GET {base}/v1/ehr`, in a deployment that does not disclose consent
    /// exclusions, where a subject no member knows and one only a member
    /// the consent pre-filter denied holds get this one answer (Regulation
    /// (EU) 2025/327 Art 8; RFC 9110 §15.5.5). The body never names the
    /// subject or an endpoint.
    SubjectUnavailable,
    /// The gateway serves as many requests as `server.max_concurrent_requests`
    /// allows, so this one is refused before it reaches anything; the answer
    /// carries `Retry-After` (RFC 9110 §15.6.4, §10.2.3).
    Overloaded,
    /// The verified caller sent more requests than `[server.caller_rate]`
    /// allows; the answer carries `Retry-After` (RFC 6585 §4).
    RateLimited,
    /// The access the request made could not be recorded in the access log,
    /// so its answer is withheld (Regulation (EU) 2025/327 Annex II 3.2).
    AccessUnrecorded,
    /// The request reaches patient data, and the access token names no
    /// natural person: a client acts, and its issuer does not declare its
    /// client tokens as acting for the professional they name (Regulation
    /// (EU) 2025/327 Annex II 3.1; RFC 9470 §3).
    NaturalPersonRequired,
    /// The request reaches patient data, and the access token states no
    /// authentication assurance at the least level its issuer's entry
    /// requires (Regulation (EU) 2025/327 Annex II 3.1; RFC 9470 §3).
    AuthenticationAssuranceInsufficient,
    /// The request reaches patient data, and the access token of a national
    /// contact point does not carry every attribute of Implementing
    /// Regulation (EU) 2026/2099 Annex Tables 1 and 2 (Art 7).
    ContactPointAttributesRequired,
    /// The national contact point's correlation header is repeated, empty,
    /// too long, or not visible ASCII.
    CorrelationInvalid,
    /// A configuration revision is refused as `config check` refuses a
    /// configuration, or the reload refused it; nothing changes.
    ConfigurationInvalid,
    /// A configuration revision was saved, applied or rolled back from a
    /// state another operator or replica changed first.
    RevisionStale,
    /// No configuration revision has the number named.
    RevisionUnknown,
    /// The running configuration revision has no earlier one to roll back
    /// to.
    RollbackUnavailable,
    /// A change in the operator console names a resource the configuration
    /// file, its registry document or a care services directory provides,
    /// which only that source changes.
    ResourceFileOwned,
    /// A change in the operator console names a resource the newest
    /// configuration revision does not hold.
    ResourceUnknown,
    /// A configuration route on the admin listener of a gateway whose
    /// configuration is not managed (`config.ui = "off"`).
    ConfigurationUnmanaged,
    /// An apply names a configuration revision that changes a resource
    /// `config.approval` names, and no operator other than its author
    /// approved it.
    ApprovalRequired,
    /// The author of a configuration revision approves it, which only
    /// another operator may.
    ApprovalOwn,
    /// An approval names a configuration revision that changes no resource
    /// `config.approval` names.
    ApprovalNotRequired,
    /// A confirmation or an approval comes from a caller that is no verified
    /// operator, such as a loopback peer with no credential.
    OperatorUnverified,
    /// A confirmation names a configuration revision that does not run
    /// awaiting one, or whose deadline passed.
    ConfirmationUnavailable,
    /// An apply while the running configuration revision awaits its
    /// confirmation.
    ConfirmationPending,
    /// A secret value is sent while `config.secrets_dir` is unset.
    SecretStoreUnavailable,
    /// The request carries two credentials, or a credential field that is no
    /// credential (RFC 6750 §3.1, `invalid_request`).
    CredentialMalformed,
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

/// The base URI of every FerroFED problem type: the book page that lists
/// every code, one anchor per code (RFC 9457 §4).
pub const BASE: &str = "https://ferrofed.eu/docs/integrate/errors.html#";

/// The extension member of a problem that carries the stable [`Code`].
pub const CODE_MEMBER: &str = "code";

/// The extension member of a problem that carries the request id, so a
/// client and an operator name the same request.
pub const REQUEST_ID_MEMBER: &str = "request_id";

/// The registry of every [`Code`]'s problem type.
static TYPES: LazyLock<ProblemTypes> = LazyLock::new(registry);

/// Builds the registry: one type per code, under its slug, titled with its
/// fixed message and used with its status.
#[expect(
    clippy::expect_used,
    reason = "BASE and every code are constants the unit tests define the registry from"
)]
fn registry() -> ProblemTypes {
    let mut types = ProblemTypes::new(BASE).expect("the problem base should be a legal base");
    for code in Code::every() {
        types
            .define(code.as_str(), code.message(), code.status())
            .expect("every code should be one legal, distinct problem type slug");
    }
    types
}

/// FerroFED's problem types, in slug order.
#[must_use]
pub fn types() -> &'static ProblemTypes {
    &TYPES
}

impl Code {
    /// The problem type of this code, as [`types`] defines it.
    ///
    /// # Panics
    /// Never: the registry defines every code, which a unit test holds.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "registry() defines every code of Code::every, which every_code_is_one_problem_type checks"
    )]
    pub fn problem_type(self) -> &'static ProblemType {
        types()
            .get(self.as_str())
            .expect("every code should be defined in the registry")
    }
}

/// The problem of `code`, with `message` as its `detail` and `request_id`
/// as its `request_id` member and, when it is a UUID, its `instance`.
///
/// The body never carries the request's path, query, headers or body, any of
/// which may carry a patient identifier (§5.4.3, RFC 9457 §5).
#[must_use]
pub fn body(code: Code, message: impl Into<String>, request_id: &str) -> Problem {
    let problem = code
        .problem_type()
        .problem()
        .with_detail(message)
        .with_request_id(request_id);
    extended(
        extended(problem, CODE_MEMBER, code.as_str()),
        REQUEST_ID_MEMBER,
        request_id,
    )
}

/// `problem` with the text extension member `name` set to `value`.
#[expect(
    clippy::expect_used,
    reason = "name is CODE_MEMBER or REQUEST_ID_MEMBER, legal names a unit test adds, and text always serializes"
)]
fn extended(problem: Problem, name: &str, value: &str) -> Problem {
    problem
        .with_extension(name, value)
        .expect("a legal member name with a text value should be added")
}

/// Answers `code` with its status and its problem carrying `message` and
/// `request_id`.
#[must_use]
pub fn response(code: Code, message: impl Into<String>, request_id: &str) -> Response {
    body(code, message, request_id).into_response()
}

/// Answers `code` with its status and its fixed [`Code::message`].
#[must_use]
pub fn fixed(code: Code, request_id: &str) -> Response {
    response(code, code.message(), request_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use http::{StatusCode, header};

    use super::{BASE, Code, body, types};

    #[test]
    fn every_code_is_one_problem_type_under_the_book_page() {
        let mut slugs = BTreeSet::new();
        for code in Code::every() {
            let defined = code.problem_type();
            assert_eq!(format!("{BASE}{}", code.as_str()), defined.uri());
            assert_eq!(Some(code.message()), defined.title());
            assert_eq!(code.status(), defined.status());
            assert!(slugs.insert(code.as_str()), "{} twice", code.as_str());
        }
        assert_eq!(slugs.len(), types().iter().count());
    }

    #[test]
    fn the_body_is_the_problem_with_the_code_and_the_request_id() {
        let answer = body(
            Code::EhrIdCollision,
            "two claimants",
            "6f1c0b9e-3d1a-4c55-9a43-0d8f1f2b7c11",
        );
        assert_eq!(
            r#"{"type":"https://ferrofed.eu/docs/integrate/errors.html#ehr-id-collision","title":"the ehr_id is claimed by more than one node (N42)","status":409,"detail":"two claimants","instance":"urn:uuid:6f1c0b9e-3d1a-4c55-9a43-0d8f1f2b7c11","code":"ehr-id-collision","request_id":"6f1c0b9e-3d1a-4c55-9a43-0d8f1f2b7c11"}"#,
            serde_json::to_string(&answer).unwrap()
        );
    }

    #[test]
    fn a_request_id_that_is_no_uuid_is_a_member_and_no_instance() {
        let answer = serde_json::to_value(body(Code::NoDestination, "none", "corr-2")).unwrap();
        assert_eq!(Some("corr-2"), answer["request_id"].as_str());
        assert!(answer.get("instance").is_none(), "{answer}");
    }

    #[test]
    fn a_problem_answers_its_status_as_problem_json() {
        let response = super::fixed(Code::NotFound, "corr-3");
        assert_eq!(StatusCode::NOT_FOUND, response.status());
        assert_eq!(
            Some("application/problem+json"),
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
        );
    }
}
