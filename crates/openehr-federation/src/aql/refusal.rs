// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a façade query is refused before anything is dispatched.
//!
//! Every refusal is an HTTP `400`: the specification refuses the query rather
//! than guessing (§5.4.1, §7.1, §11.6). A refusal locates the offending part
//! of the query by the byte range it was written at, and never quotes it: the
//! text there may be the patient identifier, and a refusal travels into logs
//! and HTTP responses (§5.4.3).

use std::fmt;
use std::ops::Range;

use openehr_query::bind::BindError;
use thiserror::Error;

/// Why a façade query is refused. Every variant answers `400`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The query is not AQL 1.1.0.
    ///
    /// The parser's own message is not kept, because it quotes the token it
    /// stopped at, and that token may be the identifier.
    #[error("the query is not AQL 1.1.0{}", At(.at))]
    NotAql {
        /// The first position the parser refused, when it reported one.
        at: Option<Range<usize>>,
    },
    /// `query_parameters` could not be bound into the query. The cause names
    /// each parameter and never its value.
    #[error("the query parameters cannot be bound: {0}")]
    Parameters(#[source] BindError),
    /// A patient predicate cannot be consumed exactly, so no single `ehr_id`
    /// scope per node covers the query (§7.1 reduction constraint, §5.4.3).
    #[error(
        "the patient predicate cannot be reduced to one ehr_id scope per node: {reason} (§7.1, §5.4.3){}",
        At(.at)
    )]
    Unreducible {
        /// Why it cannot.
        reason: Unreducible,
        /// Where the predicate was written.
        at: Option<Range<usize>>,
    },
    /// The patient identifier, or its namespace, is compared with something
    /// other than a string literal: `OBJECT_ID.value` and `PARTY_REF.namespace`
    /// are `String`, and an AQL parameter is typed as the literal it stands
    /// for (decision A6; AQL §Parameters).
    #[error("the patient identifier and its namespace are strings, and this operand is not one{}", At(.at))]
    IdentifierNotString {
        /// Where the comparison was written.
        at: Option<Range<usize>>,
    },
    /// The query names the patient twice with different values (decision A7,
    /// §7.1).
    #[error("the query names two different patient identifiers (§7.1){}", At(.at))]
    SecondSubject {
        /// Where the second value was written.
        at: Option<Range<usize>>,
    },
    /// The query names two different issuing namespaces for the patient.
    #[error("the query names two different issuing namespaces for the patient (§5.2, §7.1){}", At(.at))]
    SecondNamespace {
        /// Where the second namespace was written.
        at: Option<Range<usize>>,
    },
    /// The patient identifier is empty.
    #[error("the patient identifier is empty (§5.2){}", At(.at))]
    EmptyIdentifier {
        /// Where it was written.
        at: Option<Range<usize>>,
    },
    /// The query names no issuing namespace and the deployment declares no
    /// default (§5.2; decision A5).
    #[error(
        "the patient identifier carries no issuing namespace, and no default namespace is configured (§5.2)"
    )]
    NoNamespace,
    /// A subject path is selected in a form the gateway cannot re-inject:
    /// only `…/external_ref/id/value` and `…/external_ref/namespace` are the
    /// resolution input, and a returned subject column must be that input
    /// (N5, §7.1).
    #[error("a selected subject column must be the re-injected resolution input, and this path is not one (N5){}", At(.at))]
    SubjectProjection {
        /// Where the column was written.
        at: Option<Range<usize>>,
    },
    /// The subject column is selected but the query has no patient predicate,
    /// so there is no resolution input to re-inject (N5).
    #[error("the subject column is selected, but the query names no patient to re-inject (N5){}", At(.at))]
    SubjectWithoutPredicate {
        /// Where the column was written.
        at: Option<Range<usize>>,
    },
    /// A subject path appears in `ORDER BY`, which would carry it to the node
    /// (§5.4.2).
    #[error("a subject path cannot be ordered on, since ORDER BY is dispatched to the node (§5.4.2){}", At(.at))]
    SubjectOrdering {
        /// Where the ordering term was written.
        at: Option<Range<usize>>,
    },
    /// The patient identifier appears in another position of the query, which
    /// would carry it to the node (§5.4.1, N33).
    #[error("the patient identifier appears elsewhere in the query, and no node may receive it (§5.4.1, N33){}", At(.at))]
    IdentifierElsewhere {
        /// Where the value was found.
        at: Option<Range<usize>>,
    },
    /// A string function over a literal is compared with an
    /// identifier-bearing path and cannot be folded at the gateway, so the
    /// node could compute the identifier from it (§5.4.1, §5.4.2; decision
    /// A4).
    #[error(
        "a string function over a literal is compared with an identifier path and cannot be folded at the gateway, so the identifier could be rebuilt at the node (§5.4.1){}",
        At(.at)
    )]
    UnfoldableFunction {
        /// Where the comparison was written.
        at: Option<Range<usize>>,
    },
    /// The query reaches an `ENTRY`-level subject identifier, which this
    /// release does not yet consume as resolution input (§5.4.3, N33).
    ///
    /// Refusing it is the reading that cannot leak until that carrier lands
    /// as resolution input.
    #[error("the ENTRY-level subject carrier is not yet accepted as resolution input (§5.4.3){}", At(.at))]
    EntrySubject {
        /// Where the path was written.
        at: Option<Range<usize>>,
    },
    /// An aggregate the gateway cannot compute correctly across a fan-out
    /// (N14, N39, §11.6.3). Pin the query to one node (§8), or select the rows
    /// and aggregate in the application.
    #[error(
        "an aggregate cannot be computed correctly across nodes; direct the query to one node, or select the rows and aggregate them (N14, §11.6.3){}",
        At(.at)
    )]
    UndirectedAggregate {
        /// Where the aggregate was written.
        at: Option<Range<usize>>,
    },
    /// Offset-based paging is not supported across a fan-out (§11.6.2, N39).
    #[error("offset-based paging is not supported across a fan-out (§11.6.2, N39)")]
    OffsetUnsupported,
    /// The query clause and the ITS-REST member of the same name page
    /// differently (decision A10).
    #[error("the {member} member and the query's {clause} clause disagree")]
    PagingConflict {
        /// The ITS-REST member.
        member: &'static str,
        /// The AQL clause it pages like.
        clause: &'static str,
    },
    /// An ITS-REST paging member is negative.
    #[error("the {member} member is negative")]
    NegativePaging {
        /// The ITS-REST member.
        member: &'static str,
    },
    /// The query names no patient and no node set, and the deployment
    /// localizes on the patient (N4; decision A8). Name the endpoints with
    /// the directive or the `openEHR-federation-endpoint` header.
    #[error(
        "the query names no patient and no endpoints, so no node set is defined; name the endpoints the query is for (N4, §8)"
    )]
    NodeSetUndefined,
}

/// Why a patient predicate cannot be consumed exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unreducible {
    /// It sits under `OR` or `NOT`, outside the top-level `AND` chain.
    NotConjunctive,
    /// Its operator is not `=` (`!=`, `<`, `LIKE`, `MATCHES`, `EXISTS`).
    NotEquality,
    /// Its operand is not a literal (a path or a function call).
    NotALiteral,
    /// It is a subject path the gateway does not consume: another attribute
    /// of `EHR_STATUS.subject`, a predicate on the path, or a root that is not
    /// an `EHR` variable.
    OtherSubjectPath,
    /// The `FROM` clause binds more than one `EHR`.
    SeveralEhrs,
    /// The subject path appears inside a function call or an aggregate.
    InsideAnExpression,
}

impl fmt::Display for Unreducible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotConjunctive => "it sits under OR or NOT, outside the top-level AND chain",
            Self::NotEquality => "its operator is not =",
            Self::NotALiteral => "its operand is not a literal",
            Self::OtherSubjectPath => {
                "the path is not ehr_status/subject/external_ref/id/value or its namespace on the EHR variable"
            }
            Self::SeveralEhrs => "the FROM clause binds more than one EHR",
            Self::InsideAnExpression => "it sits inside a function call or an aggregate",
        })
    }
}

/// The ` (bytes a..b)` suffix of a refusal, or nothing for an unknown span.
struct At<'a>(&'a Option<Range<usize>>);

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(bytes) => write!(f, " (bytes {}..{})", bytes.start, bytes.end),
            None => Ok(()),
        }
    }
}
