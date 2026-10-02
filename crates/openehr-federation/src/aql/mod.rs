// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL rewrite of the Federation Tier (feature `aql`): one façade query
//! in, one standard, `ehr_id`-scoped query per node out (§7.1, N2, N7).
//!
//! [`analyse`] parses the client's query with `openehr-query`, binds the
//! ITS-REST `query_parameters` into the tree before anything reads it, finds
//! the patient on either carrier, `EHR_STATUS.subject.external_ref` or an
//! `ENTRY`-level `subject` `DV_IDENTIFIER` (§5.4.3, CP-38), and refuses with a
//! [`refusal::Refusal`] whatever cannot be consumed exactly. Both carriers
//! rewrite to the same node query. A
//! [`PatientQuery`] then prints the node query for each resolved `ehr_id` with
//! `printer::to_aql`. Every step is a transformation of the AQL syntax tree:
//! there is no AQL text splicing, and a value reaches a node query only
//! through the printer's escaping.
//!
//! The patient identifier never reaches a node query (§5.4.1, N33): the
//! predicate that carries it is replaced by `<ehr>/ehr_id/value = '<ehr_id>'`,
//! a selected subject column is re-injected after the merge instead of being
//! asked of the node (N5), and a query in which the value appears anywhere
//! else is refused, including as the folded value of `CONCAT`, `CONCAT_WS` or
//! `SUBSTRING` over literals (§5.4.1 "in any position"). The AQL release the
//! module rewrites is [`crate::AQL`].
//!
//! # Examples
//!
//! ```
//! use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
//! use openehr_federation::aql::{Analysis, ColumnSource, Context, Paging, Targeting, analyse};
//! use openehr_query::bind::Parameters;
//!
//! let context = Context::new(Targeting::AskAll).with_default_namespace("urn:oid:2.999.1");
//! let facade = "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value \
//!               FROM EHR e CONTAINS COMPOSITION c \
//!               WHERE e/ehr_status/subject/external_ref/id/value = '4711'";
//! let analysis = analyse(facade, &Parameters::new(), Paging::default(), &context)?;
//! let Analysis::Patient(patient) = analysis else { unreachable!("the query names a patient") };
//! assert_eq!(patient.subject().value(), "4711");
//!
//! let node = patient.for_node(&HierObjectId::new("7d44b88c-4199-4bad-97dc-d78268e01398")?);
//! let expected = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
//!                 WHERE e/ehr_id/value='7d44b88c-4199-4bad-97dc-d78268e01398'";
//! assert_eq!(node.aql(), expected);
//! assert_eq!(node.columns(), [ColumnSource::Subject, ColumnSource::Node(0)]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod aggregate;
mod fold;
mod function;
mod paging;
mod rewrite;
mod scan;

pub mod directive;
pub mod refusal;
pub mod subject;

use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroUsize};
use std::ops::Range;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_its::rest::generated::query::ResultSetColumn;
use openehr_query::ast::{ColumnExpr, SelectClause, SelectQuery};
use openehr_query::bind::{Parameters, bind};
use openehr_query::federation::Directive;
use openehr_query::parser::ParseError;
use openehr_query::printer::to_aql;

use crate::aggregate::{AggregateFunction, Recombination};
use crate::dedup::DedupMode;
use crate::order::ResultOrder;
use directive::FacadeQuery;
use refusal::{Refusal, Unreducible};
use scan::{Findings, Input};
use subject::{NamespaceOrigin, Subject};

/// The ITS-REST paging members sent beside the AQL (`AdhocQueryExecute`
/// `offset` and `fetch`, or the GET parameters of the same names).
///
/// They page like the AQL `OFFSET` and `LIMIT` clauses and follow the same
/// rules (§11.6 and N39 name only the clauses; no specification governs the
/// members: our own design).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Paging {
    /// The `offset` member.
    pub offset: Option<i64>,
    /// The `fetch` member.
    pub fetch: Option<i64>,
}

/// How the node set of the request is chosen (§8, N4, N10, N11).
///
/// A query that carries the `FROM ENDPOINT` or `ORGANISATION` directive is
/// [`Targeting::Directed`] at the endpoints the directive selects, which the
/// caller expands through its registry ([`directive::FacadeQuery::directive`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Targeting {
    /// The request names its endpoints (the `FROM ENDPOINT` directive or the
    /// `openEHR-federation-endpoint` header, §8), this many of them after an
    /// `ORGANISATION` is expanded to the endpoints it manages (N20).
    Directed {
        /// How many endpoints the request names.
        endpoints: NonZeroUsize,
    },
    /// Undirected, in a deployment with no localizer: every member is asked
    /// (N4, last sentence).
    AskAll,
    /// Undirected, in a deployment whose localizer derives the node set from
    /// the patient (N4).
    Localized,
}

impl Targeting {
    fn single_endpoint(self) -> bool {
        matches!(self, Self::Directed { endpoints } if endpoints.get() == 1)
    }
}

/// How the gateway answers `LIMIT n OFFSET k` with `k > 0` across a fan-out
/// (§11.6.2, N39), the strategy `OPTIONS {base}/` declares as
/// `paging.offset_strategy` (§7a.2).
///
/// Whichever is chosen, `OFFSET` is never pushed down to a node: per-node
/// rows `k..k + n` do not contain the global rows `k..k + n` (§11.6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OffsetStrategy {
    /// Every `OFFSET k > 0` is refused with [`Refusal::OffsetUnsupported`]
    /// (§11.6.2, the first option).
    Reject,
    /// The page is computed from `k + n` rows per node: each node is sent
    /// `LIMIT k + n` with no `OFFSET`, and the Tier merges, orders and keeps
    /// the rows `[k, k + n)` (§11.6.2, the second option). A page whose
    /// `k + n` is past `max_window`, or that has no `LIMIT` or no `ORDER BY`,
    /// is refused with [`Refusal::OffsetPage`].
    Bounded {
        /// The most rows one node is asked for, `k + n`.
        max_window: NonZeroU32,
    },
}

impl OffsetStrategy {
    /// The value `OPTIONS {base}/` declares as `paging.offset_strategy`
    /// (§7a.2): `reject` or `bounded`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Bounded { .. } => "bounded",
        }
    }

    /// The most rows one node is asked for a page, when the strategy bounds
    /// `k + n`.
    #[must_use]
    pub fn max_window(self) -> Option<NonZeroU32> {
        match self {
            Self::Reject => None,
            Self::Bounded { max_window } => Some(max_window),
        }
    }
}

/// What the deployment and the request add to the query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    default_namespace: Option<String>,
    targeting: Targeting,
    offset: OffsetStrategy,
    decomposable: BTreeSet<AggregateFunction>,
    dedup: DedupMode,
}

impl Context {
    /// A context with no default issuing namespace, which refuses every
    /// `OFFSET k > 0` ([`OffsetStrategy::Reject`]) and every undirected
    /// aggregate (no function is decomposable), and deduplicates nothing
    /// (N15).
    #[must_use]
    pub fn new(targeting: Targeting) -> Self {
        Self {
            default_namespace: None,
            targeting,
            offset: OffsetStrategy::Reject,
            decomposable: BTreeSet::new(),
            dedup: DedupMode::None,
        }
    }

    /// Declares how the request's node set is chosen: [`Targeting::Directed`]
    /// for a query that names its endpoints (§8, N11).
    #[must_use]
    pub fn with_targeting(mut self, targeting: Targeting) -> Self {
        self.targeting = targeting;
        self
    }

    /// How the request's node set is chosen.
    #[must_use]
    pub fn targeting(&self) -> Targeting {
        self.targeting
    }

    /// Declares the dedup mode the request selects (§10, N15). Under
    /// [`DedupMode::VersionIdentity`] every node is also asked the version
    /// uid of each row, and an undirected aggregate is refused (§11.6.3).
    #[must_use]
    pub fn with_dedup(mut self, mode: DedupMode) -> Self {
        self.dedup = mode;
        self
    }

    /// The dedup mode the request selects.
    #[must_use]
    pub fn dedup(&self) -> DedupMode {
        self.dedup
    }

    /// Declares the aggregate functions recombined across a fan-out instead of
    /// refused (§11.6.3, N39). An empty set refuses every undirected aggregate,
    /// as [`Context::new`] does.
    #[must_use]
    pub fn with_decomposable_aggregates(
        mut self,
        functions: impl IntoIterator<Item = AggregateFunction>,
    ) -> Self {
        self.decomposable = functions.into_iter().collect();
        self
    }

    /// The aggregate functions recombined across a fan-out, the list
    /// `OPTIONS {base}/` declares as `aggregates.decomposable` (§11.6.3,
    /// §7a.2), in declaration order.
    #[must_use]
    pub fn decomposable_aggregates(&self) -> &BTreeSet<AggregateFunction> {
        &self.decomposable
    }

    /// Declares how `OFFSET k > 0` is answered (§11.6.2, N39).
    #[must_use]
    pub fn with_offset_strategy(mut self, strategy: OffsetStrategy) -> Self {
        self.offset = strategy;
        self
    }

    /// How `OFFSET k > 0` is answered, the strategy `OPTIONS {base}/`
    /// declares (§11.6.2, §7a.2).
    #[must_use]
    pub fn offset_strategy(&self) -> OffsetStrategy {
        self.offset
    }

    /// Declares the issuing namespace an unqualified patient identifier
    /// resolves in (§5.2 requires the namespace; no specification governs the
    /// default: our own design).
    #[must_use]
    pub fn with_default_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.default_namespace = Some(namespace.into());
        self
    }
}

/// What a façade query is, once analysed.
#[derive(Debug, Clone)]
pub enum Analysis {
    /// The query names a patient, whose identifier is resolved to one
    /// `ehr_id` per node before dispatch (§5.2, N3).
    Patient(PatientQuery),
    /// The query names no patient: it is dispatched as written to the node set
    /// the request names, or to every member where no localizer is
    /// configured (N4).
    Unscoped(UnscopedQuery),
}

impl Analysis {
    /// The `columns[]` of the federated `RESULT_SET`, the gateway's own
    /// rendering of the façade query (N17, §9.2).
    #[must_use]
    pub fn columns(&self) -> &[ResultSetColumn] {
        match self {
            Self::Patient(query) => &query.columns,
            Self::Unscoped(query) => &query.columns,
        }
    }

    /// How the merge orders and cuts the node answers (§11.6.1, N13, N39):
    /// the node columns of the `ORDER BY` keys and the tie-break, and the
    /// façade's `LIMIT`.
    #[must_use]
    pub fn order(&self) -> &ResultOrder {
        match self {
            Self::Patient(query) => &query.order,
            Self::Unscoped(query) => &query.order,
        }
    }

    /// Where each façade column of a row comes from, in façade order.
    #[must_use]
    pub fn sources(&self) -> &[ColumnSource] {
        match self {
            Self::Patient(query) => &query.sources,
            Self::Unscoped(query) => &query.node.columns,
        }
    }

    /// How the Tier recombines the one-row node answers of an undirected
    /// aggregate query into the federation's row (§11.6.3), or `None` for a
    /// query whose node rows are merged as rows.
    #[must_use]
    pub fn recombination(&self) -> Option<&Recombination> {
        match self {
            Self::Patient(query) => query.recombination.as_ref(),
            Self::Unscoped(query) => query.recombination.as_ref(),
        }
    }

    /// Admits a best-effort answer to this query (§11.4).
    ///
    /// # Errors
    /// [`Refusal::PartialAggregate`] for an aggregate recombined across
    /// nodes: §11.6.3 permits the recombination only where it is exactly
    /// correct, and a combination over the nodes that answered is a wrong
    /// value for the federation, not a subset of a right one.
    pub fn admit_best_effort(&self) -> Result<(), Refusal> {
        match self.recombination() {
            Some(_) => Err(Refusal::PartialAggregate),
            None => Ok(()),
        }
    }
}

/// Where each façade column of a row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnSource {
    /// The node row's column at this index.
    Node(usize),
    /// The re-injected patient identifier, a constant `STRING` equal to the
    /// resolution input (N5, §7.1).
    Subject,
    /// The re-injected issuing namespace of the patient identifier.
    Namespace,
    /// An ENDPOINT attribute selected through the directive's variable, which
    /// no node is asked for and the Tier adds to the row (§9.3, N12).
    // TODO(#72): name the attribute and add its value from the registry and the resolving node.
    Endpoint,
}

/// A query to dispatch to one node, and how its rows map to the façade's
/// columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeQuery {
    aql: String,
    columns: Vec<ColumnSource>,
}

impl NodeQuery {
    /// The standard, non-federated AQL the node receives (N7).
    #[must_use]
    pub fn aql(&self) -> &str {
        &self.aql
    }

    /// One source per façade column, in façade order.
    #[must_use]
    pub fn columns(&self) -> &[ColumnSource] {
        &self.columns
    }
}

/// A façade query that names a patient.
#[derive(Debug, Clone)]
pub struct PatientQuery {
    subject: Subject,
    template: Box<SelectQuery>,
    columns: Vec<ResultSetColumn>,
    sources: Vec<ColumnSource>,
    stripped: Vec<Option<Range<usize>>>,
    order: ResultOrder,
    recombination: Option<Recombination>,
}

impl PatientQuery {
    /// The patient, resolution input for §5.2.
    #[must_use]
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The `columns[]` of the federated result (N17, §9.2).
    #[must_use]
    pub fn columns(&self) -> &[ResultSetColumn] {
        &self.columns
    }

    /// Where the patient predicates the rewrite consumed were written, one
    /// byte range per stripped predicate (`None` where the parser gave none):
    /// what a security event records of a strip, never the text (§5.4.3).
    #[must_use]
    pub fn stripped(&self) -> &[Option<Range<usize>>] {
        &self.stripped
    }

    /// The node query for a node where the patient resolved to `ehr_id`: the
    /// façade query with the patient predicate replaced by
    /// `<ehr>/ehr_id/value = '<ehr_id>'`, the subject columns left to
    /// re-injection, and every other term forwarded as written (§7.1).
    #[must_use]
    pub fn for_node(&self, ehr_id: &HierObjectId) -> NodeQuery {
        NodeQuery {
            aql: to_aql(&rewrite::scope_to(&self.template, ehr_id.value())),
            columns: self.sources.clone(),
        }
    }
}

/// A façade query that names no patient.
#[derive(Debug, Clone)]
pub struct UnscopedQuery {
    node: NodeQuery,
    columns: Vec<ResultSetColumn>,
    ehr_scoped: bool,
    order: ResultOrder,
    recombination: Option<Recombination>,
}

impl UnscopedQuery {
    /// The query every node in scope receives.
    #[must_use]
    pub fn node_query(&self) -> &NodeQuery {
        &self.node
    }

    /// The `columns[]` of the federated result (N17, §9.2).
    #[must_use]
    pub fn columns(&self) -> &[ResultSetColumn] {
        &self.columns
    }

    /// Whether the client already scoped the query to one `ehr_id` with the
    /// canonical `WHERE <ehr>/ehr_id/value = …` form (N29).
    #[must_use]
    pub fn ehr_scoped(&self) -> bool {
        self.ehr_scoped
    }
}

/// Analyses a façade query and prepares the node queries (§7.1).
///
/// `parameters` are the request's `query_parameters`, already converted to
/// AQL literals; `paging` carries its `offset` and `fetch` members. A query
/// may carry the `FROM ENDPOINT` or `ORGANISATION` directive (§8.1), which no
/// node query carries; `context` then says which node set it selects, as
/// [`directive::FacadeQuery`] describes.
///
/// An aggregate query directed to one endpoint is dispatched unchanged (N14).
/// An undirected one is recombined at the Tier when every function it applies
/// is declared decomposable in `context`, with `AVG` asked of each node as its
/// `SUM` and `COUNT` (§11.6.3), and is refused otherwise. A query that calls a
/// function AQL 1.1.0 does not define is dispatched unchanged to one directed
/// endpoint, and refused when it would reach more than one node, because the
/// gateway cannot tell whether the function aggregates (§11.6.3).
///
/// # Errors
/// A [`Refusal`], each an HTTP `400`, when the query is not AQL, a parameter
/// cannot be bound, the patient cannot be reduced to one `ehr_id` scope per
/// node, the identifier would survive into a node query, or the query asks for
/// what the gateway cannot answer correctly across nodes.
pub fn analyse(
    aql: &str,
    parameters: &Parameters,
    paging: Paging,
    context: &Context,
) -> Result<Analysis, Refusal> {
    FacadeQuery::parse(aql)?.analyse(parameters, paging, context)
}

/// Analyses the parsed façade `query`, whose `directive` the federation parse
/// lifted out (§7.1, §8.1).
fn analyse_tree(
    mut query: SelectQuery,
    directive: Option<&Directive>,
    parameters: &Parameters,
    paging: Paging,
    context: &Context,
) -> Result<Analysis, Refusal> {
    bind(&mut query, parameters).map_err(Refusal::Parameters)?;
    let facade_columns = render_columns(&query.select);
    let endpoint = match directive {
        Some(directive) => directive::strip_endpoint_columns(&mut query, directive)?,
        None => Vec::new(),
    };
    let findings = scan::scan(&query)?;
    // NOTE: §11.6.3 [[aggregate-block]] forbids per-node aggregate rows, and nothing tells the
    // gateway whether a function outside AQL aggregates, so a fan-out refuses it (N14).
    if let Some(found) = &findings.undefined
        && !context.targeting.single_endpoint()
    {
        return Err(Refusal::UndefinedFunction {
            at: found.at.clone(),
        });
    }
    let skip = paging::page(&mut query, paging, context.offset)?;
    let columns = render_columns(&query.select);
    let recombination = match &findings.aggregate {
        Some(found) if !context.targeting.single_endpoint() => {
            Some(aggregate::decompose(&mut query, context, found.at.clone())?)
        }
        Some(_) | None => None,
    };
    let ordered = rewrite::Rows::of(findings.aggregate.is_none(), context.dedup);
    let distinct = query.select.distinct;
    let mut analysis = match subject(&findings, context)? {
        Some((subject, consumed)) => {
            patient(query, &findings, subject, &consumed, columns, ordered)?
        }
        None => unscoped(query, &findings, context, columns, ordered)?,
    };
    if !endpoint.is_empty() {
        analysis.select_endpoint_attributes(facade_columns, &endpoint);
    }
    let (order, recombined, sources) = match &mut analysis {
        Analysis::Patient(query) => (
            &mut query.order,
            &mut query.recombination,
            query.sources.as_slice(),
        ),
        Analysis::Unscoped(query) => (
            &mut query.order,
            &mut query.recombination,
            query.node.columns.as_slice(),
        ),
    };
    let mut shaped = std::mem::take(order).with_offset(skip);
    if distinct {
        shaped = shaped.with_distinct(visible(sources));
    }
    *order = shaped;
    *recombined = recombination;
    Ok(analysis)
}

/// The node columns the client sees, in façade order: the columns a row is
/// distinct on (N13). The subject columns are one constant for the whole
/// answer, and a column the rewrite adds is never seen, so neither is one.
fn visible(sources: &[ColumnSource]) -> Vec<usize> {
    sources
        .iter()
        .filter_map(|source| match source {
            ColumnSource::Node(column) => Some(*column),
            // TODO(#72): an ENDPOINT attribute tells rows of different nodes apart under DISTINCT.
            ColumnSource::Subject | ColumnSource::Namespace | ColumnSource::Endpoint => None,
        })
        .collect()
}

fn first_fault(error: &ParseError) -> Option<Range<usize>> {
    match error {
        ParseError::Syntax { faults } => faults.iter().find_map(|fault| fault.bytes.clone()),
        ParseError::Lex(_) => None,
    }
}

/// The `columns[]` of the façade query: each column's alias, or `#<index>`,
/// and the path as written for a path column (N17, §9.2).
fn render_columns(select: &SelectClause) -> Vec<ResultSetColumn> {
    select
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| ResultSetColumn {
            name: column.alias.clone().unwrap_or_else(|| format!("#{index}")),
            path: match &column.column {
                ColumnExpr::Path(path) => Some(path.column_path_text()),
                ColumnExpr::Primitive(_) | ColumnExpr::Aggregate(_) | ColumnExpr::Function(_) => {
                    None
                }
            },
            additional_properties: std::collections::BTreeMap::new(),
        })
        .collect()
}

/// The patient the query names and the top-level leaves that name it, or
/// `None` for a query without a patient predicate.
fn subject(
    findings: &Findings,
    context: &Context,
) -> Result<Option<(Subject, Vec<usize>)>, Refusal> {
    let Some(first) = findings.ids.first() else {
        if let Some((_, _, at)) = findings
            .inputs
            .iter()
            .find(|(_, input, _)| *input == Input::Id)
        {
            return Err(Refusal::SubjectWithoutPredicate { at: at.clone() });
        }
        return Ok(None);
    };
    if let Some(second) = findings.ids.iter().find(|found| found.value != first.value) {
        return Err(Refusal::SecondSubject {
            at: second.at.clone(),
        });
    }
    if first.value.is_empty() {
        return Err(Refusal::EmptyIdentifier {
            at: first.at.clone(),
        });
    }
    if findings.ehr.len() > 1 {
        return Err(Refusal::Unreducible {
            reason: Unreducible::SeveralEhrs,
            at: first.at.clone(),
        });
    }
    let namespace = match findings.namespaces.first() {
        Some(named) => {
            if let Some(second) = findings
                .namespaces
                .iter()
                .find(|found| found.value != named.value)
            {
                return Err(Refusal::SecondNamespace {
                    at: second.at.clone(),
                });
            }
            (named.value.clone(), NamespaceOrigin::Query)
        }
        None => match &context.default_namespace {
            Some(default) => (default.clone(), NamespaceOrigin::Default),
            None => return Err(Refusal::NoNamespace),
        },
    };
    let consumed = findings
        .ids
        .iter()
        .chain(&findings.namespaces)
        .map(|found| found.leaf)
        .collect();
    let (namespace, origin) = namespace;
    Ok(Some((
        Subject::new(first.value.clone(), namespace, origin),
        consumed,
    )))
}

fn patient(
    query: SelectQuery,
    findings: &Findings,
    subject: Subject,
    consumed: &[usize],
    columns: Vec<ResultSetColumn>,
    ordered: rewrite::Rows,
) -> Result<Analysis, Refusal> {
    let inputs: Vec<usize> = findings.inputs.iter().map(|(index, _, _)| *index).collect();
    let stripped = findings
        .ids
        .iter()
        .chain(&findings.namespaces)
        .map(|found| found.at.clone())
        .collect();
    let mut dispatched = query.clone();
    rewrite::strip_where(&mut dispatched, consumed, None);
    rewrite::strip_columns(&mut dispatched, &inputs);
    rewrite::order_for(&mut dispatched, ordered, true)?;
    if let Some(leak) = scan::reaches(&dispatched, subject.value()) {
        return Err(match leak.kind {
            scan::LeakKind::Value => Refusal::IdentifierElsewhere { at: leak.at },
            scan::LeakKind::Unfoldable => Refusal::UnfoldableFunction { at: leak.at },
        });
    }
    let mut template = query;
    let ehr = rewrite::ehr_variable(&mut template, &findings.ehr);
    rewrite::strip_where(&mut template, consumed, Some(&ehr));
    rewrite::strip_columns(&mut template, &inputs);
    rewrite::keep_a_column(&mut template, &ehr);
    let order = rewrite::order_for(&mut template, ordered, true)?;
    let mut node = 0_usize;
    let sources = (0..columns.len())
        .map(|index| {
            match findings
                .inputs
                .iter()
                .find(|(column, _, _)| *column == index)
            {
                Some((_, Input::Id, _)) => ColumnSource::Subject,
                Some((_, Input::Namespace, _)) => ColumnSource::Namespace,
                None => {
                    let source = ColumnSource::Node(node);
                    node = node.saturating_add(1);
                    source
                }
            }
        })
        .collect();
    Ok(Analysis::Patient(PatientQuery {
        subject,
        template: Box::new(template),
        columns,
        sources,
        stripped,
        order,
        recombination: None,
    }))
}

fn unscoped(
    mut query: SelectQuery,
    findings: &Findings,
    context: &Context,
    columns: Vec<ResultSetColumn>,
    ordered: rewrite::Rows,
) -> Result<Analysis, Refusal> {
    if context.targeting == Targeting::Localized {
        return Err(Refusal::NodeSetUndefined);
    }
    if query.select.columns.is_empty() {
        // NOTE: no specification governs a query that selects only ENDPOINT attributes (§9.3):
        // our own design, each node answers one EHR column per row it holds.
        let ehr = rewrite::ehr_variable(&mut query, &findings.ehr);
        rewrite::keep_a_column(&mut query, &ehr);
    }
    let order = rewrite::order_for(&mut query, ordered, false)?;
    let sources = (0..columns.len()).map(ColumnSource::Node).collect();
    Ok(Analysis::Unscoped(UnscopedQuery {
        node: NodeQuery {
            aql: to_aql(&query),
            columns: sources,
        },
        columns,
        ehr_scoped: findings.ehr_scoped,
        order,
        recombination: None,
    }))
}
