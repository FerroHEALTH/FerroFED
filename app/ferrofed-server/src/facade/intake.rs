// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query-intake seam: `query_parameters` to typed AQL literals.
//!
//! It runs once, at the façade boundary and before `bind`
//! (`.claude/rules/rust-style.md`, seam 1; `docs/architecture.md` section 2,
//! decision A2).
//!
//! A string becomes a `String` literal, a number that fits `i64` an
//! `Integer`, any other finite number a `Real`, and a boolean a `Boolean`.
//! `null`, an array, an object and an integral number past `i64` are refused,
//! naming the parameter and never its value (AQL §Parameters types a
//! parameter as the literal it stands for).
#![expect(
    clippy::disallowed_types,
    reason = "the query-intake seam: ITS-REST types query_parameters as JSON values"
)]

use openehr_its::rest::generated::query::QueryParameters;
use openehr_query::ast::Primitive;
use openehr_query::bind::Parameters;
use serde_json::Value;

/// A query parameter that is not an AQL literal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IntakeError {
    /// The parameter is `null`, an array or an object.
    #[error("the query parameter {name:?} is not a string, a number or a boolean")]
    NotALiteral {
        /// The parameter's name.
        name: String,
    },
    /// The parameter is an integral number outside the AQL integer range.
    #[error("the query parameter {name:?} is an integer outside the 64-bit range")]
    OutOfRange {
        /// The parameter's name.
        name: String,
    },
}

/// The typed parameters of `supplied`, ready for `bind`.
///
/// # Errors
/// Returns an [`IntakeError`] naming the first parameter, in name order, that
/// is not a literal.
pub fn parameters(supplied: Option<&QueryParameters>) -> Result<Parameters, IntakeError> {
    let mut parameters = Parameters::new();
    let Some(supplied) = supplied else {
        return Ok(parameters);
    };
    for (name, value) in supplied {
        parameters.insert(name, literal(name, value)?);
    }
    Ok(parameters)
}

/// The literal `value` stands for.
fn literal(name: &str, value: &Value) -> Result<Primitive, IntakeError> {
    match value {
        Value::String(text) => Ok(Primitive::String(text.clone())),
        Value::Bool(flag) => Ok(Primitive::Boolean(*flag)),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                return Ok(Primitive::Integer(integer));
            }
            if number.is_u64() {
                return Err(IntakeError::OutOfRange {
                    name: name.to_owned(),
                });
            }
            number
                .as_f64()
                .map(Primitive::Real)
                .ok_or_else(|| IntakeError::NotALiteral {
                    name: name.to_owned(),
                })
        }
        Value::Null | Value::Array(_) | Value::Object(_) => Err(IntakeError::NotALiteral {
            name: name.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{IntakeError, parameters};
    use openehr_its::rest::generated::query::QueryParameters;

    fn supplied(json: &str) -> QueryParameters {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn each_json_scalar_becomes_its_literal() {
        let bound = parameters(Some(&supplied(
            r#"{"s":"x","i":42,"r":1.5,"b":true,"n":-7}"#,
        )))
        .unwrap();
        assert_eq!(
            bound.names().collect::<Vec<_>>(),
            ["b", "i", "n", "r", "s"],
            "every parameter is supplied"
        );
    }

    #[test]
    fn a_null_an_array_and_an_object_are_refused_by_name_only() {
        for (json, name) in [
            (r#"{"p":null}"#, "p"),
            (r#"{"q":[1]}"#, "q"),
            (r#"{"r":{"v":"SENTINEL-intake"}}"#, "r"),
        ] {
            let refused = parameters(Some(&supplied(json))).unwrap_err();
            assert_eq!(
                refused,
                IntakeError::NotALiteral {
                    name: name.to_owned()
                },
                "{json}"
            );
            assert!(
                !refused.to_string().contains("SENTINEL"),
                "the refusal never quotes a value: {refused}"
            );
        }
    }

    #[test]
    fn an_integer_past_i64_is_refused_rather_than_rounded() {
        let refused = parameters(Some(&supplied(r#"{"big":18446744073709551615}"#))).unwrap_err();
        assert_eq!(
            refused,
            IntakeError::OutOfRange {
                name: "big".to_owned()
            },
            "a u64 past i64::MAX is not silently a Real"
        );
    }

    #[test]
    fn no_parameters_is_an_empty_set() {
        assert!(parameters(None).unwrap().is_empty(), "nothing supplied");
    }
}
