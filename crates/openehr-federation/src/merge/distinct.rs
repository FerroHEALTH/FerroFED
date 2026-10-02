// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `SELECT DISTINCT` at the Tier (N13, CP-8).
//!
//! A row is a duplicate when another row has the same value in every column
//! the client selected (AQL 1.1.0 §DISTINCT), the ENDPOINT attributes the
//! gateway adds among them (§9.3), so rows of two endpoints whose attributes
//! differ are two values. Each node removed its own
//! duplicates; the Tier removes the ones across nodes. Two cells hold the same
//! value when the Tier comparator puts neither before the other (no
//! specification governs it: our own design): numbers by value, so `2` and
//! `2.0` are one value; date-times by instant and then as written, so two
//! spellings of one instant stay two values, and a zoned and an unzoned
//! date-time never meet; data values by `openehr-rm`'s `less_than` and then
//! by canonical JSON, members in key order.
//!
//! Of the rows equal under DISTINCT, the one first in the Tier order is kept:
//! the first under the `ORDER BY` keys, then by `endpoint_id`. Its cells and
//! the node it came from are the ones the answer carries.

use std::cmp::Ordering;

use super::cell::Cell;
use super::compare::cmp_cell;

/// The Tier order of two DISTINCT tuples, `Equal` exactly when the rows are
/// duplicates.
fn cmp_tuple(a: &[Cell], b: &[Cell]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| cmp_cell(x, y))
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

/// The positions of `tuples` sorted by value, ties kept in their given order.
fn by_value(tuples: &[&[Cell]]) -> Vec<usize> {
    let mut positions: Vec<usize> = (0..tuples.len()).collect();
    positions.sort_by(|x, y| match (tuples.get(*x), tuples.get(*y)) {
        (Some(a), Some(b)) => cmp_tuple(a, b),
        _ => Ordering::Equal,
    });
    positions
}

/// Whether two of `tuples` are duplicates under the Tier equality.
pub(super) fn has_duplicates(tuples: &[&[Cell]]) -> bool {
    by_value(tuples).windows(2).any(|pair| match pair {
        [x, y] => match (tuples.get(*x), tuples.get(*y)) {
            (Some(a), Some(b)) => cmp_tuple(a, b) == Ordering::Equal,
            _ => false,
        },
        _ => false,
    })
}

/// Keeps the first row of every set of duplicates, `rows` being in the Tier
/// order, and returns the kept rows in that order.
///
/// The result is what DISTINCT and then `ORDER BY` give, because the copies
/// of one value are ordered among themselves by the same Tier order that
/// picks the kept one. Collapsing again changes nothing.
pub(super) fn collapse<T>(rows: Vec<T>, tuple: impl Fn(&T) -> &[Cell]) -> Vec<T> {
    let tuples: Vec<&[Cell]> = rows.iter().map(&tuple).collect();
    let mut keep = vec![false; rows.len()];
    let mut kept: Option<&[Cell]> = None;
    for position in by_value(&tuples) {
        let Some(current) = tuples.get(position).copied() else {
            continue;
        };
        if kept.is_some_and(|first| cmp_tuple(first, current) == Ordering::Equal) {
            continue;
        }
        kept = Some(current);
        if let Some(slot) = keep.get_mut(position) {
            *slot = true;
        }
    }
    rows.into_iter()
        .zip(keep)
        .filter_map(|(row, first)| first.then_some(row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{collapse, has_duplicates};
    use crate::merge::cell::{Cell, decode};
    use serde_json::json;

    #[test]
    fn the_first_copy_in_the_given_order_is_kept() {
        let rows = vec![
            ("b", vec![decode(&json!(2))]),
            ("a", vec![decode(&json!(1))]),
            ("c", vec![decode(&json!(2.0))]),
        ];
        let kept: Vec<&str> = collapse(rows, |(_, tuple)| tuple)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(kept, ["b", "a"], "2 and 2.0 are one value, b is first");
    }

    #[test]
    fn an_empty_tuple_leaves_one_row() {
        let rows: Vec<Vec<Cell>> = vec![Vec::new(), Vec::new()];
        assert_eq!(collapse(rows, Vec::as_slice).len(), 1);
    }

    #[test]
    fn duplicates_are_found_wherever_they_sit() {
        let tuples = [
            vec![decode(&json!(1))],
            vec![decode(&json!(3))],
            vec![decode(&json!(1.0))],
        ];
        let borrowed: Vec<&[Cell]> = tuples.iter().map(Vec::as_slice).collect();
        assert!(has_duplicates(&borrowed));
        assert!(!has_duplicates(borrowed.get(..2).unwrap_or_default()));
    }
}
