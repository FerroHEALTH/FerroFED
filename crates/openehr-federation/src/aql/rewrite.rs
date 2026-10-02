// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node query of §7.1, built once from the bound façade AST as a
//! template and scoped to each node's `ehr_id` by one substitution.

use openehr_query::ast::{
    ClassExprOperand, ColumnExpr, CompareOperand, ContainsConstraint, ContainsExpr, FunctionCall,
    IdentifiedExpr, IdentifiedPath, ObjectPath, OrderByExpr, PathPart, Primitive, SelectExpr,
    SelectQuery, SortOrder, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;
use openehr_query::visit::{VisitMut, walk_terminal_mut};

use super::refusal::Refusal;
use crate::dedup::DedupMode;
use crate::order::{Direction, ResultOrder, SortKey};

/// The parameter name the template holds the `ehr_id` in. A bound query has
/// no parameter left, so the name cannot collide with one the client wrote.
const EHR_ID: &str = "ferrofed_node_ehr_id";

/// Removes the consumed `WHERE` leaves of the top-level `AND` chain. When
/// `scope` is set, the first of them becomes `<ehr>/ehr_id/value = $ehr_id`
/// (the canonical form of N29), so the scope sits where the client wrote the
/// patient.
pub(super) fn strip_where(query: &mut SelectQuery, consumed: &[usize], scope: Option<&str>) {
    let Some(where_) = query.where_.take() else {
        return;
    };
    let mut pruner = Pruner {
        consumed,
        scope,
        first: consumed.iter().min().copied(),
        leaves: 0,
    };
    query.where_ = pruner.prune(where_, true);
}

struct Pruner<'a> {
    consumed: &'a [usize],
    scope: Option<&'a str>,
    first: Option<usize>,
    leaves: usize,
}

impl Pruner<'_> {
    fn prune(&mut self, node: WhereExpr, top: bool) -> Option<WhereExpr> {
        match node {
            WhereExpr::And(left, right) if top => {
                let left = self.prune(*left, true);
                let right = self.prune(*right, true);
                match (left, right) {
                    (Some(left), Some(right)) => {
                        Some(WhereExpr::And(Box::new(left), Box::new(right)))
                    }
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                }
            }
            WhereExpr::Identified(expr, span) if top => {
                let leaf = self.leaves;
                self.leaves = self.leaves.saturating_add(1);
                if !self.consumed.contains(&leaf) {
                    return Some(WhereExpr::Identified(expr, span));
                }
                match (self.scope, self.first == Some(leaf)) {
                    (Some(ehr), true) => Some(WhereExpr::identified(ehr_id_predicate(ehr))),
                    _ => None,
                }
            }
            other => Some(other),
        }
    }
}

/// `<ehr>/ehr_id/value = $ehr_id`.
fn ehr_id_predicate(ehr: &str) -> IdentifiedExpr {
    IdentifiedExpr::Compare {
        lhs: CompareOperand::Path(ehr_id_path(ehr)),
        op: CompOp::Eq,
        rhs: Terminal::Parameter(EHR_ID.to_owned()),
    }
}

fn ehr_id_path(ehr: &str) -> IdentifiedPath {
    attribute_path(ehr, &["ehr_id", "value"])
}

/// `<variable>/<parts…>`, a path with no predicate.
fn attribute_path(variable: &str, parts: &[&str]) -> IdentifiedPath {
    IdentifiedPath::new(
        variable.to_owned(),
        None,
        Some(ObjectPath {
            parts: parts
                .iter()
                .map(|name| PathPart {
                    name: (*name).to_owned(),
                    predicate: None,
                })
                .collect(),
        }),
    )
}

/// Removes the selected columns at `indices`, the subject columns the gateway
/// re-injects instead of asking the node (N5, §7.1).
pub(super) fn strip_columns(query: &mut SelectQuery, indices: &[usize]) {
    let columns = std::mem::take(&mut query.select.columns);
    query.select.columns = columns
        .into_iter()
        .enumerate()
        .filter_map(|(index, column)| (!indices.contains(&index)).then_some(column))
        .collect();
}

/// Asks the node for `<ehr>/ehr_id/value` when no column is left, so that
/// each row still stands for an `EHR` that exists.
// NOTE: no specification governs a query that selects only the subject: our own design.
pub(super) fn keep_a_column(query: &mut SelectQuery, ehr: &str) {
    if query.select.columns.is_empty() {
        query.select.columns.push(SelectExpr {
            column: ColumnExpr::Path(ehr_id_path(ehr)),
            alias: None,
        });
    }
}

/// The variable the node scope is written on: the query's `EHR` variable, or
/// a fresh one with `FROM` wrapped in `EHR <var> CONTAINS …` when the query
/// has no `EHR` containment (AQL admits such a query, §5.4.3 accepts its
/// `ENTRY` carrier, and N7 requires the scope).
pub(super) fn ehr_variable(query: &mut SelectQuery, bound: &[String]) -> String {
    if let Some(variable) = bound.first() {
        return variable.clone();
    }
    let variable = fresh_variable(query);
    let from = query.from.clone();
    query.from = ContainsExpr::Contained {
        operand: ClassExprOperand::Class {
            rm_type: "EHR".to_owned(),
            variable: Some(variable.clone()),
            predicate: None,
        },
        contains: Some(Box::new(ContainsConstraint {
            negated: false,
            expr: from,
        })),
    };
    variable
}

/// A variable name no class of the containment uses.
fn fresh_variable(query: &SelectQuery) -> String {
    let mut taken = Vec::new();
    variables(&query.from, &mut taken);
    let mut candidate = String::from("e");
    let mut suffix = 0_u32;
    while taken.contains(&candidate) {
        suffix = suffix.saturating_add(1);
        candidate = format!("e{suffix}");
    }
    candidate
}

fn variables(from: &ContainsExpr, out: &mut Vec<String>) {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            let (ClassExprOperand::Class { variable, .. }
            | ClassExprOperand::Version { variable, .. }) = operand;
            if let Some(variable) = variable {
                out.push(variable.clone());
            }
            if let Some(constraint) = contains {
                variables(&constraint.expr, out);
            }
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            variables(left, out);
            variables(right, out);
        }
    }
}

/// How the rows of a node query are shaped at the Tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rows {
    /// An aggregate query: one row per node, the federated one at a single
    /// directed endpoint (N14), or the row the Tier recombines (§11.6.3).
    Aggregate,
    /// Rows merged under the Tier order, deduplicated on version identity
    /// when `dedup` is set (§10.2).
    Plain {
        /// Whether the request selects version-identity dedup.
        dedup: bool,
    },
}

impl Rows {
    /// The shape of a query that is `plain` (has no aggregate) under `mode`.
    pub(super) fn of(plain: bool, mode: DedupMode) -> Self {
        if plain {
            Self::Plain {
                dedup: mode == DedupMode::VersionIdentity,
            }
        } else {
            Self::Aggregate
        }
    }
}

/// The Tier order of a node query, written into it (§11.6.1).
///
/// An aggregate query's node query keeps the client's order as written, and
/// the merge only applies its `LIMIT`.
///
/// # Errors
/// As [`push_order`].
pub(super) fn order_for(
    query: &mut SelectQuery,
    rows: Rows,
    one_ehr: bool,
) -> Result<ResultOrder, Refusal> {
    match rows {
        Rows::Plain { dedup } => push_order(query, one_ehr, dedup),
        Rows::Aggregate => Ok(ResultOrder::new(
            Vec::new(),
            Vec::new(),
            dispatched_limit(query)?,
        )),
    }
}

/// Writes the Tier order into a node query and returns its description
/// (§11.6.1, N13, N39).
///
/// The node is sent the client's `LIMIT n` unchanged (§11.6.1 MUST). With no
/// `ORDER BY` the query is otherwise left as written. With one:
///
/// - an `ORDER BY` path that is not selected becomes a hidden column the merge
///   reads and the gateway strips, never answered to the client (no
///   specification governs this: our own design);
/// - the row key of [`row_key`] becomes the last `ORDER BY` key, the
///   tie-break after `endpoint_id`: the uid §11.6.1 recommends, or the
///   `ehr_id` of a row that has no uid.
///
/// A key appended after the client's keys refines their order and never
/// reorders it, so the node's top `n` stays a top `n` under the client's
/// `ORDER BY`, and the node picks the same tied rows at its cut on every
/// repeat, for `LIMIT n` and for the `LIMIT k + n` of a page (§11.6.2). Under
/// `DISTINCT` no column is added, since it would change which rows are
/// distinct: every `ORDER BY` path must be selected, the remaining selected
/// paths are the tie-break in place of the row key, and with a `LIMIT` every
/// other selected column must be fixed by those paths (`pinned_by_paths`).
///
/// `one_ehr` says the gateway scoped the node query to one `ehr_id`, so an
/// `EHR`'s own id is the same on every row and is not a key.
///
/// Under `dedup`, the version uid of [`version_uid`] is the dedup key
/// (§10.2): a hidden column when it is not selected, except under `DISTINCT`,
/// where a query that does not select it has no key and nothing is
/// suppressed. A query with a `LIMIT` and no `ORDER BY` is then ordered on
/// the uid, so the copy a node returns before its cut is the one the Tier
/// keeps. AQL leaves the collation of strings undefined (`master03-syntax.adoc`
/// §ORDER BY), so each node orders the uid under its own collation; the merge
/// orders it without regard to case and refuses a node cut at its `LIMIT`
/// whose rows disagree with that order.
///
/// # Errors
/// [`Refusal::OrderNotSelected`] for a `DISTINCT` query ordered on a path it
/// does not select, [`Refusal::UnorderedDistinctCut`] for a `DISTINCT` query
/// with a `LIMIT` whose selected paths do not fix every column, and
/// [`Refusal::NegativePaging`] for a negative `LIMIT`.
fn push_order(query: &mut SelectQuery, one_ehr: bool, dedup: bool) -> Result<ResultOrder, Refusal> {
    let limit = dispatched_limit(query)?;
    let distinct = query.select.distinct;
    let version = version_uid(&query.from)
        .filter(|path| dedup && (!distinct || selected(&query.select.columns, path).is_some()));
    // NOTE: AQL 1.1.0 §LIMIT ties a determined answer to ORDER BY, so a query without one may
    // be ordered on the uid, which keeps the copy the Tier keeps inside its node's cut (§11.6.1).
    if let Some(path) = &version
        && query.order_by.is_empty()
        && limit.is_some()
    {
        push_ascending(&mut query.order_by, path.clone());
    }
    let keyed = |order: ResultOrder, columns: &mut Vec<SelectExpr>| match &version {
        Some(path) => {
            let column = selected(columns, path).unwrap_or_else(|| hide(columns, path));
            order.with_version_key(column)
        }
        None => order,
    };
    if query.order_by.is_empty() {
        let order = ResultOrder::new(Vec::new(), Vec::new(), limit);
        return Ok(keyed(order, &mut query.select.columns));
    }
    let mut keys = Vec::with_capacity(query.order_by.len());
    for term in &query.order_by {
        let column = match selected(&query.select.columns, &term.path) {
            Some(column) => column,
            None if distinct => {
                return Err(Refusal::OrderNotSelected {
                    at: term.path.span.bytes(),
                });
            }
            None => hide(&mut query.select.columns, &term.path),
        };
        keys.push(SortKey::new(column, direction(term.order)));
    }
    let tie_break = if distinct {
        if limit.is_some() {
            pinned_by_paths(&query.select.columns)?;
        }
        let ordered: Vec<usize> = keys.iter().map(SortKey::column).collect();
        let tie_break: Vec<(usize, IdentifiedPath)> = query
            .select
            .columns
            .iter()
            .enumerate()
            .filter(|(index, _)| !ordered.contains(index))
            .filter_map(|(index, column)| match &column.column {
                ColumnExpr::Path(path) => Some((index, path.clone())),
                ColumnExpr::Primitive(_) | ColumnExpr::Aggregate(_) | ColumnExpr::Function(_) => {
                    None
                }
            })
            .collect();
        let mut columns = Vec::with_capacity(tie_break.len());
        for (index, path) in tie_break {
            push_ascending(&mut query.order_by, path);
            columns.push(index);
        }
        columns
    } else {
        row_key(&query.from, one_ehr).map_or_else(Vec::new, |path| {
            let column = selected(&query.select.columns, &path)
                .unwrap_or_else(|| hide(&mut query.select.columns, &path));
            if !query.order_by.iter().any(|term| term.path == path) {
                push_ascending(&mut query.order_by, path);
            }
            vec![column]
        })
    };
    let order = ResultOrder::new(keys, tie_break, limit);
    Ok(keyed(order, &mut query.select.columns))
}

/// Checks that every selected column of a `DISTINCT` query is fixed by its
/// selected paths, the only columns a node can order on.
///
/// AQL orders on identified paths alone (AQL master03-syntax §ORDER BY,
/// `orderByExpr : identifiedPath`), so under `DISTINCT` the node is sent the
/// selected paths as its keys and nothing else. Two distinct rows tied on all
/// of them differ only in a column that is not a path, and a node cut at its
/// `LIMIT` may keep either, a different one on each repeat, where §11.6.1
/// requires that "repeating a query returns rows in the same order". A literal
/// is one value on every row. A call to a single-row function AQL defines is
/// fixed when every argument is a literal, a parameter, a selected path or
/// such a call. A call with no argument (`NOW()` and the other clock
/// functions, which return "the current" date or time), `TERMINOLOGY` (whose
/// result comes from a terminology server), and a function AQL does not
/// define are not fixed by the row.
///
/// # Errors
/// [`Refusal::UnorderedDistinctCut`] for the first selected column that is not
/// fixed.
fn pinned_by_paths(columns: &[SelectExpr]) -> Result<(), Refusal> {
    let paths: Vec<&IdentifiedPath> = columns
        .iter()
        .filter_map(|column| match &column.column {
            ColumnExpr::Path(path) => Some(path),
            ColumnExpr::Primitive(_) | ColumnExpr::Aggregate(_) | ColumnExpr::Function(_) => None,
        })
        .collect();
    for column in columns {
        if let ColumnExpr::Function(call) = &column.column
            && !fixed(call, &paths)
        {
            return Err(Refusal::UnorderedDistinctCut {
                at: super::scan::first_path(call),
            });
        }
    }
    Ok(())
}

/// Whether `call` returns one value for every row whose `paths` are equal.
fn fixed(call: &FunctionCall, paths: &[&IdentifiedPath]) -> bool {
    // NOTE: AQL master03-syntax §Functions, a built-in is single-row; TERMINOLOGY and another
    // name are not fixed by the row.
    let FunctionCall::Builtin { args, .. } = call else {
        return false;
    };
    !args.is_empty()
        && args.iter().all(|arg| match arg {
            Terminal::Primitive(_) | Terminal::Parameter(_) => true,
            Terminal::Path(path) => paths.contains(&path),
            Terminal::Function(inner) => fixed(inner, paths),
        })
}

/// The `LIMIT` the node query carries.
///
/// # Errors
/// [`Refusal::NegativePaging`] for a negative `LIMIT`.
pub(super) fn dispatched_limit(query: &SelectQuery) -> Result<Option<u64>, Refusal> {
    query
        .limit
        .as_ref()
        .map(|clause| {
            u64::try_from(clause.limit)
                .map_err(|_negative| Refusal::NegativePaging { member: "LIMIT" })
        })
        .transpose()
}

/// The index of the selected column that is `path`, if any.
fn selected(columns: &[SelectExpr], path: &IdentifiedPath) -> Option<usize> {
    columns
        .iter()
        .position(|column| matches!(&column.column, ColumnExpr::Path(selected) if selected == path))
}

/// Appends `path` as a hidden column and returns its index.
fn hide(columns: &mut Vec<SelectExpr>, path: &IdentifiedPath) -> usize {
    columns.push(SelectExpr {
        column: ColumnExpr::Path(path.clone()),
        alias: None,
    });
    columns.len().saturating_sub(1)
}

fn push_ascending(order_by: &mut Vec<OrderByExpr>, path: IdentifiedPath) {
    order_by.push(OrderByExpr {
        path,
        order: Some(SortOrder::Ascending),
    });
}

fn direction(order: Option<SortOrder>) -> Direction {
    match order {
        Some(SortOrder::Descending) => Direction::Descending,
        Some(SortOrder::Ascending) | None => Direction::Ascending,
    }
}

/// The path whose value tells a row apart from the rows tied with it, from
/// the first class of each kind in the containment, in this order:
///
/// 1. `COMPOSITION`, then `VERSION`: `<var>/uid/value`, the version uid (the
///    RM recommends a `COMPOSITION` carry its `VERSION`'s uid);
/// 2. `EHR`: `<var>/ehr_id/value`, mandatory and unique per `EHR` (RM
///    `EHR.ehr_id`);
/// 3. `EHR_STATUS` or `EHR_ACCESS`: `<var>/uid/value`, one per `EHR`, whose
///    uid the RM recommends be its `VERSION`'s.
///
/// Under `one_ehr` the second and third are the same on every row, so they
/// are skipped. `FOLDER` is never a key: the RM recommends a uid only on a
/// tree-root folder, and a `FOLDER` class matches sub-folders too. With no
/// key, a node chooses among rows tied on every key it was sent, and the Tier
/// orders the rows it receives on their cells.
// NOTE: §11.6.1 recommends "`endpoint_id`, then uid"; the key of a row without a uid
// follows it in spirit, and no specification governs it: our own design.
fn row_key(from: &ContainsExpr, one_ehr: bool) -> Option<IdentifiedPath> {
    let found = Variables::of(from);
    if let Some(variable) = found.composition.or(found.version) {
        return Some(uid_path(&variable));
    }
    if one_ehr {
        return None;
    }
    found
        .ehr
        .map(|variable| ehr_id_path(&variable))
        .or_else(|| found.per_ehr.map(|variable| uid_path(&variable)))
}

/// The path of a row's version uid, the dedup key of §10.2: the uid of the
/// first `COMPOSITION`, else of the first `VERSION`, the first choice of
/// [`row_key`]. A query over neither reads no version, and its rows are never
/// suppressed.
// NOTE: §10.2 keys the mode on the imported composition's VERSION uid; an EHR_STATUS or
// EHR_ACCESS is one per EHR and never a copy at a second node (our own design).
fn version_uid(from: &ContainsExpr) -> Option<IdentifiedPath> {
    let found = Variables::of(from);
    found
        .composition
        .or(found.version)
        .map(|variable| uid_path(&variable))
}

/// The first variable of each class kind a row key reads.
#[derive(Default)]
struct Variables {
    composition: Option<String>,
    version: Option<String>,
    ehr: Option<String>,
    per_ehr: Option<String>,
}

impl Variables {
    fn of(from: &ContainsExpr) -> Self {
        let mut found = Self::default();
        classes(from, &mut |operand| match operand {
            ClassExprOperand::Class {
                rm_type,
                variable: Some(variable),
                ..
            } => {
                let slot = match rm_type.as_str() {
                    "COMPOSITION" => &mut found.composition,
                    "EHR" => &mut found.ehr,
                    "EHR_STATUS" | "EHR_ACCESS" => &mut found.per_ehr,
                    _ => return,
                };
                slot.get_or_insert_with(|| variable.clone());
            }
            ClassExprOperand::Version {
                variable: Some(variable),
                ..
            } => {
                found.version.get_or_insert_with(|| variable.clone());
            }
            ClassExprOperand::Class { .. } | ClassExprOperand::Version { .. } => {}
        });
        found
    }
}

fn classes(from: &ContainsExpr, visit: &mut impl FnMut(&ClassExprOperand)) {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            visit(operand);
            if let Some(constraint) = contains {
                classes(&constraint.expr, visit);
            }
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            classes(left, visit);
            classes(right, visit);
        }
    }
}

/// `<variable>/uid/value`.
fn uid_path(variable: &str) -> IdentifiedPath {
    attribute_path(variable, &["uid", "value"])
}

/// Writes the node's `ehr_id` into a template built by [`strip_where`].
pub(super) fn scope_to(template: &SelectQuery, ehr_id: &str) -> SelectQuery {
    let mut query = template.clone();
    Substitute { ehr_id }.visit_select_query_mut(&mut query);
    query
}

struct Substitute<'a> {
    ehr_id: &'a str,
}

impl VisitMut for Substitute<'_> {
    fn visit_terminal_mut(&mut self, node: &mut Terminal) {
        if matches!(node, Terminal::Parameter(name) if name == EHR_ID) {
            *node = Terminal::Primitive(Primitive::String(self.ehr_id.to_owned()));
        } else {
            walk_terminal_mut(self, node);
        }
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::parser::parse_str;
    use openehr_query::printer::to_aql;

    use super::{ehr_variable, scope_to, strip_where};

    #[test]
    fn a_query_with_no_ehr_containment_is_wrapped_never_refused() {
        // AQL admits the query, and N7 requires the node scope.
        let mut query =
            parse_str("SELECT c/uid/value FROM COMPOSITION c CONTAINS OBSERVATION e").unwrap();
        let variable = ehr_variable(&mut query, &[]);
        assert_eq!(
            variable, "e1",
            "the fresh variable must not shadow the query's own `e`"
        );
        strip_where(&mut query, &[], None);
        let mut scoped = query.clone();
        scoped.where_ = Some(openehr_query::ast::WhereExpr::identified(
            super::ehr_id_predicate(&variable),
        ));
        let node = to_aql(&scope_to(&scoped, "7d44b88c-4199-4bad-97dc-d78268e01398"));
        let expected = parse_str(
            "SELECT c/uid/value FROM EHR e1 CONTAINS COMPOSITION c CONTAINS OBSERVATION e \
             WHERE e1/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'",
        )
        .unwrap();
        assert_eq!(
            parse_str(&node).unwrap(),
            expected,
            "the wrapped node query was {node}"
        );
    }

    #[test]
    fn an_existing_ehr_variable_is_used_as_written() {
        let mut query = parse_str("SELECT c/uid/value FROM EHR x CONTAINS COMPOSITION c").unwrap();
        let before = query.clone();
        assert_eq!(
            ehr_variable(&mut query, &["x".to_owned()]),
            "x",
            "the bound EHR variable is the scope"
        );
        assert_eq!(
            query, before,
            "a query that has an EHR containment is not rewrapped"
        );
    }
}
