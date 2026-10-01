// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL rewrite of the Federation Tier (feature `aql`): one façade query
//! in, one standard, `ehr_id`-scoped query per node out (§7.1, N2, N7).
//!
//! [`analyse`] parses the client's query with `openehr-query`, binds the
//! ITS-REST `query_parameters` into the tree before anything reads it, finds
//! the patient on the `EHR_STATUS.subject.external_ref` carrier, and refuses
//! with a [`refusal::Refusal`] whatever cannot be consumed exactly. A
//! [`PatientQuery`] then prints the node query for each resolved `ehr_id` with
//! `printer::to_aql`. Every step is a transformation of the AQL syntax tree:
//! there is no AQL text splicing, and a value reaches a node query only
//! through the printer's escaping.
//!
//! The patient identifier never reaches a node query (§5.4.1, N33): the
//! predicate that carries it is replaced by `<ehr>/ehr_id/value = '<ehr_id>'`,
//! a selected subject column is re-injected after the merge instead of being
//! asked of the node (N5), and a query in which the value appears anywhere
//! else is refused. The AQL release the module rewrites is [`crate::AQL`].
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

mod rewrite;
mod scan;

pub mod refusal;
pub mod subject;

use std::num::NonZeroUsize;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_its::rest::generated::query::ResultSetColumn;
use openehr_query::ast::{ColumnExpr, Limit, SelectClause, SelectQuery};
use openehr_query::bind::{Parameters, bind};
use openehr_query::parser::{ParseError, parse_str};
use openehr_query::printer::to_aql;

use refusal::{Refusal, Unreducible};
use scan::{Findings, Input};
use subject::{NamespaceOrigin, Subject};

/// The ITS-REST paging members sent beside the AQL (`AdhocQueryExecute`
/// `offset` and `fetch`, or the GET parameters of the same names).
///
/// They page like the AQL `OFFSET` and `LIMIT` clauses and follow the same
/// rules (decision A10; §11.6 and N39 name only the clauses).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Paging {
    /// The `offset` member.
    pub offset: Option<i64>,
    /// The `fetch` member.
    pub fetch: Option<i64>,
}

/// How the node set of the request is chosen (§8, N4, N10, N11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Targeting {
    /// The request names its endpoints (the `FROM ENDPOINT` directive or the
    /// `openEHR-federation-endpoint` header, §8), this many of them.
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

/// What the deployment and the request add to the query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    default_namespace: Option<String>,
    targeting: Targeting,
}

impl Context {
    /// A context with no default issuing namespace.
    #[must_use]
    pub fn new(targeting: Targeting) -> Self {
        Self {
            default_namespace: None,
            targeting,
        }
    }

    /// Declares the issuing namespace an unqualified patient identifier
    /// resolves in (decision A5).
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
    /// configured (decision A8).
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
/// AQL literals; `paging` carries its `offset` and `fetch` members.
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
    let mut query = parse_str(aql).map_err(|error| Refusal::NotAql {
        at: first_fault(&error),
    })?;
    bind(&mut query, parameters).map_err(Refusal::Parameters)?;
    let findings = scan::scan(&query)?;
    page(&mut query, paging)?;
    if let Some(aggregate) = &findings.aggregate
        && !context.targeting.single_endpoint()
    {
        return Err(Refusal::UndirectedAggregate {
            at: aggregate.at.clone(),
        });
    }
    let columns = render_columns(&query.select);
    match subject(&findings, context)? {
        Some((subject, consumed)) => patient(query, &findings, subject, &consumed, columns),
        None => unscoped(&query, &findings, context, columns),
    }
}

fn first_fault(error: &ParseError) -> Option<std::ops::Range<usize>> {
    match error {
        ParseError::Syntax { faults } => faults.iter().find_map(|fault| fault.bytes.clone()),
        ParseError::Lex(_) => None,
    }
}

/// Applies the ITS-REST paging members to the query's `LIMIT` and `OFFSET`
/// (decision A10).
fn page(query: &mut SelectQuery, paging: Paging) -> Result<(), Refusal> {
    if paging.offset.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "offset" });
    }
    if paging.fetch.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "fetch" });
    }
    let clause_limit = query.limit.as_ref().map(|limit| limit.limit);
    let clause_offset = query.limit.as_ref().and_then(|limit| limit.offset);
    if let (Some(member), Some(clause)) = (paging.fetch, clause_limit)
        && member != clause
    {
        return Err(Refusal::PagingConflict {
            member: "fetch",
            clause: "LIMIT",
        });
    }
    if let (Some(member), Some(clause)) = (paging.offset, clause_offset)
        && member != clause
    {
        return Err(Refusal::PagingConflict {
            member: "offset",
            clause: "OFFSET",
        });
    }
    // TODO(#53): compute an exact page from k + n rows per node, within a bound.
    if clause_offset.or(paging.offset).unwrap_or(0) > 0 {
        return Err(Refusal::OffsetUnsupported);
    }
    query.limit = clause_limit.or(paging.fetch).map(|limit| Limit {
        limit,
        offset: None,
    });
    Ok(())
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
) -> Result<Analysis, Refusal> {
    let inputs: Vec<usize> = findings.inputs.iter().map(|(index, _, _)| *index).collect();
    let mut dispatched = query.clone();
    rewrite::strip_where(&mut dispatched, consumed, None);
    rewrite::strip_columns(&mut dispatched, &inputs);
    if let Some(hit) = scan::reaches(&dispatched, subject.value()) {
        return Err(Refusal::IdentifierElsewhere { at: hit.at });
    }
    let mut template = query;
    let ehr = rewrite::ehr_variable(&mut template, &findings.ehr);
    rewrite::strip_where(&mut template, consumed, Some(&ehr));
    rewrite::strip_columns(&mut template, &inputs);
    rewrite::keep_a_column(&mut template, &ehr);
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
    }))
}

fn unscoped(
    query: &SelectQuery,
    findings: &Findings,
    context: &Context,
    columns: Vec<ResultSetColumn>,
) -> Result<Analysis, Refusal> {
    if context.targeting == Targeting::Localized {
        return Err(Refusal::NodeSetUndefined);
    }
    let sources = (0..columns.len()).map(ColumnSource::Node).collect();
    Ok(Analysis::Unscoped(UnscopedQuery {
        node: NodeQuery {
            aql: to_aql(query),
            columns: sources,
        },
        columns,
        ehr_scoped: findings.ehr_scoped,
    }))
}
