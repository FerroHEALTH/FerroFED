// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The result-cell seam: a node's `RESULT_SET` cell decoded into the typed
//! value the Tier comparator orders. ITS-REST types a cell as any JSON value;
//! the typed seam is our own design, since no specification governs it.
//!
//! A cell is a boolean, a number, a temporal value, a string, an RM
//! `DV_ORDERED` object decoded through `openehr-its` canonical JSON, any other
//! JSON, or null. The decoded cell keeps its canonical JSON text, the
//! fallback of rule 4 of the Tier comparator. The cells of a recombined
//! aggregate are encoded here too, so no other module of the merge sees a
//! `Value`.
#![expect(
    clippy::disallowed_types,
    reason = "the result-cell seam: ITS-REST types a RESULT_SET cell as a JSON value"
)]

use std::collections::BTreeMap;

use openehr_base::v1_3::foundation_types::time::iso8601_date_time::Iso8601DateTime;
use openehr_its::json::from_canonical_value;
use openehr_rm::v1_2::data_types::quantity::dv_ordered::DvOrdered;
use rust_decimal::Decimal;
use serde_json::{Number, Value};

/// One decoded cell, in the class order of the Tier comparator: boolean,
/// number, temporal, string, data value, other JSON, and null last.
#[derive(Debug, Clone)]
pub(super) enum Cell {
    /// A JSON boolean.
    Bool(bool),
    /// A JSON number.
    Number(Num),
    /// A string that is a complete ISO 8601 date-time.
    Temporal(Temporal),
    /// Any other string.
    Text(String),
    /// An RM `DV_ORDERED` value.
    Data(Box<Data>),
    /// Any other JSON, by its canonical text.
    Other(String),
    /// JSON `null`.
    Null,
}

impl Cell {
    /// The class rank of rule 2 of the Tier comparator.
    pub(super) fn rank(&self) -> u8 {
        match self {
            Self::Bool(_) => 0,
            Self::Number(_) => 1,
            Self::Temporal(_) => 2,
            Self::Text(_) => 3,
            Self::Data(_) => 4,
            Self::Other(_) => 5,
            Self::Null => 6,
        }
    }
}

/// A JSON number as the reader holds it: an integer, exactly, or a binary
/// float.
// NOTE: the JSON reader keeps a fractional number as an f64 (the workspace does
// not enable serde_json's arbitrary_precision), so a literal finer than f64
// arrives rounded; no integer is ever compared through f64.
#[derive(Debug, Clone, Copy)]
pub(super) enum Num {
    /// An integer that fits `i64` or `u64`.
    Int(i128),
    /// Any other number.
    Float(f64),
}

/// A complete ISO 8601 date-time, keyed on its instant.
#[derive(Debug, Clone)]
pub(super) struct Temporal {
    /// Whether the value carries an offset; zoned and unzoned values are
    /// incomparable in `openehr-base`, so zoned ones sort first (FerroFED's
    /// own).
    pub(super) zoned: bool,
    /// Nanoseconds from the epoch of the same zonedness, on the UTC axis for
    /// a zoned value.
    pub(super) nanos: i128,
    /// The value as written, the tie-break between two spellings of one
    /// instant.
    pub(super) text: String,
}

/// A decoded `DV_ORDERED` value.
#[derive(Debug, Clone)]
pub(super) struct Data {
    /// The RM value.
    pub(super) value: DvOrdered,
    /// Its canonical JSON text.
    pub(super) json: String,
    /// The rank of its comparability class among the values of the answer,
    /// assigned by [`super::compare::rank_classes`].
    pub(super) class: usize,
    /// Whether its magnitude is available (`less_than` decides with itself).
    pub(super) measured: bool,
}

/// Decodes one cell.
pub(super) fn decode(value: &Value) -> Cell {
    match value {
        Value::Null => Cell::Null,
        Value::Bool(flag) => Cell::Bool(*flag),
        Value::Number(number) => number_cell(number),
        Value::String(text) => {
            temporal(text).map_or_else(|| Cell::Text(text.clone()), Cell::Temporal)
        }
        Value::Object(_) => data(value).map_or_else(
            || Cell::Other(canonical(value)),
            |data| Cell::Data(Box::new(data)),
        ),
        Value::Array(_) => Cell::Other(canonical(value)),
    }
}

fn number_cell(number: &Number) -> Cell {
    if let Some(integer) = number.as_i64() {
        Cell::Number(Num::Int(i128::from(integer)))
    } else if let Some(integer) = number.as_u64() {
        Cell::Number(Num::Int(i128::from(integer)))
    } else if let Some(float) = number.as_f64() {
        Cell::Number(Num::Float(float))
    } else {
        // NOTE: a Number is always one of the three; the text is the fallback.
        Cell::Other(number.to_string())
    }
}

/// The instant of a complete date-time, through `openehr-base`'s `diff`
/// against the epoch of the same zonedness, or `None` for any other string.
// NOTE: a string that is not a complete date-time is legitimately a string, not
// a defect, so a failed parse is the answer and not an error.
fn temporal(text: &str) -> Option<Temporal> {
    let value = Iso8601DateTime {
        value: text.to_owned(),
    };
    if value.is_partial() {
        return None;
    }
    let zoned = value.timezone().is_some();
    let epoch = Iso8601DateTime {
        value: if zoned {
            "1970-01-01T00:00:00Z".to_owned()
        } else {
            "1970-01-01T00:00:00".to_owned()
        },
    };
    let since = value.diff(&epoch)?;
    if since.years()? != 0 || since.months()? != 0 {
        return None;
    }
    let days = i128::from(since.weeks()?)
        .checked_mul(7)?
        .checked_add(i128::from(since.days()?))?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(i128::from(since.hours()?).checked_mul(3_600)?)?
        .checked_add(i128::from(since.minutes()?).checked_mul(60)?)?
        .checked_add(i128::from(since.seconds()?))?;
    let magnitude = seconds
        .checked_mul(1_000_000_000)?
        .checked_add(fraction_nanos(since.fractional_seconds()?)?)?;
    Some(Temporal {
        zoned,
        nanos: if since.is_negative()? {
            magnitude.checked_neg()?
        } else {
            magnitude
        },
        text: text.to_owned(),
    })
}

/// The nanoseconds of a fraction of a second in `[0, 1)`.
fn fraction_nanos(fraction: f64) -> Option<i128> {
    let nanos = (fraction * 1e9).round();
    if !(0.0..1e9).contains(&nanos) {
        return None;
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "an integral f64 in [0, 1e9) converts to i128 exactly"
    )]
    let whole = nanos as i128;
    Some(whole)
}

/// A `DV_ORDERED` object, decoded through `openehr-its` canonical JSON, or
/// `None` for any other object.
// NOTE: an object that is not a DV_ORDERED (a DV_TEXT, an RM structure) is other
// JSON by rule 4, not a defect, so a failed decode is the answer.
fn data(value: &Value) -> Option<Data> {
    let decoded: DvOrdered = from_canonical_value(value).ok()?;
    let measured = decoded.less_than(&decoded).is_some();
    Some(Data {
        value: decoded,
        json: canonical(value),
        class: 0,
        measured,
    })
}

/// A JSON `null` cell, the recombined aggregate over no value (AQL 1.1.0
/// §Aggregate functions).
pub(super) fn null() -> Value {
    Value::Null
}

/// An integer cell, or `None` past the integers a JSON number holds exactly
/// here (`i64` and `u64`).
pub(super) fn integer(value: i128) -> Option<Value> {
    match i64::try_from(value) {
        Ok(signed) => Some(Value::from(signed)),
        Err(_) => u64::try_from(value).ok().map(Value::from),
    }
}

/// A real cell holding `value` exactly, or `None` when no binary64, the
/// number the writer keeps, reads back as `value`.
pub(super) fn exact_real(value: Decimal) -> Option<Value> {
    let float = nearest(value)?;
    let back = Decimal::from_str_exact(&float.to_string()).ok()?;
    (back == value)
        .then(|| Number::from_f64(float).map(Value::Number))
        .flatten()
}

/// The real cell nearest `value`.
pub(super) fn nearest_real(value: Decimal) -> Option<Value> {
    Number::from_f64(nearest(value)?).map(Value::Number)
}

/// The binary64 nearest `value`, read from its decimal text, which the
/// standard library rounds correctly.
fn nearest(value: Decimal) -> Option<f64> {
    value
        .to_string()
        .parse::<f64>()
        .ok()
        .filter(|float| float.is_finite())
}

/// The canonical JSON text of `value`: object members in key order at every
/// depth, the rest as the reader holds it.
pub(super) fn canonical(value: &Value) -> String {
    match value {
        Value::Object(members) => {
            let sorted: BTreeMap<&String, &Value> = members.iter().collect();
            let body: Vec<String> = sorted
                .into_iter()
                .map(|(key, member)| {
                    format!("{}:{}", Value::String(key.clone()), canonical(member))
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", body.join(","))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, decode};
    use serde_json::json;

    #[test]
    fn a_complete_date_time_is_temporal_and_honours_its_offset() {
        let Cell::Temporal(later) = decode(&json!("2026-01-01T10:00:00+02:00")) else {
            panic!("a complete zoned date-time")
        };
        let Cell::Temporal(earlier) = decode(&json!("2026-01-01T09:00:00Z")) else {
            panic!("a complete zoned date-time")
        };
        assert!(
            later.nanos < earlier.nanos,
            "10:00+02:00 is 08:00Z, before 09:00Z, which a string comparison gets wrong"
        );
    }

    #[test]
    fn a_partial_date_time_or_a_date_is_a_string() {
        assert!(matches!(decode(&json!("2026-01")), Cell::Text(_)));
        assert!(matches!(decode(&json!("2026-01-01")), Cell::Text(_)));
        assert!(matches!(decode(&json!("abc")), Cell::Text(_)));
    }

    #[test]
    fn the_fraction_is_kept_to_the_nanosecond() {
        let (Cell::Temporal(a), Cell::Temporal(b)) = (
            decode(&json!("2026-01-01T00:00:00.000000001Z")),
            decode(&json!("2026-01-01T00:00:00.000000002Z")),
        ) else {
            panic!("complete date-times")
        };
        assert_eq!(b.nanos - a.nanos, 1, "one nanosecond apart");
    }

    #[test]
    fn a_dv_quantity_decodes_through_canonical_json() {
        let cell = decode(&json!({"_type": "DV_QUANTITY", "magnitude": 72.0, "units": "kg"}));
        assert!(
            matches!(&cell, Cell::Data(data) if data.measured),
            "{cell:?}"
        );
    }

    #[test]
    fn an_object_that_is_no_dv_ordered_is_other_json_in_key_order() {
        let cell = decode(&json!({"b": 1, "a": [true, null]}));
        assert!(
            matches!(&cell, Cell::Other(text) if text == r#"{"a":[true,null],"b":1}"#),
            "{cell:?}"
        );
    }
}
