// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one error type of the crate: why a federation wire value was refused.

use thiserror::Error;

use crate::status::EndpointStatus;

/// Why a federation wire value was refused, at construction or while reading.
///
/// Every variant names the object and the member it concerns. A nested failure
/// is part of the message, because a reader reports it through serde, which
/// keeps only the text; a JSON parse failure quotes what `serde_json` quotes.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WireError {
    /// A member the specification requires is absent.
    #[error("`{object}` has no `{member}`, which the specification requires")]
    MissingMember {
        /// The object the member belongs to.
        object: &'static str,
        /// The required member.
        member: &'static str,
    },
    /// A member is present with a value the reader refuses.
    #[error("`{object}.{member}` is malformed: {source}")]
    MalformedMember {
        /// The object the member belongs to.
        object: &'static str,
        /// The malformed member.
        member: &'static str,
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// A member appears twice in one JSON object, which leaves its value
    /// ambiguous.
    #[error("`{object}` carries `{member}` more than once")]
    DuplicateMember {
        /// The object the member belongs to.
        object: &'static str,
        /// The repeated member name.
        member: String,
    },
    /// A member the specification requires to be non-empty is empty.
    #[error("`{object}.{member}` is empty, and the specification requires a value")]
    EmptyMember {
        /// The object the member belongs to.
        object: &'static str,
        /// The empty member.
        member: &'static str,
    },
    /// A member is not an absolute URI (`format: uri`).
    #[error("`{object}.{member}` is not an absolute URI")]
    InvalidUri {
        /// The object the member belongs to.
        object: &'static str,
        /// The member that should hold a URI.
        member: &'static str,
        /// The parse failure.
        #[source]
        source: url::ParseError,
    },
    /// An endpoint outcome lacks a member its status requires (§9.5, N40).
    #[error("an endpoint reported `{status}` must carry `{member}` (§9.5, N40)")]
    StatusRequires {
        /// The reported status.
        status: EndpointStatus,
        /// The member that status requires.
        member: &'static str,
    },
    /// An endpoint outcome carries a member its status rules out (§9.5,
    /// §11.1, N40).
    #[error("an endpoint reported `{status}` must not carry `{member}` (§9.5, §11.1, N40)")]
    StatusForbids {
        /// The reported status.
        status: EndpointStatus,
        /// The member that status rules out.
        member: &'static str,
    },
    /// An endpoint that did not reach `active` reports contributed rows
    /// (§11.1: an unresponsive node contributes none).
    #[error(
        "an endpoint reported `{status}` contributed no rows, so `row_count` must be 0 (§11.1)"
    )]
    RowsWithoutAnswer {
        /// The reported status.
        status: EndpointStatus,
    },
    /// Two outcomes in one `meta.federation.endpoints[]` name the same
    /// endpoint (N16: every in-scope node appears with a status).
    #[error("`meta.federation.endpoints[]` reports endpoint `{id}` more than once (N16)")]
    DuplicateEndpoint {
        /// The repeated endpoint id, a registry identifier.
        id: String,
    },
    /// A declared `complete` disagrees with the reported statuses (§11.4,
    /// N37).
    #[error(
        "`meta.federation.complete` is `{declared}`, but the reported statuses make it `{derived}` (§11.4, N37)"
    )]
    CompleteMismatch {
        /// The value the envelope declared.
        declared: bool,
        /// The value the statuses imply.
        derived: bool,
    },
    /// A federation member sits flat on `meta` or under a reserved
    /// `_`-prefixed name (§9.1, N17, CP-35).
    #[error(
        "`meta.{member}` is not conformant: federation additions live only under `meta.federation` (§9.1, CP-35)"
    )]
    FlatFederationMember {
        /// The offending `meta` member name.
        member: String,
    },
    /// An unmodelled member uses the name of a member the type models, so
    /// the object would carry that name twice.
    #[error(
        "`{object}` cannot carry an extra member named `{member}`: the name is a modelled member"
    )]
    ExtraShadowsMember {
        /// The object the member belongs to.
        object: &'static str,
        /// The colliding member name.
        member: String,
    },
    /// A `spec_version` is not in `major.minor` form (§7a.2).
    #[error("`federation.spec_version` must be `major.minor` (§7a.2)")]
    SpecVersion,
    /// A membership status uses the per-query vocabulary of §11.1, which is
    /// meaningless as a standing membership status (§7a.2).
    #[error("`not-localized` is a per-query outcome and cannot be a membership status (§7a.2)")]
    MembershipNotLocalized,
    /// The DEMOGRAPHIC area is declared federated (N32).
    #[error("`its_rest.demographic` cannot be `federated` (N32)")]
    FederatedDemographic,
    /// Best-effort completeness is offered without saying how a request
    /// selects it (N37, §11.4).
    #[error(
        "`completeness.best_effort` is true, so `completeness.opt_in` is required (N37, §11.4)"
    )]
    BestEffortWithoutOptIn,
    /// Stored-query fan-out is declared without the stored-query registry
    /// (§12.7, N44).
    #[error(
        "`definition.stored_query_fan_out` requires `stored_query_registry: true` (§12.7, N44)"
    )]
    FanOutWithoutRegistry,
    /// The dedup policy lists no modes (§10, N15).
    #[error("`dedup.modes` must list at least one mode (§10, N15)")]
    NoDedupModes,
    /// The dedup default is not `none` (N15).
    #[error("`dedup.default` must be `none` (N15)")]
    DedupDefaultNotNone,
    /// The completeness default is not `all-or-nothing` (N37).
    #[error("`completeness.default` must be `all-or-nothing` (N37)")]
    CompletenessDefault,
    /// The envelope could not be written into or read out of the ITS-REST
    /// `ResultSetMetadata`.
    #[error("the federation envelope could not be encoded")]
    Envelope(#[source] serde_json::Error),
}
