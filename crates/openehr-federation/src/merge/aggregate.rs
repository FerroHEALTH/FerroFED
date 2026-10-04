// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The recombination of the one-row node answers of an aggregate query into
//! the federation's row (§11.6.3, N14, N39).
//!
//! "The result must be exactly correct" (§11.6.3), so every node value is
//! checked before anything is combined, and a value that cannot take part in
//! an exact answer refuses its node instead of being guessed around: the
//! gateway reports it `node-error`, "a response the gateway could not use"
//! (§11.1). Integers add in `i128` with checked arithmetic; a real adds in
//! decimal arithmetic, never in binary floating point. An `AVG` over integers
//! divides in `i128` too and rounds once, after the division.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use openehr_its::rest::generated::query::ResultSetRow;
use rust_decimal::Decimal;

use super::cell::{self, Cell, Num, decode};
use super::compare::cmp_cell;
use super::{Disagreement, Refused, Unrepresentable};
use crate::aggregate::{Recombination, Recombine};

/// One node's validated part of one façade column.
#[derive(Debug)]
enum Part {
    /// A count.
    Count(i128),
    /// A sum, `None` for the `NULL` of a node with no value.
    Sum(Option<Num>),
    /// A minimum or maximum, `None` for `NULL`.
    Extreme(Option<Cell>),
    /// The sum and the count of an `AVG`.
    Mean(Option<Num>, i128),
}

/// One node's answer, validated.
#[derive(Debug)]
pub(super) struct Answer {
    endpoint: String,
    row: ResultSetRow,
    parts: Vec<Part>,
}

/// Validates one node's answer against the recombination: `expected` rows
/// (one, or none under `LIMIT 0`), each value of the kind its function takes.
pub(super) fn validate(
    endpoint: String,
    mut rows: Vec<ResultSetRow>,
    recombination: &Recombination,
    expected: usize,
) -> Result<Option<Answer>, Refused> {
    let refuse = |endpoint: String, reason| Err(Refused { endpoint, reason });
    if rows.len() != expected {
        return refuse(endpoint, Disagreement::AggregateRows);
    }
    let Some(row) = rows.pop() else {
        return Ok(None);
    };
    let mut parts = Vec::with_capacity(recombination.columns().len());
    for recombine in recombination.columns() {
        let part = match *recombine {
            Recombine::Count { column } => count(&row, column).map(Part::Count),
            Recombine::Sum { column } => sum(&row, column).map(Part::Sum),
            Recombine::Min { column } | Recombine::Max { column } => {
                extreme(&row, column).map(Part::Extreme)
            }
            Recombine::Avg {
                sum: total,
                count: counted,
            } => match (sum(&row, total), count(&row, counted)) {
                (Ok(total), Ok(counted)) => mean(total, counted),
                (Err(reason), _) | (_, Err(reason)) => Err(reason),
            },
        };
        match part {
            Ok(part) => parts.push(part),
            Err(reason) => return refuse(endpoint, reason),
        }
    }
    Ok(Some(Answer {
        endpoint,
        row,
        parts,
    }))
}

/// The cell at `column`, or [`Disagreement::ShortRow`].
fn at(row: &ResultSetRow, column: usize) -> Result<Cell, Disagreement> {
    row.get(column).map(decode).ok_or(Disagreement::ShortRow)
}

/// A count: an integer that is not negative, since "the return type is always
/// an Integer" and a count is never `NULL` (AQL 1.1.0 §COUNT).
fn count(row: &ResultSetRow, column: usize) -> Result<i128, Disagreement> {
    match at(row, column)? {
        Cell::Number(Num::Int(count)) if count >= 0 => Ok(count),
        _ => Err(Disagreement::AggregateValue),
    }
}

/// A sum: a number, or `NULL`, since `SUM` takes "either Integer or Real"
/// (AQL 1.1.0 §SUM).
fn sum(row: &ResultSetRow, column: usize) -> Result<Option<Num>, Disagreement> {
    match at(row, column)? {
        Cell::Number(number) => Ok(Some(number)),
        Cell::Null => Ok(None),
        _ => Err(Disagreement::AggregateValue),
    }
}

/// A minimum or maximum: a number, a complete date-time, or `NULL`.
// NOTE: AQL 1.1.0 §MIN admits strings, whose order the Tier and a node need not
// share (§11.6.1); no specification governs the rest: our own design.
fn extreme(row: &ResultSetRow, column: usize) -> Result<Option<Cell>, Disagreement> {
    match at(row, column)? {
        cell @ (Cell::Number(_) | Cell::Temporal(_)) => Ok(Some(cell)),
        Cell::Null => Ok(None),
        _ => Err(Disagreement::AggregateValue),
    }
}

/// The `SUM` and `COUNT` of an `AVG`, which agree: `SUM` is `NULL` exactly when
/// `COUNT` counted no value, since both ignore `NULL` (AQL 1.1.0 §Aggregate
/// functions).
fn mean(total: Option<Num>, counted: i128) -> Result<Part, Disagreement> {
    if total.is_none() == (counted == 0) {
        Ok(Part::Mean(total, counted))
    } else {
        Err(Disagreement::AggregateValue)
    }
}

/// The endpoints whose `MIN` or `MAX` values are of kinds no one order
/// compares: a number, a zoned date-time, an unzoned one (`openehr-base`
/// holds the last two incomparable).
pub(super) fn incomparable(answers: &[Answer], recombination: &Recombination) -> Vec<Refused> {
    let mut refused: BTreeSet<String> = BTreeSet::new();
    for (index, recombine) in recombination.columns().iter().enumerate() {
        if !matches!(recombine, Recombine::Min { .. } | Recombine::Max { .. }) {
            continue;
        }
        let kinds: Vec<(&str, u8)> = answers
            .iter()
            .filter_map(|answer| match answer.parts.get(index) {
                Some(Part::Extreme(Some(cell))) => Some((answer.endpoint.as_str(), kind(cell))),
                _ => None,
            })
            .collect();
        let first = kinds.first().map(|(_, kind)| *kind);
        if kinds.iter().any(|(_, kind)| Some(*kind) != first) {
            refused.extend(kinds.iter().map(|(endpoint, _)| (*endpoint).to_owned()));
        }
    }
    refused
        .into_iter()
        .map(|endpoint| Refused {
            endpoint,
            reason: Disagreement::AggregateKinds,
        })
        .collect()
}

fn kind(cell: &Cell) -> u8 {
    match cell {
        Cell::Temporal(temporal) if temporal.zoned => 1,
        Cell::Temporal(_) => 2,
        _ => 0,
    }
}

/// The federation's row, recombined from every node's validated answer.
///
/// # Errors
/// [`Unrepresentable`] when a value cannot be written exactly: a count or an
/// integer sum past `u64`, or a decimal sum past what the decimal holds or
/// what a JSON number reads back as.
pub(super) fn recombine(
    answers: &[Answer],
    recombination: &Recombination,
) -> Result<ResultSetRow, Unrepresentable> {
    let mut row = Vec::with_capacity(recombination.columns().len());
    for (index, recombine) in recombination.columns().iter().enumerate() {
        let overflow = Unrepresentable { column: index };
        let parts = answers.iter().filter_map(|answer| answer.parts.get(index));
        let cell = match *recombine {
            Recombine::Count { .. } => parts
                .filter_map(|part| match part {
                    Part::Count(count) => Some(*count),
                    _ => None,
                })
                .try_fold(0_i128, i128::checked_add)
                .and_then(cell::integer)
                .ok_or(overflow)?,
            Recombine::Sum { .. } => match summed(parts).ok_or(overflow)? {
                Total::Null => cell::null(),
                Total::Integer(sum) => cell::integer(sum).ok_or(overflow)?,
                Total::Real(sum) => cell::exact_real(sum).ok_or(overflow)?,
            },
            Recombine::Min { column } | Recombine::Max { column } => {
                let wanted = if matches!(recombine, Recombine::Max { .. }) {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
                chosen(answers, index, wanted)
                    .and_then(|answer| answer.row.get(column).cloned())
                    .unwrap_or_else(cell::null)
            }
            Recombine::Avg { .. } => match averaged(parts).ok_or(overflow)? {
                Total::Null => cell::null(),
                Total::Integer(mean) => cell::integer(mean).ok_or(overflow)?,
                Total::Real(mean) => cell::nearest_real(mean).ok_or(overflow)?,
            },
        };
        row.push(cell);
    }
    Ok(row)
}

/// The `SUM` of the node sums in `parts`, or `None` when it cannot be held
/// exactly.
fn summed<'a>(parts: impl Iterator<Item = &'a Part>) -> Option<Total> {
    let sums: Vec<Num> = parts
        .filter_map(|part| match part {
            Part::Sum(sum) => *sum,
            _ => None,
        })
        .collect();
    total(&sums)
}

/// The `AVG` over the node sums and counts in `parts`, [`Total::Null`] when no
/// node counted a value, or `None` when it cannot be held exactly.
fn averaged<'a>(parts: impl Iterator<Item = &'a Part>) -> Option<Total> {
    let mut sums = Vec::new();
    let mut counted = 0_i128;
    for part in parts {
        if let Part::Mean(sum, count) = part {
            sums.extend(*sum);
            counted = counted.checked_add(*count)?;
        }
    }
    if counted == 0 {
        return Some(Total::Null);
    }
    match total(&sums)? {
        Total::Null => integer_mean(0, counted).map(Total::Integer),
        Total::Integer(sum) => integer_mean(sum, counted).map(Total::Integer),
        Total::Real(sum) => decimal(Num::Int(counted))
            .and_then(|counted| sum.checked_div(counted))
            .map(Total::Real),
    }
}

/// The mean `sum / counted`, rounded once to the nearest integer with a tie
/// to the even one; `None` for a `counted` of zero, since a count is never
/// negative.
///
/// The node sums carry the input type, since the input determines the return
/// type of `SUM` (AQL 1.1.0 §3.9.1.4), and an Integer input gives an Integer
/// `AVG` (AQL 1.1.0 §3.9.1.5).
// NOTE: AQL 1.1.0 §3.9.1.5 states no rounding, so no specification governs this: our own
// design; the nearest integer, ties to even as IEEE 754 roundTiesToEven, with no bias.
fn integer_mean(sum: i128, counted: i128) -> Option<i128> {
    let quotient = sum.checked_div(counted)?;
    let remainder = sum.checked_rem(counted)?;
    let twice = remainder.unsigned_abs().checked_mul(2)?;
    let away = match twice.cmp(&counted.unsigned_abs()) {
        Ordering::Greater => true,
        Ordering::Equal => quotient & 1 == 1,
        Ordering::Less => false,
    };
    match (away, sum < 0) {
        (false, _) => Some(quotient),
        (true, false) => quotient.checked_add(1),
        (true, true) => quotient.checked_sub(1),
    }
}

/// A recombined sum or mean, before it is written as a cell.
#[derive(Debug)]
enum Total {
    /// No node holds a value.
    Null,
    /// Every node sum is an integer, so the value is one.
    Integer(i128),
    /// A node sum is a real, so the value is one.
    Real(Decimal),
}

/// The exact sum of `sums`, or `None` when it cannot be held exactly.
fn total(sums: &[Num]) -> Option<Total> {
    if sums.is_empty() {
        return Some(Total::Null);
    }
    if let Some(integers) = sums
        .iter()
        .map(|sum| match sum {
            Num::Int(integer) => Some(*integer),
            Num::Float(_) => None,
        })
        .collect::<Option<Vec<i128>>>()
    {
        return integers
            .into_iter()
            .try_fold(0_i128, i128::checked_add)
            .map(Total::Integer);
    }
    let mut total = Decimal::ZERO;
    for sum in sums {
        total = exact_sum(total, decimal(*sum)?)?;
    }
    Some(Total::Real(total))
}

/// `a + b`, or `None` when the decimal would round it.
// NOTE: no specification governs this: our own design; a sum that keeps the
// larger scale of its terms was not rounded to fit the 96-bit mantissa.
fn exact_sum(a: Decimal, b: Decimal) -> Option<Decimal> {
    let sum = a.checked_add(b)?;
    (sum.scale() >= a.scale().max(b.scale())).then_some(sum)
}

/// The decimal a node number stands for, or `None` past what the decimal
/// holds exactly.
// NOTE: a real arrives as the binary64 nearest the node's text, whose shortest
// round-trip text (the standard library's Display) is that text for up to 15 digits.
fn decimal(number: Num) -> Option<Decimal> {
    match number {
        Num::Int(integer) => Decimal::try_from_i128_with_scale(integer, 0).ok(),
        Num::Float(float) => Decimal::from_str_exact(&float.to_string()).ok(),
    }
}

/// The answer whose `MIN` (`wanted` less) or `MAX` (`wanted` greater) value
/// the Tier comparator picks, or `None` when no node holds one. The first
/// node in endpoint order wins a tie.
fn chosen(answers: &[Answer], index: usize, wanted: Ordering) -> Option<&Answer> {
    let mut best: Option<(&Cell, &Answer)> = None;
    for answer in answers {
        if let Some(Part::Extreme(Some(cell))) = answer.parts.get(index)
            && best.is_none_or(|(held, _)| cmp_cell(cell, held) == wanted)
        {
            best = Some((cell, answer));
        }
    }
    best.map(|(_, answer)| answer)
}
