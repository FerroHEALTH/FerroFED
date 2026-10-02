// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Folding of the AQL string functions that build a string from their
//! arguments: `CONCAT`, `CONCAT_WS` and `SUBSTRING` (AQL `master03-syntax.adoc`
//! §String functions).
//!
//! A node evaluates `CONCAT('47', '11')` to `'4711'`, so a value test that
//! reads each literal on its own lets the identifier through in pieces
//! (§5.4.1 "in any position"). The rewrite folds these calls over
//! their literal arguments and tests the folded text as well.

use openehr_query::ast::{BuiltinFunction, FunctionCall, Primitive, StringFunction, Terminal};

/// The string a call of `CONCAT`, `CONCAT_WS` or `SUBSTRING` evaluates to when
/// every argument is a literal or itself folds, or `None`.
///
/// An integer argument folds as its decimal text, which is how a node that
/// coerces it would render it; AQL types the arguments as `String`, and
/// reading the coercion keeps the value test the stricter of the two.
pub(super) fn fold(call: &FunctionCall) -> Option<String> {
    let FunctionCall::Builtin {
        function: BuiltinFunction::String(function),
        args,
        ..
    } = call
    else {
        return None;
    };
    match function {
        StringFunction::Concat => {
            if args.is_empty() {
                return None;
            }
            args.iter().map(text).collect()
        }
        StringFunction::ConcatWs => {
            let (separator, parts) = args.split_first()?;
            if parts.is_empty() {
                return None;
            }
            let separator = text(separator)?;
            let parts: Option<Vec<String>> = parts.iter().map(text).collect();
            Some(parts?.join(&separator))
        }
        StringFunction::Substring => substring(args),
        StringFunction::Length | StringFunction::Contains | StringFunction::Position => None,
    }
}

/// Whether `call` is a named function, AQL's own or another, with a literal
/// anywhere among its arguments, nested calls included.
pub(super) fn over_a_literal(call: &FunctionCall) -> bool {
    let (FunctionCall::Builtin { args, .. } | FunctionCall::Other { args, .. }) = call else {
        return false;
    };
    args.iter().any(|arg| match arg {
        Terminal::Primitive(Primitive::String(_) | Primitive::Integer(_) | Primitive::Real(_)) => {
            true
        }
        Terminal::Function(inner) => over_a_literal(inner),
        Terminal::Primitive(Primitive::Boolean(_) | Primitive::Null)
        | Terminal::Parameter(_)
        | Terminal::Path(_) => false,
    })
}

/// The text of one argument: a string or integer literal, or a call that
/// folds.
fn text(arg: &Terminal) -> Option<String> {
    match arg {
        Terminal::Primitive(Primitive::String(value)) => Some(value.clone()),
        Terminal::Primitive(Primitive::Integer(value)) => Some(value.to_string()),
        Terminal::Function(inner) => fold(inner),
        Terminal::Primitive(Primitive::Real(_) | Primitive::Boolean(_) | Primitive::Null)
        | Terminal::Parameter(_)
        | Terminal::Path(_) => None,
    }
}

/// `SUBSTRING(expression, position[, length])`: the characters from the
/// 1-based `position`, at most `length` of them.
fn substring(args: &[Terminal]) -> Option<String> {
    let (expression, rest) = args.split_first()?;
    let expression = text(expression)?;
    let (position, length) = match rest {
        [position] => (integer(position)?, None),
        [position, length] => (integer(position)?, Some(integer(length)?)),
        _ => return None,
    };
    let skip = usize::try_from(position.checked_sub(1)?).ok()?;
    let chars = expression.chars().skip(skip);
    Some(match length {
        Some(length) => chars.take(usize::try_from(length).ok()?).collect(),
        None => chars.collect(),
    })
}

fn integer(arg: &Terminal) -> Option<i64> {
    match arg {
        Terminal::Primitive(Primitive::Integer(value)) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::ast::{ColumnExpr, FunctionCall};
    use openehr_query::parser::parse_str;

    use super::{fold, over_a_literal};

    fn call(expression: &str) -> FunctionCall {
        let query = parse_str(&format!("SELECT {expression} FROM EHR e")).unwrap();
        match query.select.columns.into_iter().next().unwrap().column {
            ColumnExpr::Function(call) => call,
            other => panic!("expected a function call, got {other:?}"),
        }
    }

    #[test]
    fn concat_folds_its_literals_in_order() {
        assert_eq!(
            fold(&call("CONCAT('47', '11')")),
            Some("4711".to_owned()),
            "AQL CONCAT"
        );
        assert_eq!(
            fold(&call("concat('47', 11)")),
            Some("4711".to_owned()),
            "case-insensitive, integers as text"
        );
    }

    #[test]
    fn concat_ws_puts_the_separator_between_the_parts() {
        assert_eq!(
            fold(&call("CONCAT_WS('', '47', '11')")),
            Some("4711".to_owned()),
            "an empty separator"
        );
        assert_eq!(
            fold(&call("CONCAT_WS('-', 'a', 'b', 'c')")),
            Some("a-b-c".to_owned()),
            "between each pair"
        );
    }

    #[test]
    fn substring_is_one_based_with_an_optional_length() {
        assert_eq!(
            fold(&call("SUBSTRING('x4711y', 2, 4)")),
            Some("4711".to_owned()),
            "AQL SUBSTRING"
        );
        assert_eq!(
            fold(&call("SUBSTRING('x4711', 2)")),
            Some("4711".to_owned()),
            "to the end"
        );
        assert_eq!(
            fold(&call("SUBSTRING('x4711', 0)")),
            None,
            "position 0 has no defined result"
        );
    }

    #[test]
    fn nested_calls_fold() {
        let nested = "CONCAT(SUBSTRING(CONCAT('x', '47'), 2), CONCAT_WS('', '1', '1'))";
        assert_eq!(
            fold(&call(nested)),
            Some("4711".to_owned()),
            "every level folds"
        );
    }

    #[test]
    fn a_call_over_a_path_or_another_function_does_not_fold() {
        assert_eq!(
            fold(&call("CONCAT('47', c/name/value)")),
            None,
            "a path has no value at the gateway"
        );
        assert_eq!(
            fold(&call("LENGTH('4711')")),
            None,
            "only the string-building functions fold"
        );
        assert!(
            over_a_literal(&call("CONCAT('47', c/name/value)")),
            "but it is over a literal"
        );
        assert!(
            !over_a_literal(&call("CONCAT(c/name/value)")),
            "a call over paths alone is not"
        );
    }
}
