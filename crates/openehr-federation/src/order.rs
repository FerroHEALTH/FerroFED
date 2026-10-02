// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the rows a node query returns are ordered and cut at the Tier
//! (§11.6.1, §11.6.2, N13, N39).
//!
//! The rewrite of the `aql` feature describes, for the node query it builds,
//! which node columns carry the `ORDER BY` keys, which carry the tie-break
//! after `endpoint_id`, the `LIMIT` every node was sent, and the rows the Tier
//! skips for an `OFFSET`; the merge of the `merge` feature reads the same
//! description. It is plain data, so neither feature
//! depends on the other.

/// The direction of one `ORDER BY` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `ASC`, the default.
    Ascending,
    /// `DESC`.
    Descending,
}

/// One `ORDER BY` key: the node column it reads, and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    column: usize,
    direction: Direction,
}

impl SortKey {
    /// The key reading node column `column` in `direction`.
    #[must_use]
    pub fn new(column: usize, direction: Direction) -> Self {
        Self { column, direction }
    }

    /// The node column the key reads.
    #[must_use]
    pub fn column(&self) -> usize {
        self.column
    }

    /// The key's direction.
    #[must_use]
    pub fn direction(&self) -> Direction {
        self.direction
    }
}

/// The Tier order of a federated answer, its `LIMIT` and its `OFFSET`.
///
/// Every node was sent one `LIMIT`, ordered by the keys and then by the
/// tie-break columns ascending: the client's `n`, or `k + n` for a page that
/// starts at `OFFSET k` (§11.6.2). The Tier orders the merged rows by the
/// keys, then `endpoint_id`, then the tie-break columns (§11.6.1: "RECOMMENDED:
/// `endpoint_id`, then uid"), keeps the first [`ResultOrder::limit`] and drops
/// the first [`ResultOrder::offset`] of those, so the answer is the rows
/// `[k, k + n)`. With no keys, any `n` rows of the union are a correct answer
/// to a query with no `OFFSET`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultOrder {
    keys: Vec<SortKey>,
    tie_break: Vec<usize>,
    limit: Option<u64>,
    offset: u64,
}

impl ResultOrder {
    /// An answer with no Tier order and no limit: the rows of the nodes in
    /// endpoint order.
    #[must_use]
    pub fn unordered() -> Self {
        Self::default()
    }

    /// An answer ordered by `keys`, tie-broken after `endpoint_id` on the node
    /// columns `tie_break` ascending, and cut at `limit` rows.
    #[must_use]
    pub fn new(keys: Vec<SortKey>, tie_break: Vec<usize>, limit: Option<u64>) -> Self {
        Self {
            keys,
            tie_break,
            limit,
            offset: 0,
        }
    }

    /// This answer starting at row `offset` of the Tier order: the merge keeps
    /// the first [`ResultOrder::limit`] rows and drops the first `offset` of
    /// them (§11.6.2).
    ///
    /// For the page `LIMIT n OFFSET k`, every node was sent `LIMIT k + n`, so
    /// `limit` is `k + n` and `offset` is `k`.
    #[must_use]
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }

    /// The `ORDER BY` keys, in order.
    #[must_use]
    pub fn keys(&self) -> &[SortKey] {
        &self.keys
    }

    /// The node columns that break a tie after `endpoint_id`, ascending: the
    /// row's uid, or under `DISTINCT` the selected columns the keys do not
    /// read.
    #[must_use]
    pub fn tie_break(&self) -> &[usize] {
        &self.tie_break
    }

    /// The `LIMIT` every node was sent: the façade's `n`, or `k + n` for a
    /// page that starts at `OFFSET k` (§11.6.1, §11.6.2).
    #[must_use]
    pub fn limit(&self) -> Option<u64> {
        self.limit
    }

    /// The rows the Tier drops from the front of the cut answer: the façade's
    /// `OFFSET k`, zero when the query pages from the first row (§11.6.2).
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }
}
