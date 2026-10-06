// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The result-cell seam: each node row read against the façade's own columns,
//! with the subject columns re-injected (N5, §7.1) and the ENDPOINT attributes
//! added (§9.3, N12).
//!
//! A node answers the rewritten query, whose `SELECT` lacks the subject
//! columns and the ENDPOINT attributes, so a façade row is built column by
//! column from where each one comes from: a node cell by position, the
//! resolution input as a constant `STRING` (N5), or the value the registry
//! holds for the endpoint the row came from, as a `STRING` (§9.3). A cell
//! travels as the node sent it. The access log reads the template and
//! archetype ids of the archetype roots a delivered row holds, and no other
//! member of a cell ([`root_objects`]).
#![expect(
    clippy::disallowed_types,
    reason = "the result-cell seam: ITS-REST types a RESULT_SET cell as a JSON value"
)]

use ehds_logging::classify::RootObject;
use openehr_federation::aql::ColumnSource;
use openehr_federation::aql::subject::Subject;
use openehr_federation::attribute::EndpointAttribute;
use openehr_its::json::from_canonical_value;
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_rm::v1_2::common::archetyped::archetyped::Archetyped;
use serde_json::Value;

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
    /// A column is an ENDPOINT attribute, and the fan-out answered no value
    /// of it beside the row (§9.3).
    #[error("a column is an ENDPOINT attribute, and no value of it came with the row")]
    EndpointAttribute,
}

/// What the gateway adds to the node rows: the patient the query names, and
/// the ENDPOINT attributes the fan-out answered beside each row.
#[derive(Debug, Clone, Copy)]
pub struct Added<'a> {
    /// The resolution input the subject columns re-inject (N5).
    pub subject: Option<&'a Subject>,
    /// The ENDPOINT attributes the plan added, in the order of each row's
    /// values ([`openehr_federation::aql::Analysis::attributes`]).
    pub attributes: &'a [EndpointAttribute],
    /// The values of `attributes` beside each row, one entry per row
    /// ([`ferrofed_engine::fanout::FederatedAnswer::attributes`]).
    pub values: &'a [Vec<String>],
}

/// The cells every node row must carry for `sources` to read it: one past the
/// highest node column, or none when every column is added by the gateway.
#[must_use]
pub fn width(sources: &[ColumnSource]) -> usize {
    sources
        .iter()
        .filter_map(|source| match source {
            ColumnSource::Node(index) => index.checked_add(1),
            ColumnSource::Subject | ColumnSource::Namespace | ColumnSource::Endpoint(_) => None,
        })
        .max()
        .unwrap_or(0)
}

/// The façade rows of `rows`, each built from `sources` and what `added`
/// holds.
///
/// # Errors
/// Returns [`CellError::ShortRow`] for a node row too short for the node
/// columns `sources` reads, [`CellError::NoSubject`] when a source
/// re-injects a subject the query does not name, and
/// [`CellError::EndpointAttribute`] when a source is an ENDPOINT attribute no
/// value of which came with the row.
pub fn reinject(
    rows: Vec<ResultSetRow>,
    sources: &[ColumnSource],
    added: &Added<'_>,
) -> Result<Vec<ResultSetRow>, CellError> {
    let needed = width(sources);
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            if row.len() < needed {
                return Err(CellError::ShortRow {
                    found: row.len(),
                    needed,
                });
            }
            let values = added.values.get(index).map(Vec::as_slice);
            sources
                .iter()
                .map(|source| cell(&row, *source, added, values))
                .collect()
        })
        .collect()
}

/// Returns cell `index` of `row` when the node sent it as a JSON string, the
/// form a `String` primitive takes in a `RESULT_SET` row, or `None` for a
/// missing cell or any other value.
///
/// # Examples
///
/// ```
/// use ferrofed_server::facade::cells::text;
///
/// let row = vec!["9e4e5f2a-5b8c-4d3e-8f1a-2b3c4d5e6f70".into(), 7.into()];
/// assert_eq!(Some("9e4e5f2a-5b8c-4d3e-8f1a-2b3c4d5e6f70"), text(&row, 0));
/// assert_eq!(None, text(&row, 1));
/// assert_eq!(None, text(&row, 2));
/// ```
#[must_use]
pub fn text(row: &ResultSetRow, index: usize) -> Option<&str> {
    row.get(index).and_then(Value::as_str)
}

/// The cell of `row` at `index` written as JSON text, an RM object as the
/// canonical JSON the node sent, or `None` when the row is shorter or the
/// cell is `null`.
#[must_use]
pub fn json(row: &ResultSetRow, index: usize) -> Option<String> {
    row.get(index)
        .filter(|cell| !cell.is_null())
        .map(Value::to_string)
}

/// The cell `source` names in `row`, whose ENDPOINT attribute values are
/// `values`.
fn cell(
    row: &[Value],
    source: ColumnSource,
    added: &Added<'_>,
    values: Option<&[String]>,
) -> Result<Value, CellError> {
    match source {
        ColumnSource::Node(index) => row.get(index).cloned().ok_or(CellError::ShortRow {
            found: row.len(),
            needed: index.saturating_add(1),
        }),
        ColumnSource::Subject => added
            .subject
            .map(|subject| Value::String(subject.value().to_owned()))
            .ok_or(CellError::NoSubject),
        ColumnSource::Namespace => added
            .subject
            .map(|subject| Value::String(subject.namespace().to_owned()))
            .ok_or(CellError::NoSubject),
        ColumnSource::Endpoint(attribute) => added
            .attributes
            .iter()
            .position(|added| *added == attribute)
            .and_then(|position| values?.get(position))
            .map(|value| Value::String(value.clone()))
            .ok_or(CellError::EndpointAttribute),
    }
}

/// Returns what the delivered façade `rows` show of their data, for the
/// access log.
///
/// That is the model ids of every cell in a node column of `sources` that is
/// an archetype root, and whether any such cell is not one. A root is an RM object with `archetype_details` (ITS-REST `Locatable`,
/// `Archetyped`), or an `ORIGINAL_VERSION` whose `data` is one. A cell of
/// any other kind, a leaf value or an aggregate, and a root whose ids do not
/// read, is unrooted, so the query's own constraints classify it; `null`
/// holds no data. No specification governs which cells are read: our own
/// design.
#[must_use]
pub fn root_objects(rows: &[ResultSetRow], sources: &[ColumnSource]) -> (Vec<RootObject>, bool) {
    roots(rows.iter().flat_map(|row| {
        row.iter()
            .zip(sources)
            .filter(|(_, source)| matches!(source, ColumnSource::Node(_)))
            .map(|(cell, _)| Some(cell))
    }))
}

/// Returns what the `rows` one node answered show of their data, read as
/// [`root_objects`] reads the delivered rows, before the merge.
///
/// A node row holds the node columns alone, each at the position its
/// [`ColumnSource::Node`] names. A cell the row is too short to hold counts
/// as unrooted, so the query's own constraints classify it.
#[must_use]
pub fn node_root_objects(
    rows: &[ResultSetRow],
    sources: &[ColumnSource],
) -> (Vec<RootObject>, bool) {
    roots(rows.iter().flat_map(|row| {
        sources.iter().filter_map(move |source| match source {
            ColumnSource::Node(index) => Some(row.get(*index)),
            ColumnSource::Subject | ColumnSource::Namespace | ColumnSource::Endpoint(_) => None,
        })
    }))
}

/// The root objects among `cells`, and whether any cell is no root: a
/// missing cell is unrooted, and `null` holds no data.
fn roots<'a>(cells: impl Iterator<Item = Option<&'a Value>>) -> (Vec<RootObject>, bool) {
    let mut objects = Vec::new();
    let mut unrooted = false;
    for cell in cells {
        match cell {
            Some(cell) if cell.is_null() => {}
            Some(cell) => match root(cell) {
                Some(object) => objects.push(object),
                None => unrooted = true,
            },
            None => unrooted = true,
        }
    }
    (objects, unrooted)
}

/// The root object `cell` is, or `None` when it is no archetype root whose
/// ids read.
fn root(cell: &Value) -> Option<RootObject> {
    let object = cell.as_object()?;
    if let Some(data) = object.get("data")
        && object.get("_type").and_then(Value::as_str) == Some("ORIGINAL_VERSION")
    {
        let mut root = root(data)?;
        root.version_uid = object
            .get("uid")
            .and_then(|uid| uid.get("value"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        return Some(root);
    }
    // NOTE: ITS-REST `Archetyped`: a details object that is not one names no id, and the
    // cell counts as unrooted, which classifies it by the query and never as of no category.
    let details: Archetyped = from_canonical_value(object.get("archetype_details")?).ok()?;
    Some(RootObject {
        template_id: details.template_id.map(|template| template.value),
        archetype_id: Some(details.archetype_id.value),
        version_uid: object
            .get("uid")
            .and_then(|uid| uid.get("value"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::{Added, CellError, reinject};
    use openehr_federation::aql::ColumnSource;
    use openehr_federation::attribute::EndpointAttribute;
    use serde_json::json;

    const NOTHING: Added<'static> = Added {
        subject: None,
        attributes: &[],
        values: &[],
    };

    #[test]
    fn node_cells_keep_their_position_without_a_subject() {
        let rows = vec![vec![json!("a"), json!(1)], vec![json!("b"), json!(2)]];
        let sources = [ColumnSource::Node(1), ColumnSource::Node(0)];
        assert_eq!(
            reinject(rows, &sources, &NOTHING).unwrap(),
            vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]],
            "each façade column reads its node column"
        );
    }

    #[test]
    fn a_row_too_short_for_the_query_is_refused() {
        let rows = vec![vec![json!("a")]];
        let sources = [ColumnSource::Node(0), ColumnSource::Node(1)];
        assert_eq!(
            reinject(rows, &sources, &NOTHING),
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
            reinject(rows, &[ColumnSource::Subject], &NOTHING),
            Err(CellError::NoSubject),
            "nothing to re-inject"
        );
    }

    #[test]
    fn each_row_carries_the_attributes_of_its_own_endpoint() {
        let rows = vec![vec![json!("a1")], vec![json!("b1")]];
        let values = [
            vec!["node-a".to_owned(), "cdr-a".to_owned()],
            vec!["node-b".to_owned(), "cdr-b".to_owned()],
        ];
        let added = Added {
            subject: None,
            attributes: &[EndpointAttribute::EndpointId, EndpointAttribute::SystemId],
            values: &values,
        };
        let sources = [
            ColumnSource::Endpoint(EndpointAttribute::SystemId),
            ColumnSource::Node(0),
            ColumnSource::Endpoint(EndpointAttribute::EndpointId),
        ];
        assert_eq!(
            reinject(rows, &sources, &added).unwrap(),
            vec![
                vec![json!("cdr-a"), json!("a1"), json!("node-a")],
                vec![json!("cdr-b"), json!("b1"), json!("node-b")],
            ],
            "§9.3, N12: a row's attributes are its endpoint's"
        );
    }

    #[test]
    fn an_attribute_with_no_value_beside_the_row_is_refused() {
        let rows = vec![vec![json!("a1")]];
        let values = [Vec::new()];
        let added = Added {
            subject: None,
            attributes: &[EndpointAttribute::Url],
            values: &values,
        };
        let sources = [ColumnSource::Endpoint(EndpointAttribute::Url)];
        assert_eq!(
            reinject(rows.clone(), &sources, &added),
            Err(CellError::EndpointAttribute),
            "a recombined row comes from no endpoint, so it has no attribute to add"
        );
        assert_eq!(
            reinject(rows, &sources, &NOTHING),
            Err(CellError::EndpointAttribute),
            "an attribute the plan did not add is never filled in"
        );
    }
}
