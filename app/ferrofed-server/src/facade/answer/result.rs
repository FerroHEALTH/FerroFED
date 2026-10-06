// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows of a federated query's `RESULT_SET`: the merged node rows with
//! what the gateway adds re-injected (§9, §10).

use http::StatusCode;
use openehr_federation::aql::ColumnSource;
use openehr_its::rest::generated::query::ResultSet;

use super::Failure;
use crate::facade::cells;

/// Re-injects into the rows of `result_set` the subject and the ENDPOINT
/// attributes `added` names, at the columns `sources` place them, when the
/// answer settled on `200`; under any other status it answers no rows.
///
/// # Errors
///
/// Returns [`Failure::Cells`] when a node's rows do not match the query it
/// was sent.
pub(super) fn reinjected(
    result_set: &mut ResultSet,
    status: StatusCode,
    sources: &[ColumnSource],
    added: &cells::Added<'_>,
) -> Result<(), Failure> {
    let rows = std::mem::take(&mut result_set.rows);
    result_set.rows = if status == StatusCode::OK {
        cells::reinject(rows, sources, added).map_err(Failure::Cells)?
    } else {
        Vec::new()
    };
    Ok(())
}
