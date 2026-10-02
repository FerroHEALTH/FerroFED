// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated merge of node answers (feature `merge`).
//!
//! `ORDER BY` with `LIMIT` is re-applied at the Tier with a deterministic
//! tie-break (§11.6.1, N13, N39), and `LIMIT n OFFSET k` is sliced from the
//! merged order (§11.6.2). Per-node `ORDER BY` and `LIMIT` alone are not a
//! federated answer (N9). Every node is sent the client's `LIMIT n`; [`merge`]
//! orders the rows of every node under one Tier comparator and keeps the first
//! `n` (§11.6.1 MUST). The row order is the `ORDER BY` keys in turn, then the
//! endpoint id, then the tie-break columns (the row's uid, §11.6.1
//! RECOMMENDED, or the `ehr_id` of a row with no uid), then the row's cells by
//! canonical JSON, so a repeated query returns the same rows in the same
//! order. For a page at `OFFSET k`, every node is sent `LIMIT k + n`
//! with no `OFFSET`, and the merge keeps the rows `[k, k + n)` of the Tier
//! order: the global first `k + n` rows lie in the union of every node's first
//! `k + n`, so the slice is the global page (§11.6.2, "retrieving `k + n` rows
//! per node, merging, ordering and slicing").
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
//! Under `SELECT DISTINCT` the Tier keeps one row of every set of rows equal
//! in the columns the client selected (N13), the first in the Tier order, and
//! only then cuts at `LIMIT` and `OFFSET` (AQL 1.1.0 §LIMIT). A node that may
//! have been cut and returned two rows the Tier holds equal is refused like
//! one out of order: its cut can hide a distinct row.
//!
//! Every node is still sent the client's `LIMIT n` (or `k + n` for a page),
//! and the global distinct top `n` lies in the union of the nodes' answers.
//! Take a value of it, and the node holding its kept copy, the copy first in
//! the Tier order. Under `DISTINCT` the keys and the tie-break are the
//! selected paths, and the rewrite refuses a `LIMIT` whose other selected
//! columns those paths do not fix (`unordered-distinct-cut`), so two distinct
//! values differ on a key and every value that node orders ahead of the copy
//! is also ahead of it in the Tier order. There are fewer than `n` of those,
//! so the value is within the node's distinct top `n`, and the node returned
//! it.
//!
//! Under version-identity dedup (§10.2) the rows of a version held at several
//! endpoints are suppressed at every endpoint but the one kept, before
//! `DISTINCT` and the cut, and a tie on the keys is broken by the tie-break
//! columns, the version uid among them, before the endpoint id. That order is
//! what keeps `LIMIT n` per node exact. Take a row of the global top `n` of
//! the deduplicated union, and its node. A row that node orders ahead of it is
//! either kept, and then ahead of it in the Tier order, or a copy of a version
//! kept at another endpoint. One version id names one immutable version, so
//! the kept copy has the same keys and uid, and ranks ahead of the row too.
//! Fewer than `n` kept rows are ahead of it, so the node returned it, and the
//! endpoint that keeps a version of the top `n` returned its copy. With the
//! endpoint id before the uid, a copy tied on the keys could take the slot of
//! a row that belongs in the top `n`.
//!
//! Two copies of one version can spell its id in different cases, which BASE
//! holds to be one identifier (`master05-identification_package.adoc`
//! §"Composite Identifiers and Case"). Under dedup the Tier therefore reads
//! the version uid by `openehr-base`'s `composite_id_key`, as a key and as a
//! tie-break column, so the copies rank as one, and every row keeps its text
//! as its node sent it. The argument then holds for a node whose own order on
//! the uid ignores case as well. The Tier cannot change how a node sorts the
//! `ORDER BY` on the uid it is sent. A node that orders uids byte for byte
//! and returned `n` rows in an order this one disagrees with is refused, and
//! one whose order differs only past its cut passes the check, as above.
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

mod aggregate;
mod cell;
mod compare;
mod dedup;
mod distinct;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use openehr_base::v1_3::base_types::identification::lexical::composite_id_key;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::query::ResultSetRow;

use crate::aggregate::Recombination;
use crate::dedup::DedupMode;
use crate::error::WireError;
use crate::id::EndpointId;
use crate::meta::DedupRecord;
use crate::order::{Direction, ResultOrder};
use cell::{Cell, canonical, decode};
use compare::{cmp_cell, rank_classes};

/// The rows one endpoint answered.
#[derive(Debug, Clone)]
pub struct NodeAnswer {
    endpoint: String,
    system_id: Option<String>,
    rows: Vec<ResultSetRow>,
}

impl NodeAnswer {
    /// The `rows` endpoint `endpoint` answered, in the order it sent them.
    #[must_use]
    pub fn new(endpoint: impl Into<String>, rows: Vec<ResultSetRow>) -> Self {
        Self {
            endpoint: endpoint.into(),
            system_id: None,
            rows,
        }
    }

    /// This answer from an endpoint whose node has the openEHR `system_id`
    /// `system_id`: under version-identity dedup, its copy of a version whose
    /// `creating_system_id` is `system_id` is the originating copy (§10.2).
    #[must_use]
    pub fn with_system_id(mut self, system_id: impl Into<String>) -> Self {
        self.system_id = Some(system_id.into());
        self
    }
}

/// The merged answer: the rows in the Tier order, cut at the `LIMIT`, the
/// endpoints whose answer could not be used, and what dedup suppressed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Merged {
    rows: Vec<ResultSetRow>,
    refused: Vec<Refused>,
    suppressed: Suppressed,
}

/// The rows version-identity dedup suppressed (§10.2, §10.3, N36).
///
/// Every row of a copy dropped for another endpoint's copy of the same
/// version counts, before `DISTINCT`, `OFFSET` and `LIMIT`, so the endpoints'
/// `row_count`s reconcile with the answer (§9.5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suppressed {
    rows: u64,
    endpoints: Vec<String>,
}

impl Suppressed {
    /// How many rows were suppressed.
    #[must_use]
    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// The endpoints whose copies were dropped, in endpoint id order, each
    /// once.
    #[must_use]
    pub fn endpoints(&self) -> &[String] {
        &self.endpoints
    }

    /// The `meta.federation.dedup` record of an answer under `mode` (§10.2):
    /// the mode always, and under version-identity the count of suppressed
    /// rows, with the endpoints whose copies were dropped when there are any
    /// (§10.3).
    ///
    /// # Errors
    /// Returns [`WireError::EmptyMember`] when a suppressed endpoint id is
    /// empty.
    pub fn record(&self, mode: DedupMode) -> Result<DedupRecord, WireError> {
        let mut record = mode.record();
        if mode == DedupMode::VersionIdentity {
            record.suppressed_rows = Some(self.rows);
            if !self.endpoints.is_empty() {
                let endpoints = self
                    .endpoints
                    .iter()
                    .map(|endpoint| EndpointId::new(endpoint.as_str()))
                    .collect::<Result<Vec<_>, _>>()?;
                record.suppressed_endpoints = Some(endpoints);
            }
        }
        Ok(record)
    }
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

    /// What version-identity dedup suppressed; nothing under any other mode.
    #[must_use]
    pub fn suppressed(&self) -> &Suppressed {
        &self.suppressed
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
    /// A row lacks a column the Tier order or a recombined aggregate reads.
    ShortRow,
    /// An aggregate query answers one row (AQL 1.1.0 §Aggregate functions:
    /// "a single result based on a group of rows"), and the node answered
    /// another number of rows.
    AggregateRows,
    /// A node value cannot take part in an exactly correct aggregate
    /// (§11.6.3): a count that is not an integer, a sum that is not a number,
    /// a minimum or maximum that is neither a number nor a complete date-time,
    /// or an `AVG` whose sum and count disagree.
    AggregateValue,
    /// The nodes' `MIN` or `MAX` values are of kinds no one order compares
    /// (a number and a date-time, or a zoned and an unzoned date-time).
    AggregateKinds,
    /// Under `SELECT DISTINCT`, a node that returned `n` rows returned two
    /// that the Tier holds equal, so a distinct row can lie past its cut.
    Distinct,
    /// Under version-identity dedup, a row's version uid is neither `null`
    /// nor an `OBJECT_VERSION_ID` (§10.2).
    VersionId,
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Order => "result order disagrees with the federation order",
            Self::PastTheLimit => "the node returned more rows than the LIMIT it was sent",
            Self::ShortRow => "a row lacks a column the federation order reads",
            Self::AggregateRows => "an aggregate query answers one row, and the node did not",
            Self::AggregateValue => {
                "the node's aggregate value cannot be recombined into an exactly correct answer"
            }
            Self::AggregateKinds => {
                "the nodes' MIN or MAX values are of kinds that cannot be compared with each other"
            }
            Self::Distinct => {
                "the node returned, at its LIMIT, two rows the federation holds equal under DISTINCT"
            }
            Self::VersionId => "a row's version uid is not an OBJECT_VERSION_ID",
        })
    }
}

/// A recombined aggregate the gateway cannot write exactly (§11.6.3: "the
/// result must be exactly correct").
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the recombined aggregate of column {column} cannot be written exactly as a JSON number")]
pub struct Unrepresentable {
    column: usize,
}

impl Unrepresentable {
    /// The façade column whose value cannot be written.
    #[must_use]
    pub fn column(self) -> usize {
        self.column
    }
}

/// One row as the comparator reads it.
#[derive(Debug)]
struct Decoded {
    keys: Vec<Cell>,
    tie: Vec<Cell>,
    distinct: Vec<Cell>,
    version: Option<ObjectVersionId>,
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
/// `limit` are kept. Either way, the first [`ResultOrder::offset`] of the kept
/// rows are then dropped (§11.6.2). The result does not depend on the order of
/// `nodes`.
///
/// Under [`ResultOrder::distinct`], the rows equal on the distinct columns
/// are collapsed to the first of them in that order before the cut (N13, AQL
/// 1.1.0 §LIMIT), and a node that returned `limit` rows two of which are
/// equal is refused with [`Disagreement::Distinct`].
///
/// Under [`ResultOrder::version_key`], the rows of the nodes that pass are
/// deduplicated first (§10.2): of the endpoints holding one
/// `OBJECT_VERSION_ID`, only one keeps its rows, the one whose
/// [`NodeAnswer::with_system_id`] is the version's `creating_system_id`, else
/// the lowest endpoint id, with identifiers that differ only in case taken as
/// one (BASE `master05-identification_package.adoc` §"Composite Identifiers
/// and Case"). A tie on the keys is then broken by the tie-break columns
/// before the endpoint id, and the version uid, as a key or a tie-break
/// column, is ordered by `openehr-base`'s `composite_id_key`, so two copies
/// of one version rank as one. A node with a version uid that is not an
/// `OBJECT_VERSION_ID` is refused with [`Disagreement::VersionId`].
#[must_use]
pub fn merge(mut nodes: Vec<NodeAnswer>, order: &ResultOrder) -> Merged {
    nodes.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    if order.keys().is_empty() && order.distinct().is_none() && order.version_key().is_none() {
        let mut rows: Vec<ResultSetRow> = nodes.into_iter().flat_map(|node| node.rows).collect();
        cut(&mut rows, order);
        return Merged {
            rows,
            ..Merged::default()
        };
    }
    let systems: BTreeMap<String, String> = nodes
        .iter()
        .filter_map(|node| Some((node.endpoint.clone(), node.system_id.clone()?)))
        .collect();
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
                .chain(row.distinct.iter_mut())
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
    let mut suppressed = Suppressed::default();
    if order.version_key().is_some() {
        (accepted, suppressed) = dedup::suppress(accepted, &systems);
    }
    // NOTE: no specification governs this: our own design; with no `ORDER BY`
    // key the rows stay in endpoint id order, as the nodes sent them.
    if !order.keys().is_empty() {
        accepted.sort_by(|a, b| tier(a, b, order));
    }
    // NOTE: §11.6.1, AQL 1.1.0 §LIMIT: a value of the global distinct top `n` has fewer
    // than `n` values ahead of its kept copy at that copy's node, so that node returned it.
    if order.distinct().is_some() {
        accepted = distinct::collapse(accepted, |(_, row, _)| row.distinct.as_slice());
    }
    let mut rows: Vec<ResultSetRow> = accepted.into_iter().map(|(_, _, raw)| raw).collect();
    cut(&mut rows, order);
    Merged {
        rows,
        refused,
        suppressed,
    }
}

/// Recombines the one-row answers of an aggregate query into the
/// federation's row (§11.6.3, N14, N39).
///
/// Each node was sent the aggregate query the rewrite wrote, and answers it
/// with one row. `COUNT` is the sum of the node counts; `SUM` the sum of the
/// node sums that are not `NULL`, or `NULL`; `MIN` and `MAX` the node value the
/// Tier comparator puts first or last, as the node wrote it, over numbers or
/// complete date-times only; and `AVG` the sum of the node sums over the sum of
/// the node counts, or `NULL` when no node counted a value, nulls ignored
/// throughout as AQL ignores them (AQL 1.1.0 §Aggregate functions). Integers
/// add exactly, and reals add in decimal arithmetic. The mean is the decimal
/// quotient to 28 significant digits, written as the nearest JSON number.
///
/// A node whose answer cannot take part in an exactly correct value is
/// refused, and then no row is returned at all: a recombination over some of
/// the answers would be a wrong value (§11.6.3). With no answer to recombine,
/// there is no row either, as for a query in which no node answered (§11.3).
/// The row is then cut at the order's `LIMIT` and `OFFSET`.
///
/// # Errors
/// Returns [`Unrepresentable`] when the recombined value cannot be written
/// exactly: a count or an integer sum past `u64`, or a sum of reals the
/// decimal cannot hold or no JSON number reads back as.
pub fn combine(
    mut nodes: Vec<NodeAnswer>,
    recombination: &Recombination,
    order: &ResultOrder,
) -> Result<Merged, Unrepresentable> {
    nodes.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    // NOTE: AQL 1.1.0 §LIMIT, at most `row_count` rows: the node is sent the
    // client's LIMIT, so LIMIT 0 asks it for none.
    let expected = usize::from(order.limit() != Some(0));
    let mut refused = Vec::new();
    let mut answers = Vec::new();
    for node in nodes {
        match aggregate::validate(node.endpoint, node.rows, recombination, expected) {
            Ok(Some(answer)) => answers.push(answer),
            Ok(None) => {}
            Err(refusal) => refused.push(refusal),
        }
    }
    refused.extend(aggregate::incomparable(&answers, recombination));
    refused.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    if !refused.is_empty() || answers.is_empty() {
        return Ok(Merged {
            refused,
            ..Merged::default()
        });
    }
    let mut rows = vec![aggregate::recombine(&answers, recombination)?];
    cut(&mut rows, order);
    Ok(Merged {
        rows,
        refused,
        ..Merged::default()
    })
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
                .map(|key| ordered(&raw, key.column(), order))
                .collect::<Option<Vec<Cell>>>()
                .ok_or(Disagreement::ShortRow)?;
            let tie = order
                .tie_break()
                .iter()
                .map(|column| ordered(&raw, *column, order))
                .collect::<Option<Vec<Cell>>>()
                .ok_or(Disagreement::ShortRow)?;
            let distinct = order
                .distinct()
                .unwrap_or_default()
                .iter()
                .map(|column| raw.get(*column).map(decode))
                .collect::<Option<Vec<Cell>>>()
                .ok_or(Disagreement::ShortRow)?;
            // NOTE: our own design; the node's malformed value is not carried, since the
            // refusal names the defect and a node-error never quotes the node's data.
            let version = match order.version_key() {
                Some(column) => cell::version(raw.get(column).ok_or(Disagreement::ShortRow)?)
                    .map_err(|_not_a_version| Disagreement::VersionId)?,
                None => None,
            };
            let cells = raw.iter().map(canonical).collect();
            Ok((
                Decoded {
                    keys,
                    tie,
                    distinct,
                    version,
                    cells,
                },
                raw,
            ))
        })
        .collect()
}

/// The cell of `raw` at `column` as the Tier order reads it, or `None` when
/// the row is too short.
///
/// Under version-identity dedup the version uid is read by its BASE
/// comparison key, so the copies of one version whose ids differ only in case
/// rank as one, in the Tier order and in the check of a node's order. The row
/// itself keeps its text as the node sent it.
// NOTE: BASE master05 §"Composite Identifiers and Case", §11.6.1: outside dedup the uid orders as
// sent, since a node that changes a uid's case is outside the containment argument of §11.6.1.
fn ordered(raw: &ResultSetRow, column: usize, order: &ResultOrder) -> Option<Cell> {
    let cell = decode(raw.get(column)?);
    Some(match cell {
        Cell::Text(uid) if order.version_key() == Some(column) => {
            Cell::Text(composite_id_key(&uid))
        }
        other => other,
    })
}

/// The check of one node's visible order, and under `DISTINCT` of its
/// visible duplicates (FerroFED's own, within §11.6.1 and N13).
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
    if order.distinct().is_some() {
        let tuples: Vec<&[Cell]> = rows
            .iter()
            .map(|(row, _)| row.distinct.as_slice())
            .collect();
        if distinct::has_duplicates(&tuples) {
            return Err(Disagreement::Distinct);
        }
    }
    Ok(())
}

/// The order a node was sent: the keys, then the tie-break ascending.
fn dispatched(a: &Decoded, b: &Decoded, order: &ResultOrder) -> Ordering {
    keys(a, b, order).then_with(|| tie(a, b))
}

/// The Tier order: the keys, the endpoint, the tie-break, then the cells;
/// under version-identity dedup the tie-break comes before the endpoint.
// NOTE: §11.6.1 asks only for a stable secondary key (endpoint_id first is RECOMMENDED);
// under dedup the uid ranks a version's copies as one, so the kept copy lies in its node's cut.
fn tier(a: &Placed, b: &Placed, order: &ResultOrder) -> Ordering {
    let endpoint = || a.0.cmp(&b.0);
    let keyed = keys(&a.1, &b.1, order);
    match order.version_key() {
        Some(_) => keyed.then_with(|| tie(&a.1, &b.1)).then_with(endpoint),
        None => keyed.then_with(endpoint).then_with(|| tie(&a.1, &b.1)),
    }
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

/// Keeps the rows `[offset, limit)` of the ordered rows (§11.6.1, §11.6.2).
fn cut(rows: &mut Vec<ResultSetRow>, order: &ResultOrder) {
    if let Some(limit) = order.limit() {
        // NOTE: no specification governs this: our own design; a limit past
        // usize::MAX keeps every row.
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    // NOTE: no specification governs this: our own design; an offset past
    // usize::MAX skips every row, as an offset past the row count does.
    let skip = usize::try_from(order.offset()).unwrap_or(usize::MAX);
    rows.drain(..skip.min(rows.len()));
}
