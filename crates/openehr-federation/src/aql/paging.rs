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
/// `OFFSET` never reaches a node (§11.6.2). Under
/// [`OffsetStrategy::Bounded`], a page at `OFFSET k` is dispatched as
/// `LIMIT k + n`, and the Tier skips `k` rows of the merged order.
pub(super) fn page(
    query: &mut SelectQuery,
    paging: Paging,
    strategy: OffsetStrategy,
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
    if clause_offset.is_some_and(i64::is_negative) {
        return Err(Refusal::NegativePaging { member: "OFFSET" });
    }
    let limit = clause_limit.or(paging.fetch);
    let offset = clause_offset.or(paging.offset).unwrap_or(0);
    let (dispatched, skip) = if offset == 0 {
        (limit, 0)
    } else {
        let OffsetStrategy::Bounded { max_window } = strategy else {
            return Err(Refusal::OffsetUnsupported);
        };
        let Some(limit) = limit else {
            return Err(Refusal::OffsetPage {
                reason: OffsetPage::NoLimit,
            });
        };
        // NOTE: §11.6.2 is silent on OFFSET without ORDER BY, so this is our own
        // design: its "merging, ordering and slicing" has no order to slice.
        if query.order_by.is_empty() {
            return Err(Refusal::OffsetPage {
                reason: OffsetPage::NoOrder,
            });
        }
        let past = Refusal::OffsetPage {
            reason: OffsetPage::PastTheBound {
                max_window: max_window.get(),
            },
        };
        let window = offset
            .checked_add(limit)
            .filter(|window| *window <= i64::from(max_window.get()))
            .ok_or(past)?;
        let skip = u64::try_from(offset)
            .map_err(|_negative| Refusal::NegativePaging { member: "OFFSET" })?;
        (Some(window), skip)
    };
    query.limit = dispatched.map(|limit| Limit {
        limit,
        offset: None,
    });
    Ok(skip)
}
