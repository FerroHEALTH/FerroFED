// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The result-cell seam: each node row read against the façade's own columns,
//! with the subject columns re-injected (`.claude/rules/rust-style.md`, seam
//! 2; N5, §7.1).
//!
//! A node answers the rewritten query, whose `SELECT` lacks the subject
//! columns, so a façade row is built column by column from where each one
//! comes from: a node cell by position, or the resolution input as a constant
//! `STRING` (N5). A cell travels as the node sent it.
#![expect(
    clippy::disallowed_types,
    reason = "the result-cell seam: ITS-REST types a RESULT_SET cell as a JSON value"
)]

use openehr_federation::aql::ColumnSource;
use openehr_federation::aql::subject::Subject;
use openehr_its::rest::generated::query::ResultSetRow;
use serde_json::Value;

// TODO(#52): decode cells into typed values for the cross-node ORDER BY.

/// A node row that cannot be read against the façade's columns.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CellError {
    /// A row has fewer cells than the node query selected.
    #[error("a node answered a row with {found} cells where the query selects at least {needed}")]
    ShortRow {
        /// The cells the row has.
        found: usize,
        /// The cells the façade columns read.
        needed: usize,
    },
    /// A column re-injects the subject, and the query names none.
    #[error("a column re-injects the patient, and the query names none")]
    NoSubject,
}

/// The façade rows of `rows`, each built from `sources`.
///
/// # Errors
/// Returns [`CellError::ShortRow`] for a node row too short for the node
/// columns `sources` reads, and [`CellError::NoSubject`] when a source
/// re-injects a subject the query does not name.
pub fn reinject(
    rows: Vec<ResultSetRow>,
    sources: &[ColumnSource],
    subject: Option<&Subject>,
) -> Result<Vec<ResultSetRow>, CellError> {
    let needed = sources
        .iter()
        .filter_map(|source| match source {
            ColumnSource::Node(index) => index.checked_add(1),
            ColumnSource::Subject | ColumnSource::Namespace => None,
        })
        .max()
        .unwrap_or(0);
    rows.into_iter()
        .map(|row| {
            if row.len() < needed {
                return Err(CellError::ShortRow {
                    found: row.len(),
                    needed,
                });
            }
            sources
                .iter()
                .map(|source| cell(&row, *source, subject))
                .collect()
        })
        .collect()
}

/// The cell `source` names in `row`.
fn cell(
    row: &[Value],
    source: ColumnSource,
    subject: Option<&Subject>,
) -> Result<Value, CellError> {
    match source {
        ColumnSource::Node(index) => row.get(index).cloned().ok_or(CellError::ShortRow {
            found: row.len(),
            needed: index.saturating_add(1),
        }),
        ColumnSource::Subject => subject
            .map(|subject| Value::String(subject.value().to_owned()))
            .ok_or(CellError::NoSubject),
        ColumnSource::Namespace => subject
            .map(|subject| Value::String(subject.namespace().to_owned()))
            .ok_or(CellError::NoSubject),
    }
}

#[cfg(test)]
mod tests {
    use super::{CellError, reinject};
    use openehr_federation::aql::ColumnSource;
    use serde_json::json;

    #[test]
    fn node_cells_keep_their_position_without_a_subject() {
        let rows = vec![vec![json!("a"), json!(1)], vec![json!("b"), json!(2)]];
        let sources = [ColumnSource::Node(1), ColumnSource::Node(0)];
        assert_eq!(
            reinject(rows, &sources, None).unwrap(),
            vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]],
            "each façade column reads its node column"
        );
    }

    #[test]
    fn a_row_too_short_for_the_query_is_refused() {
        let rows = vec![vec![json!("a")]];
        let sources = [ColumnSource::Node(0), ColumnSource::Node(1)];
        assert_eq!(
            reinject(rows, &sources, None),
            Err(CellError::ShortRow {
                found: 1,
                needed: 2
            }),
            "a missing cell is never filled in"
        );
    }

    #[test]
    fn a_subject_column_without_a_subject_is_refused() {
        let rows = vec![vec![json!("a")]];
        assert_eq!(
            reinject(rows, &[ColumnSource::Subject], None),
            Err(CellError::NoSubject),
            "nothing to re-inject"
        );
    }
}
