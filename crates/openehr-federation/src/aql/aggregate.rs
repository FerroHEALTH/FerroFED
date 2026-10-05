// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Aggregates across a fan-out (§11.6.3, N14, N39): a declared decomposable
//! function is dispatched to every node and recombined at the Tier, `AVG` as
//! the `SUM` and the `COUNT` of its path, and anything else is refused.

use std::ops::Range;

use openehr_query::ast::{AggregateCall, ColumnExpr, SelectExpr, SelectQuery, StatFunc};

use super::Context;
use super::refusal::{Indecomposable, Refusal};
use crate::aggregate::{AggregateFunction, Recombination, Recombine};
use crate::dedup::DedupMode;

/// Rewrites the select list of an undirected aggregate query into the one
/// every node is sent, and returns how the Tier recombines the node rows.
///
/// Every column must be an aggregate whose function the context declares
/// decomposable, with no `DISTINCT`, no `COUNT(DISTINCT …)` and no dedup mode
/// (§11.6.3). `COUNT`, `SUM`, `MIN` and
/// `MAX` are dispatched as written. `AVG(x)` is dispatched as `SUM(x)` and
/// `COUNT(x)`, the per-node counts §11.6.3 requires before `AVG` may be
/// decomposed; both ignore `NULL` as `AVG` does (AQL 1.1.0 §Aggregate
/// functions), so their quotient is the mean.
///
/// # Errors
/// [`Refusal::UndirectedAggregate`] when the context does not declare a
/// function the query applies, at `at` when it declares none, and
/// [`Refusal::Indecomposable`] for a query that breaks the decomposition.
pub(super) fn decompose(
    query: &mut SelectQuery,
    context: &Context,
    at: Option<Range<usize>>,
) -> Result<Recombination, Refusal> {
    let declared = &context.decomposable;
    if declared.is_empty() {
        return Err(Refusal::UndirectedAggregate { at });
    }
    for column in &query.select.columns {
        if let ColumnExpr::Aggregate(call) = &column.column
            && !declared.contains(&function(call))
        {
            return Err(Refusal::UndirectedAggregate { at: written(call) });
        }
    }
    if query.select.distinct {
        return Err(Refusal::Indecomposable {
            reason: Indecomposable::Distinct,
            at,
        });
    }
    // NOTE: §11.6.3, a decomposable aggregate "must not be combined with … de-duplication
    // (§10)": a node's count includes the copies the Tier would suppress.
    if context.dedup != DedupMode::None {
        return Err(Refusal::Indecomposable {
            reason: Indecomposable::Dedup,
            at,
        });
    }
    let columns = std::mem::take(&mut query.select.columns);
    let mut node: Vec<SelectExpr> = Vec::with_capacity(columns.len());
    let mut recombined = Vec::with_capacity(columns.len());
    for column in columns {
        let call = match column.column {
            ColumnExpr::Aggregate(call) => call,
            ColumnExpr::Path(path) => {
                return Err(Refusal::Indecomposable {
                    reason: Indecomposable::PlainColumn,
                    at: path.span.bytes(),
                });
            }
            ColumnExpr::Primitive(_) | ColumnExpr::Function(_) => {
                return Err(Refusal::Indecomposable {
                    reason: Indecomposable::PlainColumn,
                    at: None,
                });
            }
        };
        let index = node.len();
        let recombine = match &call {
            AggregateCall::Count { distinct: true, .. } => {
                return Err(Refusal::Indecomposable {
                    reason: Indecomposable::CountDistinct,
                    at: written(&call),
                });
            }
            AggregateCall::Count {
                distinct: false, ..
            } => Recombine::Count { column: index },
            AggregateCall::Stat {
                func: StatFunc::Sum,
                ..
            } => Recombine::Sum { column: index },
            AggregateCall::Stat {
                func: StatFunc::Min,
                ..
            } => Recombine::Min { column: index },
            AggregateCall::Stat {
                func: StatFunc::Max,
                ..
            } => Recombine::Max { column: index },
            AggregateCall::Stat {
                func: StatFunc::Avg,
                path,
            } => {
                node.push(aggregate(AggregateCall::Stat {
                    func: StatFunc::Sum,
                    path: path.clone(),
                }));
                node.push(aggregate(AggregateCall::Count {
                    distinct: false,
                    path: Some(path.clone()),
                }));
                recombined.push(Recombine::Avg {
                    sum: index,
                    count: index.saturating_add(1),
                });
                continue;
            }
        };
        node.push(SelectExpr {
            column: ColumnExpr::Aggregate(call),
            alias: column.alias,
        });
        recombined.push(recombine);
    }
    query.select.columns = node;
    Ok(Recombination::new(recombined))
}

/// The function an aggregate call applies.
fn function(call: &AggregateCall) -> AggregateFunction {
    match call {
        AggregateCall::Count { .. } => AggregateFunction::Count,
        AggregateCall::Stat { func, .. } => match func {
            StatFunc::Sum => AggregateFunction::Sum,
            StatFunc::Min => AggregateFunction::Min,
            StatFunc::Max => AggregateFunction::Max,
            StatFunc::Avg => AggregateFunction::Avg,
        },
    }
}

/// Where an aggregate's path was written, `None` for `COUNT(*)`.
fn written(call: &AggregateCall) -> Option<Range<usize>> {
    match call {
        AggregateCall::Count { path, .. } => path.as_ref().and_then(|path| path.span.bytes()),
        AggregateCall::Stat { path, .. } => path.span.bytes(),
    }
}

/// A dispatched column with no alias: the node's column names are never the
/// client's (N17, §9.2).
fn aggregate(call: AggregateCall) -> SelectExpr {
    SelectExpr {
        column: ColumnExpr::Aggregate(call),
        alias: None,
    }
}
