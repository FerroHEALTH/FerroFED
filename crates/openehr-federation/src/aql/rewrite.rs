// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node query of §7.1, built once from the bound façade AST as a
//! template and scoped to each node's `ehr_id` by one substitution.

use openehr_query::ast::{
    ClassExprOperand, ColumnExpr, CompareOperand, ContainsConstraint, ContainsExpr, IdentifiedExpr,
    IdentifiedPath, ObjectPath, OrderByExpr, PathPart, Primitive, SelectExpr, SelectQuery,
    SortOrder, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;
use openehr_query::visit::{VisitMut, walk_terminal_mut};

use super::refusal::Refusal;
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
    IdentifiedPath::new(
        ehr.to_owned(),
        None,
        Some(ObjectPath {
            parts: ["ehr_id", "value"]
                .into_iter()
                .map(|name| PathPart {
                    name: name.to_owned(),
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
/// has no `EHR` containment (decision A3: AQL admits such a query, and N7
/// requires the scope).
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

/// Writes the Tier order into a node query and returns its description
/// (§11.6.1, N13, N39; `docs/architecture.md` section 9, decisions A28 and A43).
///
/// The node is sent the client's `LIMIT n` unchanged (§11.6.1 MUST). With no
/// `ORDER BY` the query is otherwise left as written. With one:
///
/// - an `ORDER BY` path that is not selected becomes a hidden column the merge
///   reads and the gateway strips, never answered to the client (A28);
/// - the uid of the query's versioned object becomes the last `ORDER BY` key,
///   the tie-break §11.6.1 recommends after `endpoint_id`.
///
/// A key appended after the client's keys refines their order and never
/// reorders it, so the node's top `n` stays a top `n` under the client's
/// `ORDER BY`, and the node picks the same tied rows at its cut on every
/// repeat. Under `DISTINCT` no column is added, since it would change which
/// rows are distinct: every `ORDER BY` path must be selected, and the
/// remaining selected paths are the tie-break in place of the uid.
///
/// # Errors
/// [`Refusal::OrderNotSelected`] for a `DISTINCT` query ordered on a path it
/// does not select, and [`Refusal::NegativePaging`] for a negative `LIMIT`.
pub(super) fn push_order(query: &mut SelectQuery) -> Result<ResultOrder, Refusal> {
    let limit = match query.limit.as_ref() {
        Some(clause) => Some(
            u64::try_from(clause.limit)
                .map_err(|_negative| Refusal::NegativePaging { member: "LIMIT" })?,
        ),
        None => None,
    };
    if query.order_by.is_empty() {
        return Ok(ResultOrder::new(Vec::new(), Vec::new(), limit));
    }
    let distinct = query.select.distinct;
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
        versioned_variable(&query.from).map_or_else(Vec::new, |variable| {
            let path = uid_path(&variable);
            let column = selected(&query.select.columns, &path)
                .unwrap_or_else(|| hide(&mut query.select.columns, &path));
            if !query.order_by.iter().any(|term| term.path == path) {
                push_ascending(&mut query.order_by, path);
            }
            vec![column]
        })
    };
    Ok(ResultOrder::new(keys, tie_break, limit))
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

/// The variable whose `uid` identifies a row's versioned object: the first
/// `COMPOSITION`, else the first `VERSION`, of the containment.
fn versioned_variable(from: &ContainsExpr) -> Option<String> {
    let mut composition = None;
    let mut version = None;
    classes(from, &mut |operand| match operand {
        ClassExprOperand::Class {
            rm_type,
            variable: Some(variable),
            ..
        } if rm_type == "COMPOSITION" => {
            composition.get_or_insert_with(|| variable.clone());
        }
        ClassExprOperand::Version {
            variable: Some(variable),
            ..
        } => {
            version.get_or_insert_with(|| variable.clone());
        }
        ClassExprOperand::Class { .. } | ClassExprOperand::Version { .. } => {}
    });
    composition.or(version)
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
    IdentifiedPath::new(
        variable.to_owned(),
        None,
        Some(ObjectPath {
            parts: ["uid", "value"]
                .into_iter()
                .map(|name| PathPart {
                    name: name.to_owned(),
                    predicate: None,
                })
                .collect(),
        }),
    )
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
        // Decision A3: AQL admits the query, and N7 requires the node scope.
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
