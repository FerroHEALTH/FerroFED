// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated merge of node answers (feature `merge`): `ORDER BY` with
//! `LIMIT` re-applied at the Tier with a deterministic tie-break (§11.6.1,
//! N13, N39; `docs/architecture.md` section 9).
//!
//! Per-node `ORDER BY` and `LIMIT` alone are not a federated answer (N9).
//! Every node is sent the client's `LIMIT n`; [`merge`] orders the rows of
//! every node under one Tier comparator and keeps the first `n` (§11.6.1
//! MUST). The row order is the `ORDER BY` keys in turn, then the endpoint id,
//! then the tie-break columns (the row's uid, §11.6.1 RECOMMENDED), then the
//! row's cells by canonical JSON, so a repeated query returns the same rows in
//! the same order.
//!
//! §11.6.1 makes `LIMIT n` per node correct "under a total order". AQL fixes no
//! total order for nulls, collation or data values, so a node may order
//! differently than the Tier. The merge checks what it can see (FerroFED's
//! own): a node that returned `n` rows, and so may have been cut, must have
//! returned them in the Tier order on the keys and tie-break it was sent. A
//! node that fails is refused, and the gateway reports it `node-error` ("a
//! response the gateway could not use", §11.1). A node that returned fewer
//! rows returned all it matched, so its order cannot hide a row.
//!
//! The check sees only the rows a node returned. A node that orders
//! differently from the Tier past its cut (a locale collation that ranks a row
//! the Tier puts first behind the `n` it returned) passes the check, and its
//! top `n` is then not the Tier's top `n`.
//!
//! # Examples
//!
//! ```
//! use openehr_federation::merge::{NodeAnswer, merge};
//! use openehr_federation::order::{Direction, ResultOrder, SortKey};
//! use serde_json::json;
//!
//! // The façade asked for `ORDER BY` column 0 `LIMIT 2`; each node was sent
//! // `LIMIT 2` with the uid (column 1) as the last key.
//! let order = ResultOrder::new(vec![SortKey::new(0, Direction::Ascending)], vec![1], Some(2));
//! let a = NodeAnswer::new("node-a", vec![vec![json!(1), json!("a1")], vec![json!(4), json!("a2")]]);
//! let b = NodeAnswer::new("node-b", vec![vec![json!(2), json!("b1")], vec![json!(3), json!("b2")]]);
//! let merged = merge(vec![b, a], &order);
//! assert!(merged.refused().is_empty());
//! assert_eq!(merged.rows(), [vec![json!(1), json!("a1")], vec![json!(2), json!("b1")]]);
//! ```

mod cell;
mod compare;

use std::cmp::Ordering;
use std::fmt;

use openehr_its::rest::generated::query::ResultSetRow;

use crate::order::{Direction, ResultOrder};
use cell::{Cell, canonical, decode};
use compare::{cmp_cell, rank_classes};

/// The rows one endpoint answered.
#[derive(Debug, Clone)]
pub struct NodeAnswer {
    endpoint: String,
    rows: Vec<ResultSetRow>,
}

impl NodeAnswer {
    /// The `rows` endpoint `endpoint` answered, in the order it sent them.
    #[must_use]
    pub fn new(endpoint: impl Into<String>, rows: Vec<ResultSetRow>) -> Self {
        Self {
            endpoint: endpoint.into(),
            rows,
        }
    }
}

/// The merged answer: the rows in the Tier order, cut at the `LIMIT`, and the
/// endpoints whose answer could not be used.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    rows: Vec<ResultSetRow>,
    refused: Vec<Refused>,
}

impl Merged {
    /// The merged rows, with every node column, hidden ones included.
    #[must_use]
    pub fn rows(&self) -> &[ResultSetRow] {
        &self.rows
    }

    /// The endpoints whose answer the merge refused, in endpoint id order;
    /// none of their rows is in [`Merged::rows`].
    #[must_use]
    pub fn refused(&self) -> &[Refused] {
        &self.refused
    }

    /// The rows and the refusals.
    #[must_use]
    pub fn into_parts(self) -> (Vec<ResultSetRow>, Vec<Refused>) {
        (self.rows, self.refused)
    }
}

/// An endpoint whose answer the merge could not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    endpoint: String,
    reason: Disagreement,
}

impl Refused {
    /// The endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Why its answer could not be used.
    #[must_use]
    pub fn reason(&self) -> Disagreement {
        self.reason
    }
}

/// Why a node's answer cannot be merged into a correct federated answer
/// (FerroFED's own vocabulary inside the specification's `node-error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Disagreement {
    /// A node that returned `n` rows returned them out of the Tier order on
    /// the keys and tie-break it was sent.
    Order,
    /// The node returned more rows than the `LIMIT` it was sent.
    PastTheLimit,
    /// A row lacks a column the Tier order reads.
    ShortRow,
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Order => "result order disagrees with the federation order",
            Self::PastTheLimit => "the node returned more rows than the LIMIT it was sent",
            Self::ShortRow => "a row lacks a column the federation order reads",
        })
    }
}

/// One row as the comparator reads it.
#[derive(Debug)]
struct Decoded {
    keys: Vec<Cell>,
    tie: Vec<Cell>,
    cells: Vec<String>,
}

/// One accepted row: its endpoint, its decoded form, and the row itself.
type Placed = (String, Decoded, ResultSetRow);

/// Merges the answers of the nodes under `order` (§11.6.1, N13, N39).
///
/// With no `ORDER BY` key, the rows are the nodes' rows in endpoint id order,
/// cut at the limit: any `limit` rows of the union answer a query that fixes
/// no order. With keys, every node that returned `limit` rows is checked, the
/// rows of the nodes that pass are put in the Tier order, and the first
/// `limit` are kept. The result does not depend on the order of `nodes`.
#[must_use]
pub fn merge(mut nodes: Vec<NodeAnswer>, order: &ResultOrder) -> Merged {
    nodes.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    if order.keys().is_empty() {
        let mut rows: Vec<ResultSetRow> = nodes.into_iter().flat_map(|node| node.rows).collect();
        cut(&mut rows, order.limit());
        return Merged {
            rows,
            refused: Vec::new(),
        };
    }
    let mut refused = Vec::new();
    let mut decoded: Vec<(String, Vec<(Decoded, ResultSetRow)>)> = Vec::new();
    for node in nodes {
        match decode_node(node.rows, order) {
            Ok(rows) => decoded.push((node.endpoint, rows)),
            Err(reason) => refused.push(Refused {
                endpoint: node.endpoint,
                reason,
            }),
        }
    }
    rank_classes(decoded.iter_mut().flat_map(|(_, rows)| {
        rows.iter_mut().flat_map(|(row, _)| {
            row.keys
                .iter_mut()
                .chain(row.tie.iter_mut())
                .filter_map(|cell| match cell {
                    Cell::Data(data) => Some(&mut **data),
                    _ => None,
                })
        })
    }));
    let mut accepted: Vec<Placed> = Vec::new();
    for (endpoint, rows) in decoded {
        match check(&rows, order) {
            Ok(()) => accepted.extend(
                rows.into_iter()
                    .map(|(row, raw)| (endpoint.clone(), row, raw)),
            ),
            Err(reason) => refused.push(Refused { endpoint, reason }),
        }
    }
    refused.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    accepted.sort_by(|a, b| tier(a, b, order));
    let mut rows: Vec<ResultSetRow> = accepted.into_iter().map(|(_, _, raw)| raw).collect();
    cut(&mut rows, order.limit());
    Merged { rows, refused }
}

/// Decodes the cells the order reads from every row of one node.
fn decode_node(
    rows: Vec<ResultSetRow>,
    order: &ResultOrder,
) -> Result<Vec<(Decoded, ResultSetRow)>, Disagreement> {
    rows.into_iter()
        .map(|raw| {
            let keys = order
                .keys()
                .iter()
                .map(|key| raw.get(key.column()).map(decode))
                .collect::<Option<Vec<Cell>>>()
                .ok_or(Disagreement::ShortRow)?;
            let tie = order
                .tie_break()
                .iter()
                .map(|column| raw.get(*column).map(decode))
                .collect::<Option<Vec<Cell>>>()
                .ok_or(Disagreement::ShortRow)?;
            let cells = raw.iter().map(canonical).collect();
            Ok((Decoded { keys, tie, cells }, raw))
        })
        .collect()
}

/// The check of one node's visible order (FerroFED's own, within §11.6.1).
fn check(rows: &[(Decoded, ResultSetRow)], order: &ResultOrder) -> Result<(), Disagreement> {
    let Some(limit) = order.limit() else {
        return Ok(());
    };
    // NOTE: no specification governs this: our own design; no answer in memory
    // reaches usize::MAX rows, so a larger limit cuts nothing.
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if rows.len() > limit {
        return Err(Disagreement::PastTheLimit);
    }
    if rows.len() < limit {
        return Ok(());
    }
    for pair in rows.windows(2) {
        if let [(a, _), (b, _)] = pair
            && dispatched(a, b, order) == Ordering::Greater
        {
            return Err(Disagreement::Order);
        }
    }
    Ok(())
}

/// The order a node was sent: the keys, then the tie-break ascending.
fn dispatched(a: &Decoded, b: &Decoded, order: &ResultOrder) -> Ordering {
    keys(a, b, order).then_with(|| tie(a, b))
}

/// The Tier order: the keys, the endpoint, the tie-break, then the cells.
fn tier(a: &Placed, b: &Placed, order: &ResultOrder) -> Ordering {
    keys(&a.1, &b.1, order)
        .then_with(|| a.0.cmp(&b.0))
        .then_with(|| tie(&a.1, &b.1))
        .then_with(|| a.1.cells.cmp(&b.1.cells))
}

fn keys(a: &Decoded, b: &Decoded, order: &ResultOrder) -> Ordering {
    for ((x, y), key) in a.keys.iter().zip(&b.keys).zip(order.keys()) {
        let ordering = match key.direction() {
            Direction::Ascending => cmp_cell(x, y),
            Direction::Descending => cmp_cell(x, y).reverse(),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn tie(a: &Decoded, b: &Decoded) -> Ordering {
    a.tie
        .iter()
        .zip(&b.tie)
        .map(|(x, y)| cmp_cell(x, y))
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn cut(rows: &mut Vec<ResultSetRow>, limit: Option<u64>) {
    if let Some(limit) = limit {
        // NOTE: no specification governs this: our own design; a limit past
        // usize::MAX keeps every row.
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
}
