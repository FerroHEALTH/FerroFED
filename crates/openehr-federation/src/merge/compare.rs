// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Tier comparator: one total order over decoded cells (no specification
//! governs it: our own design).
//!
//! AQL leaves the order of nulls, the collation of strings and the order of
//! data values undefined (`master03-syntax.adoc` §ORDER BY), so the gateway
//! owns one total order, completing the partial orders of the `openehr-*`
//! crates. For two cells the first rule that applies decides:
//!
//! 1. null is the greatest value, so it sorts last under `ASC` and first under
//!    `DESC` (FerroFED's own);
//! 2. the class rank orders a cross-class pair: boolean, number, temporal,
//!    string, data value, other JSON;
//! 3. within a class: numbers exactly, never an integer through `f64`;
//!    complete date-times by instant through `openehr-base`, zoned before
//!    unzoned; strings by Unicode code point (FerroFED's own); data values by
//!    `openehr-rm`'s `less_than` within a comparability class;
//! 4. otherwise the canonical JSON text by code point.

use std::cmp::Ordering;

use super::cell::{Cell, Data, Num, Temporal};

/// The Tier order of two cells.
pub(super) fn cmp_cell(a: &Cell, b: &Cell) -> Ordering {
    match (a, b) {
        (Cell::Bool(x), Cell::Bool(y)) => x.cmp(y),
        (Cell::Number(x), Cell::Number(y)) => cmp_num(*x, *y),
        (Cell::Temporal(x), Cell::Temporal(y)) => cmp_temporal(x, y),
        (Cell::Text(x), Cell::Text(y)) | (Cell::Other(x), Cell::Other(y)) => x.cmp(y),
        (Cell::Data(x), Cell::Data(y)) => cmp_data(x, y),
        (Cell::Null, Cell::Null) => Ordering::Equal,
        _ => a.rank().cmp(&b.rank()),
    }
}

fn cmp_num(a: Num, b: Num) -> Ordering {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => x.cmp(&y),
        // NOTE: adding 0.0 turns -0.0 into 0.0, so equal numbers compare
        // equal and total_cmp orders every other pair numerically.
        (Num::Float(x), Num::Float(y)) => (x + 0.0).total_cmp(&(y + 0.0)),
        (Num::Int(x), Num::Float(y)) => cmp_int_float(x, y),
        (Num::Float(x), Num::Int(y)) => cmp_int_float(y, x).reverse(),
    }
}

/// An integer against a finite float, exactly: neither passes through the
/// other's representation where it would round.
fn cmp_int_float(int: i128, float: f64) -> Ordering {
    // NOTE: IEEE 754 binary64 bits for 2^127 (biased exponent 1150, zero
    // mantissa); every i128 lies in [-2^127, 2^127).
    const BOUND: f64 = f64::from_bits(0x47E0_0000_0000_0000);
    if float >= BOUND {
        return Ordering::Less;
    }
    if float < -BOUND {
        return Ordering::Greater;
    }
    let whole = float.trunc();
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "an integral f64 inside [-2^127, 2^127) converts to i128 exactly"
    )]
    let integral = whole as i128;
    match int.cmp(&integral) {
        Ordering::Equal if float > whole => Ordering::Less,
        Ordering::Equal if float < whole => Ordering::Greater,
        other => other,
    }
}

fn cmp_temporal(a: &Temporal, b: &Temporal) -> Ordering {
    b.zoned
        .cmp(&a.zoned)
        .then_with(|| a.nanos.cmp(&b.nanos))
        .then_with(|| a.text.cmp(&b.text))
}

fn cmp_data(a: &Data, b: &Data) -> Ordering {
    a.class
        .cmp(&b.class)
        .then_with(|| b.measured.cmp(&a.measured))
        .then_with(|| {
            if !(a.measured && b.measured) {
                return Ordering::Equal;
            }
            if a.value.less_than(&b.value) == Some(true) {
                Ordering::Less
            } else if b.value.less_than(&a.value) == Some(true) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| a.json.cmp(&b.json))
}

/// Ranks the comparability classes of the data values of one answer.
///
/// Two values share a class when `openehr-rm`'s `is_strictly_comparable_to`
/// holds (the same `DV_ORDERED` subtype, with the same units for a quantity
/// and the same kind for a proportion), and within a class `less_than` orders
/// them. Classes are ranked by the smallest canonical JSON among their
/// members, which depends only on the set of values, never on the order the
/// nodes answered in (FerroFED's own).
pub(super) fn rank_classes<'a>(cells: impl IntoIterator<Item = &'a mut Data>) {
    let mut cells: Vec<&mut Data> = cells.into_iter().collect();
    let mut representatives: Vec<usize> = Vec::new();
    let mut least: Vec<String> = Vec::new();
    let mut membership = Vec::with_capacity(cells.len());
    for index in 0..cells.len() {
        let found = representatives.iter().position(|&representative| {
            match (cells.get(representative), cells.get(index)) {
                (Some(known), Some(cell)) => known.value.is_strictly_comparable_to(&cell.value),
                _ => false,
            }
        });
        let class = found.unwrap_or_else(|| {
            representatives.push(index);
            least.push(String::new());
            representatives.len().saturating_sub(1)
        });
        if let (Some(cell), Some(smallest)) = (cells.get(index), least.get_mut(class))
            && (smallest.is_empty() || cell.json < *smallest)
        {
            smallest.clone_from(&cell.json);
        }
        membership.push(class);
    }
    let mut order: Vec<usize> = (0..least.len()).collect();
    order.sort_by(|x, y| least.get(*x).cmp(&least.get(*y)));
    let mut rank = vec![0; least.len()];
    for (position, class) in order.into_iter().enumerate() {
        if let Some(slot) = rank.get_mut(class) {
            *slot = position;
        }
    }
    for (cell, class) in cells.iter_mut().zip(membership) {
        if let Some(position) = rank.get(class) {
            cell.class = *position;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{cmp_cell, rank_classes};
    use crate::merge::cell::{Cell, decode};
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn an_integer_and_a_float_compare_exactly() {
        let big = decode(&json!(9_007_199_254_740_993_u64));
        let float = decode(&json!(9_007_199_254_740_992.0_f64));
        assert_eq!(
            cmp_cell(&big, &float),
            Ordering::Greater,
            "2^53 + 1 is greater than 2^53, which f64 cannot tell apart"
        );
        assert_eq!(
            cmp_cell(&decode(&json!(1)), &decode(&json!(1.5))),
            Ordering::Less
        );
        assert_eq!(
            cmp_cell(&decode(&json!(-2)), &decode(&json!(-1.5))),
            Ordering::Less
        );
    }

    #[test]
    fn null_is_greatest_and_classes_rank_in_order() {
        let ordered = [
            json!(false),
            json!(3),
            json!("2026-01-01T00:00:00Z"),
            json!("abc"),
            json!([1]),
            json!(null),
        ];
        for pair in ordered.windows(2) {
            if let [a, b] = pair {
                assert_eq!(
                    cmp_cell(&decode(a), &decode(b)),
                    Ordering::Less,
                    "{a} before {b}"
                );
            }
        }
    }

    #[test]
    fn strings_compare_by_code_point() {
        assert_eq!(
            cmp_cell(&decode(&json!("B")), &decode(&json!("a"))),
            Ordering::Less,
            "U+0042 before U+0061, whatever a locale would say"
        );
    }

    #[expect(
        clippy::disallowed_types,
        reason = "the test seam: a generated cell is the JSON value a node would send"
    )]
    fn cell_json() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            any::<bool>().prop_map(|flag| json!(flag)),
            (-3_i64..3).prop_map(|integer| json!(integer)),
            (-3.0_f64..3.0).prop_map(|float| json!(float)),
            (0_u8..3, any::<bool>()).prop_map(|(hour, zoned)| {
                json!(format!(
                    "2026-01-01T0{hour}:00:00{}",
                    if zoned { "+01:00" } else { "" }
                ))
            }),
            "[a-c]{0,2}".prop_map(|text| json!(text)),
            (0_u8..3, prop_oneof![Just("kg"), Just("g")]).prop_map(|(magnitude, units)| {
                json!({"_type": "DV_QUANTITY", "magnitude": f64::from(magnitude), "units": units})
            }),
            (0_i64..3).prop_map(|magnitude| json!({"_type": "DV_COUNT", "magnitude": magnitude})),
            Just(json!([1])),
            Just(json!(null)),
        ]
    }

    proptest! {
        #[test]
        fn the_tier_comparator_is_a_total_order(
            values in proptest::collection::vec(cell_json(), 3),
        ) {
            let mut cells: Vec<Cell> = values.iter().map(decode).collect();
            rank_classes(cells.iter_mut().filter_map(|cell| match cell {
                Cell::Data(data) => Some(&mut **data),
                _ => None,
            }));
            let [a, b, c] = [&cells[0], &cells[1], &cells[2]];
            prop_assert_eq!(cmp_cell(a, a), Ordering::Equal, "reflexive");
            prop_assert_eq!(cmp_cell(a, b), cmp_cell(b, a).reverse(), "antisymmetric");
            if cmp_cell(a, b) != Ordering::Greater && cmp_cell(b, c) != Ordering::Greater {
                prop_assert_ne!(cmp_cell(a, c), Ordering::Greater, "transitive");
            }
        }
    }
}
