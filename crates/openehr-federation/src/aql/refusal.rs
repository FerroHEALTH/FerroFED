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
    /// other than a string literal: `OBJECT_ID.value`, `PARTY_REF.namespace`
    /// and the `id`, `issuer` and `type` of a `DV_IDENTIFIER` are `String`, and
    /// an AQL parameter is typed as the literal it stands for (AQL
    /// §Parameters).
    #[error("the patient identifier and its namespace are strings, and this operand is not one{}", At(.at))]
    IdentifierNotString {
        /// Where the comparison was written.
        at: Option<Range<usize>>,
    },
    /// The query names the patient twice with different values (§7.1, the
    /// reduction constraint).
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
    /// default (§5.2 requires the namespace; no specification governs the
    /// default: our own design).
    #[error(
        "the patient identifier carries no issuing namespace, and no default namespace is configured (§5.2)"
    )]
    NoNamespace,
    /// A subject path is selected in a form the gateway cannot re-inject:
    /// only `…/external_ref/id/value` and `…/external_ref/namespace` are the
    /// resolution input, and a returned subject column must be that input
    /// (N5, §7.1). An `ENTRY`-level subject column is never re-injected,
    /// because the row may be about a relative (`PARTY_RELATED`, §5.4.2).
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
    /// node could compute the identifier from it (§5.4.1 "in any position",
    /// §5.4.2).
    #[error(
        "a string function over a literal is compared with an identifier path and cannot be folded at the gateway, so the identifier could be rebuilt at the node (§5.4.1){}",
        At(.at)
    )]
    UnfoldableFunction {
        /// Where the comparison was written.
        at: Option<Range<usize>>,
    },
    /// An undirected aggregate whose function the deployment does not declare
    /// decomposable (N14, N39, §11.6.3). Pin the query to one node (§8), or
    /// select the rows and aggregate in the application.
    #[error(
        "an aggregate cannot be computed correctly across nodes; direct the query to one node, or select the rows and aggregate them (N14, §11.6.3){}",
        At(.at)
    )]
    UndirectedAggregate {
        /// Where the aggregate was written.
        at: Option<Range<usize>>,
    },
    /// An undirected aggregate whose function is declared decomposable, in a
    /// query that breaks the decomposition (§11.6.3, N39). Pin the query to
    /// one node (§8), or select the rows and aggregate in the application.
    #[error(
        "this aggregate cannot be recombined exactly across nodes: {reason}; direct the query to one node, or select the rows and aggregate them (N14, §11.6.3){}",
        At(.at)
    )]
    Indecomposable {
        /// What breaks the decomposition.
        reason: Indecomposable,
        /// Where the offending part was written.
        at: Option<Range<usize>>,
    },
    /// Best-effort completion was requested for an aggregate recombined
    /// across nodes: a recombined aggregate is exactly correct only over the
    /// answer of every node in scope, so a partial one would be a wrong value
    /// (§11.6.3, §11.4).
    #[error(
        "partial completeness cannot be combined with an aggregate recombined across nodes, which is exactly correct only over every node (§11.6.3, §11.4); send the query without partial, direct it to one node, or select the rows"
    )]
    PartialAggregate,
    /// A function AQL 1.1.0 does not define (AQL master03-syntax §Functions),
    /// in a query that would reach more than one node. The gateway cannot tell
    /// whether the function aggregates, and an aggregate fanned out answers
    /// one row per node, which §11.6.3 forbids (N14, N39). Pin the query to
    /// one node (§8), or select the rows and compute the function in the
    /// application.
    #[error(
        "the query calls a function AQL 1.1.0 does not define, which may aggregate and so cannot be shown correct across nodes; direct the query to one node, or select the rows and compute it in the application (N14, §11.6.3){}",
        At(.at)
    )]
    UndefinedFunction {
        /// Where the call was written: its first path argument, or the
        /// `WHERE` condition it sits in.
        at: Option<Range<usize>>,
    },
    /// Offset-based paging is not supported across a fan-out (§11.6.2, N39).
    #[error("offset-based paging is not supported across a fan-out (§11.6.2, N39)")]
    OffsetUnsupported,
    /// The gateway computes an `OFFSET` page from `k + n` rows per node, and
    /// this page cannot be computed that way (§11.6.2, N39).
    #[error("this OFFSET page cannot be computed across a fan-out: {reason} (§11.6.2, N39)")]
    OffsetPage {
        /// Why it cannot.
        reason: OffsetPage,
    },
    /// The query clause and the ITS-REST member of the same name page
    /// differently (ITS-REST Query API `Offset` and `Fetch`, §11.6).
    #[error("the {member} member and the query's {clause} clause disagree")]
    PagingConflict {
        /// The ITS-REST member.
        member: &'static str,
        /// The AQL clause it pages like.
        clause: &'static str,
    },
    /// An ITS-REST paging member, or a row count of the query, is negative.
    #[error("the {member} member is negative")]
    NegativePaging {
        /// The ITS-REST member, or `LIMIT` or `TOP`.
        member: &'static str,
    },
    /// The deprecated `TOP n BACKWARD` asks for the last rows of an order the
    /// gateway cannot reproduce across nodes; `ORDER BY … DESC LIMIT n` says
    /// the same thing in AQL 1.1.0, which deprecates `TOP` in favour of
    /// `LIMIT` with `ORDER BY` (AQL master03-syntax §TOP).
    #[error("TOP … BACKWARD is not supported across a fan-out; write ORDER BY … DESC LIMIT n")]
    TopBackward,
    /// The query uses the deprecated `TOP` together with a `LIMIT` clause,
    /// which AQL forbids whether or not the two counts agree (AQL
    /// master03-syntax §TOP and §LIMIT).
    #[error(
        "TOP and a LIMIT clause cannot be used in the same query (AQL §TOP, §LIMIT); write ORDER BY … LIMIT n"
    )]
    TopWithLimit,
    /// The query uses the deprecated `TOP` and the request carries the
    /// ITS-REST `fetch` member, which "cannot be combined with AQL-top"
    /// (ITS-REST Query API, Common Headers and Query Parameters).
    #[error(
        "the fetch member cannot be combined with TOP (ITS-REST Query API); write ORDER BY … LIMIT n, or send fetch alone"
    )]
    TopWithFetch,
    /// Under `DISTINCT`, an `ORDER BY` path that is not selected: the gateway
    /// cannot add it to the node query as a hidden column without changing
    /// which rows are distinct (N13; no specification governs the hidden
    /// column: our own design).
    #[error(
        "under DISTINCT, an ORDER BY path must also be selected, because adding it to the node query would change which rows are distinct (N13){}",
        At(.at)
    )]
    OrderNotSelected {
        /// Where the `ORDER BY` path was written.
        at: Option<Range<usize>>,
    },
    /// Under `DISTINCT` with `ORDER BY` and `LIMIT`, a selected function
    /// column is not fixed by the selected paths. AQL orders on paths alone
    /// (AQL master03-syntax §ORDER BY), so a node cut at its `LIMIT` may keep
    /// a different one of two distinct rows tied on every path on each
    /// repeat, and §11.6.1 requires that "repeating a query returns rows in
    /// the same order". Select the paths the function reads, or drop the
    /// `LIMIT`.
    #[error(
        "under DISTINCT with ORDER BY and LIMIT, a selected function column must be computed from selected paths only, because a node can order only on paths and could otherwise cut among distinct rows differently on each repeat (§11.6.1, AQL §ORDER BY); select the paths it reads, or drop the LIMIT{}",
        At(.at)
    )]
    UnorderedDistinctCut {
        /// Where the function column was written: its first path argument.
        at: Option<Range<usize>>,
    },
    /// The query names no patient and no node set, and the deployment
    /// localizes on the patient (N4). Name the endpoints with
    /// the directive or the `openEHR-federation-endpoint` header.
    #[error(
        "the query names no patient and no endpoints, so no node set is defined; name the endpoints the query is for (N4, §8)"
    )]
    NodeSetUndefined,
    /// The variable of the `FROM ENDPOINT` directive is bound again in
    /// `FROM`, or used anywhere but as a selected column. A path through it
    /// selects an ENDPOINT attribute the Tier adds to the rows (§9.3, N12);
    /// §8.1 and §9.3 define no other use, and a node, which never sees the
    /// directive, would receive an unbound variable.
    #[error(
        "the endpoint directive's variable can only be selected, as an ENDPOINT attribute, and cannot be bound again in FROM (§8.1, §9.3){}",
        At(.at)
    )]
    EndpointVariable {
        /// Where the variable was used or bound.
        at: Option<Range<usize>>,
    },
}

impl Refusal {
    /// Every name [`Refusal::kind`] returns, one per variant, in declaration
    /// order.
    ///
    /// The names are API: a gateway answers them as the stable code of its
    /// error body, so a name is only ever added, never renamed or removed.
    pub const KINDS: &'static [&'static str] = &[
        "not-aql",
        "parameters",
        "unreducible",
        "identifier-not-string",
        "second-subject",
        "second-namespace",
        "empty-identifier",
        "no-namespace",
        "subject-projection",
        "subject-without-predicate",
        "subject-ordering",
        "identifier-elsewhere",
        "unfoldable-function",
        "undirected-aggregate",
        "indecomposable-aggregate",
        "partial-aggregate",
        "undefined-function",
        "offset-unsupported",
        "offset-page",
        "paging-conflict",
        "negative-paging",
        "top-backward",
        "top-with-limit",
        "top-with-fetch",
        "order-not-selected",
        "unordered-distinct-cut",
        "node-set-undefined",
        "endpoint-variable",
    ];

    /// A stable name for this refusal: the code a gateway's error body
    /// carries, and the name a security event records the refusal by, so
    /// neither quotes any of the query (§5.4.3).
    ///
    /// Every name is in [`Refusal::KINDS`].
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotAql { .. } => "not-aql",
            Self::Parameters(_) => "parameters",
            Self::Unreducible { .. } => "unreducible",
            Self::IdentifierNotString { .. } => "identifier-not-string",
            Self::SecondSubject { .. } => "second-subject",
            Self::SecondNamespace { .. } => "second-namespace",
            Self::EmptyIdentifier { .. } => "empty-identifier",
            Self::NoNamespace => "no-namespace",
            Self::SubjectProjection { .. } => "subject-projection",
            Self::SubjectWithoutPredicate { .. } => "subject-without-predicate",
            Self::SubjectOrdering { .. } => "subject-ordering",
            Self::IdentifierElsewhere { .. } => "identifier-elsewhere",
            Self::UnfoldableFunction { .. } => "unfoldable-function",
            Self::UndirectedAggregate { .. } => "undirected-aggregate",
            Self::Indecomposable { .. } => "indecomposable-aggregate",
            Self::PartialAggregate => "partial-aggregate",
            Self::UndefinedFunction { .. } => "undefined-function",
            Self::OffsetUnsupported => "offset-unsupported",
            Self::OffsetPage { .. } => "offset-page",
            Self::PagingConflict { .. } => "paging-conflict",
            Self::NegativePaging { .. } => "negative-paging",
            Self::TopBackward => "top-backward",
            Self::TopWithLimit => "top-with-limit",
            Self::TopWithFetch => "top-with-fetch",
            Self::OrderNotSelected { .. } => "order-not-selected",
            Self::UnorderedDistinctCut { .. } => "unordered-distinct-cut",
            Self::NodeSetUndefined => "node-set-undefined",
            Self::EndpointVariable { .. } => "endpoint-variable",
        }
    }

    /// The byte range of the query the refusal points at, when it points at
    /// one: the position, never the text written there (§5.4.3).
    #[must_use]
    pub fn at(&self) -> Option<&Range<usize>> {
        match self {
            Self::NotAql { at }
            | Self::Unreducible { at, .. }
            | Self::IdentifierNotString { at }
            | Self::SecondSubject { at }
            | Self::SecondNamespace { at }
            | Self::EmptyIdentifier { at }
            | Self::SubjectProjection { at }
            | Self::SubjectWithoutPredicate { at }
            | Self::SubjectOrdering { at }
            | Self::IdentifierElsewhere { at }
            | Self::UnfoldableFunction { at }
            | Self::UndirectedAggregate { at }
            | Self::Indecomposable { at, .. }
            | Self::UndefinedFunction { at }
            | Self::OrderNotSelected { at }
            | Self::UnorderedDistinctCut { at }
            | Self::EndpointVariable { at } => at.as_ref(),
            Self::Parameters(_)
            | Self::NoNamespace
            | Self::PartialAggregate
            | Self::OffsetUnsupported
            | Self::OffsetPage { .. }
            | Self::PagingConflict { .. }
            | Self::NegativePaging { .. }
            | Self::TopBackward
            | Self::TopWithLimit
            | Self::TopWithFetch
            | Self::NodeSetUndefined => None,
        }
    }
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
    /// It is a path into a patient carrier the gateway does not consume:
    /// another attribute of `EHR_STATUS.subject` or of an `ENTRY`-level
    /// `subject`, a predicate on the carrier, or a root that cannot carry it.
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
                "the path is not one the gateway resolves on: ehr_status/subject/external_ref/id/value or its namespace on the EHR variable, or an ENTRY-level subject/identifiers/id, issuer or type"
            }
            Self::SeveralEhrs => "the FROM clause binds more than one EHR",
            Self::InsideAnExpression => "it sits inside a function call or an aggregate",
        })
    }
}

/// What keeps a declared decomposable aggregate from being recombined exactly.
///
/// "The aggregate must not be combined with `DISTINCT`, `GROUP BY` on a
/// dimension that spans nodes, or de-duplication (§10), any of which breaks
/// decomposability" (§11.6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Indecomposable {
    /// The query selects `DISTINCT`.
    Distinct,
    /// `COUNT(DISTINCT …)`: a value counted at two nodes is one value, so the
    /// distinct count is not the sum of the node counts.
    CountDistinct,
    /// A column that is not an aggregate is selected beside the aggregates,
    /// which would group the rows by it, and AQL 1.1.0 has no `GROUP BY` to
    /// merge such groups by.
    PlainColumn,
    /// The request selects a dedup mode (§10): a node's aggregate already
    /// counts the copies the Tier would suppress, and its one row does not say
    /// which they are.
    Dedup,
}

impl fmt::Display for Indecomposable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Distinct => "the query selects DISTINCT",
            Self::CountDistinct => {
                "COUNT(DISTINCT …) is not the sum of the node counts, because one value can be counted at two nodes"
            }
            Self::PlainColumn => "a column that is not an aggregate is selected beside it",
            Self::Dedup => {
                "the request selects de-duplication (§10), and a node's aggregate includes the copies it would suppress"
            }
        })
    }
}

/// Why an `OFFSET` page cannot be computed from `k + n` rows per node: the
/// strategy is "permitted only where the gateway can bound `k + n` (it MUST
/// reject when it cannot)" (§11.6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OffsetPage {
    /// `k + n` is past the rows the gateway asks of one node.
    PastTheBound {
        /// The most rows the gateway asks of one node for a page.
        max_window: u32,
    },
    /// The query has no `LIMIT`, so `k + n` has no bound.
    NoLimit,
    /// The query has no `ORDER BY`, so its rows have no order across nodes
    /// to page through.
    NoOrder,
}

impl fmt::Display for OffsetPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PastTheBound { max_window } => write!(
                f,
                "OFFSET plus LIMIT is past this gateway's bound of {max_window} rows per node"
            ),
            Self::NoLimit => f.write_str("OFFSET without LIMIT has no bound"),
            Self::NoOrder => {
                f.write_str("OFFSET without ORDER BY has no order across nodes to page through")
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{Indecomposable, OffsetPage, Refusal, Unreducible};
    use openehr_query::bind::BindError;

    /// One refusal of every variant, in declaration order.
    ///
    /// The `match` in [`ordinal`] has no wildcard, so a new variant fails to
    /// compile there until it is listed, and the test below then holds
    /// [`Refusal::KINDS`] to it.
    fn every() -> Vec<Refusal> {
        vec![
            Refusal::NotAql { at: None },
            Refusal::Parameters(BindError { faults: Vec::new() }),
            Refusal::Unreducible {
                reason: Unreducible::NotEquality,
                at: None,
            },
            Refusal::IdentifierNotString { at: None },
            Refusal::SecondSubject { at: None },
            Refusal::SecondNamespace { at: None },
            Refusal::EmptyIdentifier { at: None },
            Refusal::NoNamespace,
            Refusal::SubjectProjection { at: None },
            Refusal::SubjectWithoutPredicate { at: None },
            Refusal::SubjectOrdering { at: None },
            Refusal::IdentifierElsewhere { at: None },
            Refusal::UnfoldableFunction { at: None },
            Refusal::UndirectedAggregate { at: None },
            Refusal::Indecomposable {
                reason: Indecomposable::Distinct,
                at: None,
            },
            Refusal::PartialAggregate,
            Refusal::UndefinedFunction { at: None },
            Refusal::OffsetUnsupported,
            Refusal::OffsetPage {
                reason: OffsetPage::NoLimit,
            },
            Refusal::PagingConflict {
                member: "fetch",
                clause: "LIMIT",
            },
            Refusal::NegativePaging { member: "offset" },
            Refusal::TopBackward,
            Refusal::TopWithLimit,
            Refusal::TopWithFetch,
            Refusal::OrderNotSelected { at: None },
            Refusal::UnorderedDistinctCut { at: None },
            Refusal::NodeSetUndefined,
            Refusal::EndpointVariable { at: None },
        ]
    }

    /// The declaration position of `refusal`'s variant.
    fn ordinal(refusal: &Refusal) -> usize {
        match refusal {
            Refusal::NotAql { .. } => 0,
            Refusal::Parameters(_) => 1,
            Refusal::Unreducible { .. } => 2,
            Refusal::IdentifierNotString { .. } => 3,
            Refusal::SecondSubject { .. } => 4,
            Refusal::SecondNamespace { .. } => 5,
            Refusal::EmptyIdentifier { .. } => 6,
            Refusal::NoNamespace => 7,
            Refusal::SubjectProjection { .. } => 8,
            Refusal::SubjectWithoutPredicate { .. } => 9,
            Refusal::SubjectOrdering { .. } => 10,
            Refusal::IdentifierElsewhere { .. } => 11,
            Refusal::UnfoldableFunction { .. } => 12,
            Refusal::UndirectedAggregate { .. } => 13,
            Refusal::Indecomposable { .. } => 14,
            Refusal::PartialAggregate => 15,
            Refusal::UndefinedFunction { .. } => 16,
            Refusal::OffsetUnsupported => 17,
            Refusal::OffsetPage { .. } => 18,
            Refusal::PagingConflict { .. } => 19,
            Refusal::NegativePaging { .. } => 20,
            Refusal::TopBackward => 21,
            Refusal::TopWithLimit => 22,
            Refusal::TopWithFetch => 23,
            Refusal::OrderNotSelected { .. } => 24,
            Refusal::UnorderedDistinctCut { .. } => 25,
            Refusal::NodeSetUndefined => 26,
            Refusal::EndpointVariable { .. } => 27,
        }
    }

    #[test]
    fn kinds_names_every_variant_once_in_declaration_order() {
        let every = every();
        let ordinals: Vec<usize> = every.iter().map(ordinal).collect();
        assert_eq!(
            (0..Refusal::KINDS.len()).collect::<Vec<_>>(),
            ordinals,
            "every() lists each variant once, in order"
        );
        let kinds: Vec<&str> = every.iter().map(Refusal::kind).collect();
        assert_eq!(Refusal::KINDS, kinds.as_slice());
    }

    #[test]
    fn every_kind_is_lower_kebab_case() {
        for kind in Refusal::KINDS {
            assert!(
                !kind.is_empty()
                    && !kind.starts_with('-')
                    && !kind.ends_with('-')
                    && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{kind}"
            );
        }
    }
}
