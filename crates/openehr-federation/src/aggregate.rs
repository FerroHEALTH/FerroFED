// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the answers of an aggregate query are recombined at the Tier into the
//! federation's one row (§11.6.3, N14, N39).
//!
//! A gateway "MAY also support *decomposable* aggregates over multiple nodes:
//! `COUNT` and `SUM` by summing the per-node results, `MIN`/`MAX` by
//! re-applying across them", and "`AVG` MUST NOT be decomposed this way unless
//! the gateway also retrieves the per-node counts" (§11.6.3). The rewrite of
//! the `aql` feature describes, for each column of the façade query, which
//! node columns carry the per-node results and how they combine; the merge of
//! the `merge` feature reads the same description. It is plain data, so
//! neither feature depends on the other.

/// An AQL aggregate function (AQL 1.1.0 §Aggregate functions), by the name
/// `OPTIONS {base}/` declares it under `aggregates.decomposable` (§7a.2).
///
/// The order is the declaration order: `COUNT`, `SUM`, `MIN`, `MAX`, `AVG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AggregateFunction {
    /// `COUNT(*)` or `COUNT(path)`, without `DISTINCT`.
    Count,
    /// `SUM(path)`.
    Sum,
    /// `MIN(path)`.
    Min,
    /// `MAX(path)`.
    Max,
    /// `AVG(path)`.
    Avg,
}

impl AggregateFunction {
    /// Every function, in declaration order.
    pub const ALL: [Self; 5] = [Self::Count, Self::Sum, Self::Min, Self::Max, Self::Avg];

    /// The name `OPTIONS {base}/` declares in `aggregates.decomposable`
    /// (§7a.2, §11.6.3): the AQL keyword in upper case.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Count => "COUNT",
            Self::Sum => "SUM",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Avg => "AVG",
        }
    }
}

/// How one façade column is recombined from the node columns of the
/// dispatched query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recombine {
    /// `COUNT`: the sum of the node counts.
    Count {
        /// The node column holding the node's count.
        column: usize,
    },
    /// `SUM`: the sum of the node sums that are not `NULL`, or `NULL` when
    /// every one is.
    Sum {
        /// The node column holding the node's sum.
        column: usize,
    },
    /// `MIN`: the least node minimum that is not `NULL`.
    Min {
        /// The node column holding the node's minimum.
        column: usize,
    },
    /// `MAX`: the greatest node maximum that is not `NULL`.
    Max {
        /// The node column holding the node's maximum.
        column: usize,
    },
    /// `AVG`, asked of every node as the `SUM` and the `COUNT` of the same
    /// path: the sum of the node sums over the sum of the node counts, or
    /// `NULL` when no node counted a value.
    Avg {
        /// The node column holding the node's `SUM`.
        sum: usize,
        /// The node column holding the node's `COUNT`.
        count: usize,
    },
}

impl Recombine {
    /// The aggregate function the façade column applies.
    #[must_use]
    pub fn function(self) -> AggregateFunction {
        match self {
            Self::Count { .. } => AggregateFunction::Count,
            Self::Sum { .. } => AggregateFunction::Sum,
            Self::Min { .. } => AggregateFunction::Min,
            Self::Max { .. } => AggregateFunction::Max,
            Self::Avg { .. } => AggregateFunction::Avg,
        }
    }
}

/// The recombination of an aggregate query across a fan-out: one
/// [`Recombine`] per façade column, in façade order.
///
/// Every node answers the dispatched query with one row; the Tier combines
/// those rows into one row with a cell per façade column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recombination {
    columns: Vec<Recombine>,
}

impl Recombination {
    /// The recombination of the façade columns `columns`, in façade order.
    #[must_use]
    pub fn new(columns: Vec<Recombine>) -> Self {
        Self { columns }
    }

    /// How each façade column is recombined, in façade order.
    #[must_use]
    pub fn columns(&self) -> &[Recombine] {
        &self.columns
    }
}
