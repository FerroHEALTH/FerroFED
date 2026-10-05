// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The recombination of aggregate answers across nodes (feature `merge`,
//! §11.6.3, N14, N39): each declared function equals the aggregate over the
//! union of the node rows, exactly and ignoring nulls as AQL does (AQL 1.1.0
//! §Aggregate functions), and a node value that cannot take part in an
//! exactly correct answer refuses its node and leaves no row.
#![cfg(feature = "merge")]

use std::cmp::Ordering;

use openehr_federation::aggregate::{Recombination, Recombine};
use openehr_federation::merge::{Disagreement, Merged, NodeAnswer, combine};
use openehr_federation::order::ResultOrder;
use openehr_its::rest::generated::query::ResultSetRow;
use proptest::collection::vec;
use proptest::prelude::*;
use serde_json::json;

/// One `RESULT_SET` cell as a node writes it.
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: a RESULT_SET cell is a JSON value on the ITS-REST wire"
)]
type Cell = serde_json::Value;

/// The columns every generated node row answers, as the rewrite dispatches
/// `COUNT(*), COUNT(x), SUM(x), MIN(x), MAX(x), AVG(x)`.
fn every_function() -> Recombination {
    Recombination::new(vec![
        Recombine::Count { column: 0 },
        Recombine::Count { column: 1 },
        Recombine::Sum { column: 2 },
        Recombine::Min { column: 3 },
        Recombine::Max { column: 4 },
        Recombine::Avg { sum: 5, count: 6 },
    ])
}

/// The decimal text of `mantissa` scaled down by `scale` digits.
fn fixed(mantissa: i128, scale: u32) -> String {
    if scale == 0 {
        return mantissa.to_string();
    }
    let unit = 10_i128.pow(scale);
    let sign = if mantissa < 0 { "-" } else { "" };
    let magnitude = mantissa.unsigned_abs();
    let unit = unit.unsigned_abs();
    format!(
        "{sign}{}.{:0width$}",
        magnitude.checked_div(unit).unwrap(),
        magnitude.checked_rem(unit).unwrap(),
        width = usize::try_from(scale).unwrap()
    )
}

/// The JSON number written as `text`, as a node writes it.
fn number(text: &str) -> Cell {
    serde_json::from_str(text).unwrap()
}

/// A value of the generated column: an integer, or a real with three
/// decimals.
fn value(mantissa: i128, scale: u32) -> Cell {
    number(&fixed(mantissa, scale))
}

/// The rows, the values counted, and the sum, minimum and maximum mantissa
/// of `values` (nulls among them): what one node computes, or what the
/// federation answers over the union.
type Aggregates = (usize, usize, Option<i128>, Option<i128>, Option<i128>);

fn aggregates(values: &[Option<i128>]) -> Aggregates {
    let present: Vec<i128> = values.iter().flatten().copied().collect();
    let sum = (!present.is_empty()).then(|| present.iter().sum());
    (
        values.len(),
        present.len(),
        sum,
        present.iter().min().copied(),
        present.iter().max().copied(),
    )
}

/// One node's answer row to the dispatched query.
fn node_row(values: &[Option<i128>], scale: u32) -> ResultSetRow {
    let (rows, counted, sum, min, max) = aggregates(values);
    let cell = |found: Option<i128>| found.map_or(json!(null), |m| value(m, scale));
    vec![
        json!(rows),
        json!(counted),
        cell(sum),
        cell(min),
        cell(max),
        cell(sum),
        json!(counted),
    ]
}

/// The mean `sum / (count · 10^scale)` to 40 decimals, read as the nearest
/// JSON number.
fn mean(sum: i128, count: usize, scale: u32) -> Cell {
    let denominator = i128::try_from(count).unwrap() * 10_i128.pow(scale);
    let negative = sum < 0;
    let (mut remainder, denominator) = (sum.unsigned_abs(), denominator.unsigned_abs());
    let whole = remainder.checked_div(denominator).unwrap();
    let mut text = format!("{}{whole}.", if negative { "-" } else { "" });
    remainder = remainder.checked_rem(denominator).unwrap();
    for _ in 0..40 {
        remainder *= 10;
        let digit = u8::try_from(remainder.checked_div(denominator).unwrap()).unwrap();
        text.push(char::from(b'0' + digit));
        remainder = remainder.checked_rem(denominator).unwrap();
    }
    json!(text.parse::<f64>().unwrap())
}

/// The integer nearest `sum / count`, a tie going to the even one: of the two
/// integers around the quotient, the one whose multiple of `count` lies
/// nearer `sum`.
fn nearest_even(sum: i128, count: usize) -> Cell {
    let count = i128::try_from(count).unwrap();
    let below = sum.div_euclid(count);
    let above = below + 1;
    let distance = |candidate: i128| (candidate * count - sum).abs();
    let nearest = match distance(below).cmp(&distance(above)) {
        Ordering::Less => below,
        Ordering::Equal if below.rem_euclid(2) == 0 => below,
        Ordering::Greater | Ordering::Equal => above,
    };
    json!(nearest)
}

/// The federation's row over the union of `values`.
fn expected(values: &[Option<i128>], scale: u32) -> ResultSetRow {
    let (rows, counted, sum, min, max) = aggregates(values);
    let cell = |found: Option<i128>| found.map_or(json!(null), |m| value(m, scale));
    // AQL 1.1.0 §3.9.1.5: the input type determines the return type of AVG.
    let average = sum.map_or(json!(null), |sum| {
        if scale == 0 {
            nearest_even(sum, counted)
        } else {
            mean(sum, counted, scale)
        }
    });
    vec![
        json!(rows),
        json!(counted),
        cell(sum),
        cell(min),
        cell(max),
        average,
    ]
}

/// The values, each with the node it lives at, and the number of nodes.
fn split() -> impl Strategy<Value = (Vec<(Option<i128>, usize)>, usize, u32)> {
    (1_usize..=4, prop::bool::ANY).prop_flat_map(|(nodes, real)| {
        let value = prop::option::weighted(0.8, -1_000_000_i128..1_000_000);
        (
            vec((value, 0..nodes), 0..24),
            Just(nodes),
            Just(if real { 3 } else { 0 }),
        )
    })
}

fn answers(values: &[(Option<i128>, usize)], nodes: usize, scale: u32) -> Vec<NodeAnswer> {
    (0..nodes)
        .map(|node| {
            let held: Vec<Option<i128>> = values
                .iter()
                .filter(|(_, at)| *at == node)
                .map(|(value, _)| *value)
                .collect();
            NodeAnswer::new(format!("node-{node}"), vec![node_row(&held, scale)])
        })
        .collect()
}

fn combined(nodes: Vec<NodeAnswer>, recombination: &Recombination) -> Merged {
    combine(nodes, recombination, &ResultOrder::unordered()).expect("the value is written exactly")
}

proptest! {
    // conformance: CP-10 CP-32
    #[test]
    fn every_recombined_function_is_the_aggregate_over_the_union((values, nodes, scale) in split()) {
        let merged = combined(answers(&values, nodes, scale), &every_function());
        prop_assert!(merged.refused().is_empty(), "{:?}", merged.refused());
        let union: Vec<Option<i128>> = values.iter().map(|(value, _)| *value).collect();
        prop_assert_eq!(merged.rows(), [expected(&union, scale)], "§11.6.3: exactly correct");
    }

    // conformance: CP-32
    #[test]
    fn the_recombination_does_not_depend_on_the_order_the_nodes_answered_in(
        (values, nodes, scale) in split(),
    ) {
        let forward = combined(answers(&values, nodes, scale), &every_function());
        let mut reversed = answers(&values, nodes, scale);
        reversed.reverse();
        prop_assert_eq!(forward, combined(reversed, &every_function()));
    }
}

/// A one-column recombination of `recombine` over nodes answering `cells`.
fn one(recombine: Recombine, cells: &[Cell]) -> Merged {
    let nodes = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| NodeAnswer::new(format!("node-{index}"), vec![vec![cell.clone()]]))
        .collect();
    combined(nodes, &Recombination::new(vec![recombine]))
}

const SUM: Recombine = Recombine::Sum { column: 0 };
const COUNT: Recombine = Recombine::Count { column: 0 };
const MIN: Recombine = Recombine::Min { column: 0 };
const MAX: Recombine = Recombine::Max { column: 0 };

// conformance: CP-32
#[test]
fn reals_add_in_decimal_arithmetic() {
    let merged = one(SUM, &[number("0.1"), number("0.2")]);
    assert_eq!(
        merged.rows(),
        [vec![number("0.3")]],
        "never the binary 0.30000000000000004"
    );
    let mixed = one(SUM, &[json!(1), number("0.5"), json!(null)]);
    assert_eq!(mixed.rows(), [vec![number("1.5")]], "an integer and a real");
}

// conformance: CP-32
#[test]
fn an_aggregate_over_no_value_is_null_and_a_count_is_zero() {
    assert_eq!(
        one(SUM, &[json!(null), json!(null)]).rows(),
        [vec![json!(null)]]
    );
    assert_eq!(one(MIN, &[json!(null)]).rows(), [vec![json!(null)]]);
    assert_eq!(one(COUNT, &[json!(0), json!(0)]).rows(), [vec![json!(0)]]);
    let avg = Recombination::new(vec![Recombine::Avg { sum: 0, count: 1 }]);
    let nodes = vec![
        NodeAnswer::new("node-a", vec![vec![json!(null), json!(0)]]),
        NodeAnswer::new("node-b", vec![vec![json!(null), json!(0)]]),
    ];
    assert_eq!(
        combined(nodes, &avg).rows(),
        [vec![json!(null)]],
        "AQL §AVG"
    );
}

/// The recombined `AVG` over nodes answering the `SUM` and `COUNT` pairs
/// `sums`.
fn average(sums: &[(Cell, i128)]) -> Merged {
    let nodes = sums
        .iter()
        .enumerate()
        .map(|(index, (sum, count))| {
            NodeAnswer::new(
                format!("node-{index}"),
                vec![vec![sum.clone(), json!(count)]],
            )
        })
        .collect();
    combined(
        nodes,
        &Recombination::new(vec![Recombine::Avg { sum: 0, count: 1 }]),
    )
}

// conformance: CP-32
#[test]
fn avg_over_integers_is_the_nearest_integer_a_tie_to_the_even_one() {
    for (sum, count, mean) in [
        (19, 3, 6),
        (20, 3, 7),
        (6, 3, 2),
        (1, 2, 0),
        (5, 2, 2),
        (7, 2, 4),
        (-5, 2, -2),
        (-7, 2, -4),
        (-3, 4, -1),
        (-1, 4, 0),
    ] {
        assert_eq!(
            average(&[(json!(sum), count)]).rows(),
            [vec![json!(mean)]],
            "AQL 1.1.0 §3.9.1.5: {sum} / {count} over Integer input is an Integer"
        );
    }
}

// conformance: CP-32
#[test]
fn avg_over_integers_rounds_once_over_the_federation_sum_and_count() {
    assert_eq!(
        average(&[(json!(1), 2), (json!(2), 2)]).rows(),
        [vec![json!(1)]],
        "3 / 4 is 0.75, whatever each node's own mean would round to"
    );
    assert_eq!(
        average(&[(json!(3), 2), (json!(1), 2)]).rows(),
        [vec![json!(1)]],
        "4 / 4, where the node means 1.5 and 0.5 round to 2 and 0"
    );
    assert_eq!(
        average(&[(json!(1), 2), (json!(1), 2)]).rows(),
        [vec![json!(0)]],
        "2 / 4 is the tie 0.5, which goes to the even 0"
    );
}

// conformance: CP-32
#[test]
fn avg_with_a_real_node_sum_is_the_decimal_mean() {
    assert_eq!(
        average(&[(json!(12), 2), (number("0.5"), 1)]).rows(),
        [vec![number("4.166666666666667")]],
        "AQL 1.1.0 §3.9.1.5: a Real input gives a Real"
    );
    assert_eq!(
        average(&[(number("4.0"), 2)]).rows(),
        [vec![number("2.0")]],
        "a Real mean stays a Real when it is whole"
    );
}

// conformance: CP-32
#[test]
fn min_and_max_over_date_times_compare_instants_and_keep_the_written_value() {
    let later_text = json!("2026-01-01T09:00:00Z");
    let earlier_text = json!("2026-01-01T10:00:00+02:00");
    let cells = [later_text.clone(), earlier_text.clone(), json!(null)];
    assert_eq!(
        one(MIN, &cells).rows(),
        [vec![earlier_text]],
        "10:00+02:00 is 08:00Z, which a string comparison puts last"
    );
    assert_eq!(one(MAX, &cells).rows(), [vec![later_text]]);
    assert_eq!(
        one(MAX, &[json!(3), number("2.5"), json!(-7)]).rows(),
        [vec![json!(3)]],
        "integers and reals compare exactly"
    );
}

/// The refused endpoints and their reasons.
fn refusals(merged: &Merged) -> Vec<(&str, Disagreement)> {
    merged
        .refused()
        .iter()
        .map(|refused| (refused.endpoint(), refused.reason()))
        .collect()
}

// conformance: CP-10 CP-32
#[test]
fn a_value_of_the_wrong_kind_refuses_its_node_and_leaves_no_row() {
    let quantity = json!({"_type": "DV_QUANTITY", "magnitude": 72.0, "units": "kg"});
    let cases = [
        (MIN, json!("a string")),
        (MAX, quantity),
        (MIN, json!(true)),
        (SUM, json!("12")),
        (COUNT, number("3.0")),
        (COUNT, json!(-1)),
        (COUNT, json!(null)),
    ];
    for (recombine, wrong) in cases {
        let merged = one(recombine, &[json!(2), wrong.clone()]);
        assert!(
            merged.rows().is_empty(),
            "{recombine:?} over {wrong}: no guessed value"
        );
        assert_eq!(
            refusals(&merged),
            [("node-1", Disagreement::AggregateValue)],
            "§11.1: {recombine:?} over {wrong} is an answer the gateway could not use"
        );
    }
}

// conformance: CP-10 CP-32
#[test]
fn an_avg_whose_sum_and_count_disagree_refuses_its_node() {
    let avg = Recombination::new(vec![Recombine::Avg { sum: 0, count: 1 }]);
    for row in [vec![json!(null), json!(2)], vec![json!(4), json!(0)]] {
        let nodes = vec![
            NodeAnswer::new("node-a", vec![vec![json!(4), json!(2)]]),
            NodeAnswer::new("node-b", vec![row.clone()]),
        ];
        let merged = combined(nodes, &avg);
        assert!(merged.rows().is_empty());
        assert_eq!(
            refusals(&merged),
            [("node-b", Disagreement::AggregateValue)],
            "{row:?}"
        );
    }
}

// conformance: CP-10 CP-32
#[test]
fn values_no_one_order_compares_refuse_every_node_that_holds_one() {
    for (a, b) in [
        (json!(5), json!("2026-01-01T10:00:00Z")),
        (json!("2026-01-01T10:00:00Z"), json!("2026-01-01T10:00:00")),
    ] {
        let merged = one(MIN, &[a.clone(), b.clone(), json!(null)]);
        assert!(merged.rows().is_empty(), "{a} and {b}");
        assert_eq!(
            refusals(&merged),
            [
                ("node-0", Disagreement::AggregateKinds),
                ("node-1", Disagreement::AggregateKinds)
            ],
            "{a} and {b}: no exact MIN exists"
        );
    }
}

// conformance: CP-32
#[test]
fn a_node_that_does_not_answer_one_row_is_refused() {
    let recombination = Recombination::new(vec![COUNT]);
    for rows in [Vec::new(), vec![vec![json!(1)], vec![json!(2)]]] {
        let nodes = vec![
            NodeAnswer::new("node-a", vec![vec![json!(1)]]),
            NodeAnswer::new("node-b", rows.clone()),
        ];
        let merged = combined(nodes, &recombination);
        assert!(merged.rows().is_empty());
        assert_eq!(
            refusals(&merged),
            [("node-b", Disagreement::AggregateRows)],
            "{rows:?}"
        );
    }
    let short = vec![NodeAnswer::new("node-a", vec![Vec::new()])];
    assert_eq!(
        refusals(&combined(short, &recombination)),
        [("node-a", Disagreement::ShortRow)]
    );
}

// conformance: CP-32
#[test]
fn with_no_answer_there_is_no_row() {
    // §11.3: a query in which no node answered returns empty rows.
    assert_eq!(combined(Vec::new(), &every_function()), Merged::default());
}

// conformance: CP-32
#[test]
fn the_recombined_row_is_cut_at_the_limit_and_offset() {
    let recombination = Recombination::new(vec![COUNT]);
    let nodes = || vec![NodeAnswer::new("node-a", vec![vec![json!(1)]])];
    let paged = ResultOrder::new(Vec::new(), Vec::new(), Some(2)).with_offset(1);
    let merged = combine(nodes(), &recombination, &paged).unwrap();
    assert!(
        merged.rows().is_empty() && merged.refused().is_empty(),
        "§11.6.2: row 1 of one"
    );
    let none = ResultOrder::new(Vec::new(), Vec::new(), Some(0));
    let empty = vec![NodeAnswer::new("node-a", Vec::new())];
    let merged = combine(empty, &recombination, &none).unwrap();
    assert!(
        merged.rows().is_empty() && merged.refused().is_empty(),
        "LIMIT 0 asks for none"
    );
}

// conformance: CP-32
#[test]
fn a_value_the_gateway_cannot_write_exactly_is_an_error_never_a_rounding() {
    let recombination = Recombination::new(vec![COUNT]);
    let past_u64 = vec![
        NodeAnswer::new("node-a", vec![vec![json!(u64::MAX)]]),
        NodeAnswer::new("node-b", vec![vec![json!(1)]]),
    ];
    let error = combine(past_u64, &recombination, &ResultOrder::unordered()).unwrap_err();
    assert_eq!(error.column(), 0);
    for cells in [
        vec![number("1e30"), json!(1)],
        vec![number("9e27"), number("0.1")],
    ] {
        let nodes = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| NodeAnswer::new(format!("node-{index}"), vec![vec![cell.clone()]]))
            .collect();
        assert!(
            combine(
                nodes,
                &Recombination::new(vec![SUM]),
                &ResultOrder::unordered()
            )
            .is_err(),
            "{cells:?}: the decimal would round the sum"
        );
    }
}
