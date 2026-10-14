// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `LIMIT`, `OFFSET` and `TOP` of a façade query and its ITS-REST paging
//! members, reduced to the `LIMIT` every node is sent and the rows the Tier
//! skips (§11.6.1, §11.6.2, N39).

use openehr_query::ast::{Limit, SelectQuery, TopDirection};

use super::refusal::{OffsetPage, Refusal};
use super::{OffsetStrategy, Paging};

/// Applies the ITS-REST paging members to the query's `LIMIT` and `OFFSET`
/// (ITS-REST Query API `Offset` and `Fetch`, §11.6), and returns the rows the
/// Tier skips.
///
/// The members select a window over the rows the AQL selects
/// ([`window`]), and the window is then paged as an AQL `OFFSET` and `LIMIT`
/// would be. `OFFSET` never reaches a node (§11.6.2). Under
/// [`OffsetStrategy::Bounded`], a page at skip `k` with count `n` is
/// dispatched as `LIMIT k + n`, and the Tier skips `k` rows of the merged
/// order. `by_endpoint` says the client ordered on an ENDPOINT attribute the
/// directive pass took out of `ORDER BY`, so the query is ordered even when
/// no `ORDER BY` term is left (§8.1, §9.3).
pub(super) fn page(
    query: &mut SelectQuery,
    paging: Paging,
    strategy: OffsetStrategy,
    by_endpoint: bool,
) -> Result<u64, Refusal> {
    if paging.offset.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "offset" });
    }
    if paging.fetch.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "fetch" });
    }
    // NOTE: AQL master03-syntax §TOP deprecates `TOP` "in favour of the `LIMIT`
    // clause combined with `ORDER BY`", so `TOP n` is read as `LIMIT n`.
    let top = query.select.top.take();
    if let Some(top) = &top {
        if query.limit.is_some() {
            return Err(Refusal::TopWithLimit);
        }
        if paging.fetch.is_some() {
            return Err(Refusal::TopWithFetch);
        }
        if top.direction == Some(TopDirection::Backward) {
            return Err(Refusal::TopBackward);
        }
    }
    let clause_limit = query
        .limit
        .as_ref()
        .map(|limit| limit.limit)
        .or(top.map(|top| top.count));
    if clause_limit.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "LIMIT" });
    }
    let clause_offset = query.limit.as_ref().and_then(|limit| limit.offset);
    if clause_offset.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "OFFSET" });
    }
    let (skip, count) = window(clause_offset, clause_limit, paging);
    let (dispatched, skip) = match skip {
        Some(0) => (count, 0),
        skip => {
            let OffsetStrategy::Bounded { max_window } = strategy else {
                return Err(Refusal::OffsetUnsupported);
            };
            let Some(count) = count else {
                return Err(Refusal::OffsetPage {
                    reason: OffsetPage::NoLimit,
                });
            };
            // NOTE: §11.6.2 is silent on OFFSET without ORDER BY, so this is our own
            // design: its "merging, ordering and slicing" has no order to slice.
            if query.order_by.is_empty() && !by_endpoint {
                return Err(Refusal::OffsetPage {
                    reason: OffsetPage::NoOrder,
                });
            }
            let past = || Refusal::OffsetPage {
                reason: OffsetPage::PastTheBound {
                    max_window: max_window.get(),
                },
            };
            let skip = skip.ok_or_else(past)?;
            let window = skip
                .checked_add(count)
                .filter(|window| *window <= i64::from(max_window.get()))
                .ok_or_else(past)?;
            let skip = u64::try_from(skip)
                .map_err(|_negative| Refusal::NegativePaging { member: "OFFSET" })?;
            (Some(window), skip)
        }
    };
    query.limit = dispatched.map(|limit| Limit {
        limit,
        offset: None,
    });
    Ok(skip)
}

/// The rows of the AQL result the request selects: the skip from the first
/// row the AQL matches, and the count, `None` for every remaining row.
///
/// ITS-REST defines `offset` as "the row number in result-set to start
/// result-set from" and `fetch` as the "number of rows to fetch" (Query API,
/// `query-codegen.openapi.yaml`), so the members select a window over the
/// rows the AQL `OFFSET` and `LIMIT` select (§11.6): the skip is the AQL
/// `OFFSET` plus `offset`, and the count the smaller of the rows the AQL
/// `LIMIT` leaves after `offset` and `fetch`. Every argument is non-negative.
/// The skip is `None` when it overflows, a page past every bound.
fn window(
    clause_offset: Option<i64>,
    clause_limit: Option<i64>,
    paging: Paging,
) -> (Option<i64>, Option<i64>) {
    let member_offset = paging.offset.unwrap_or(0);
    let skip = clause_offset.unwrap_or(0).checked_add(member_offset);
    let remaining = clause_limit.map(|limit| limit.saturating_sub(member_offset).max(0));
    let count = match (remaining, paging.fetch) {
        (Some(remaining), Some(fetch)) => Some(remaining.min(fetch)),
        (remaining, fetch) => remaining.or(fetch),
    };
    (skip, count)
}
