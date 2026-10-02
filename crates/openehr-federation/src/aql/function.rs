// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions AQL 1.1.0 defines, as single-row or aggregate (AQL
//! `master03-syntax.adoc` §Functions), and the calls the rewrite cannot
//! classify.
//!
//! `openehr-query` already tells two of the classes apart: an aggregate
//! (`COUNT`, `MIN`, `MAX`, `SUM`, `AVG`) is an `AggregateCall`, and
//! `TERMINOLOGY` is a `FunctionCall::Terminology`. Every other call is a
//! `FunctionCall::Named` that carries only its name, which this module reads
//! against the single-row functions of the specification. A named call outside
//! that set may aggregate, and nothing in the query says whether it does.

use openehr_query::ast::FunctionCall;

// TODO(#195): replace this list with openehr-query's classification of function calls (FerroHEALTH/FerroEHR#3529)
// NOTE: AQL master03-syntax §String functions, §Numeric functions and §Date and time functions
// list exactly these single-row functions; AqlLexer.g4 groups them as the three `*_FUNCTION_ID`s.
const SINGLE_ROW: [&str; 16] = [
    "LENGTH",
    "CONTAINS",
    "POSITION",
    "SUBSTRING",
    "CONCAT",
    "CONCAT_WS",
    "ABS",
    "MOD",
    "CEIL",
    "FLOOR",
    "ROUND",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_DATE_TIME",
    "NOW",
    "CURRENT_TIMEZONE",
];

/// Whether `call` is a single-row function AQL 1.1.0 defines: one of the
/// string, numeric, or date and time functions by its case-insensitive name,
/// or `TERMINOLOGY` (AQL master03-syntax §Other functions).
///
/// A single-row function returns "a single result for every row of the result
/// set" (AQL master03-syntax §Functions), so each node computes it over its
/// own rows and the merge keeps those rows as rows.
pub(super) fn single_row(call: &FunctionCall) -> bool {
    match call {
        FunctionCall::Named { name, .. } => SINGLE_ROW
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known)),
        FunctionCall::Terminology(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::ast::{ColumnExpr, FunctionCall, SelectQuery};
    use openehr_query::parser::parse_str;

    use super::single_row;

    fn parsed(select: &str) -> SelectQuery {
        parse_str(&format!(
            "SELECT {select} FROM EHR e CONTAINS COMPOSITION c"
        ))
        .unwrap_or_else(|error| panic!("{select}: {error}"))
    }

    fn call(select: &str) -> FunctionCall {
        match parsed(select)
            .select
            .columns
            .into_iter()
            .next()
            .map(|c| c.column)
        {
            Some(ColumnExpr::Function(call)) => call,
            other => panic!("{select} is a function call, got {other:?}"),
        }
    }

    /// Every single-row function of master03-syntax §Functions, one call per
    /// section, in the section's order.
    const DEFINED: [&str; 16] = [
        "LENGTH(c/name/value)",
        "CONTAINS(c/name/value, 'a')",
        "POSITION('a', c/name/value)",
        "SUBSTRING(c/name/value, 1, 2)",
        "CONCAT(c/name/value, 'a')",
        "CONCAT_WS('-', c/name/value, 'a')",
        "ABS(c/context/start_time/magnitude)",
        "MOD(c/context/start_time/magnitude, 2)",
        "CEIL(c/context/start_time/magnitude)",
        "FLOOR(c/context/start_time/magnitude)",
        "ROUND(c/context/start_time/magnitude, 1)",
        "CURRENT_DATE()",
        "CURRENT_TIME()",
        "CURRENT_DATE_TIME()",
        "NOW()",
        "CURRENT_TIMEZONE()",
    ];

    #[test]
    fn every_single_row_function_is_defined_in_any_case() {
        for select in DEFINED {
            assert!(single_row(&call(select)), "{select}");
            assert!(single_row(&call(&select.to_ascii_lowercase())), "{select}");
        }
    }

    #[test]
    fn terminology_is_a_single_row_function() {
        assert!(single_row(&call(
            "TERMINOLOGY('expand', 'hl7.org/fhir/4.0', 'http://snomed.info/sct?fhir_vs=isa/50697003')"
        )));
    }

    #[test]
    fn the_aggregates_are_classified_by_the_parser() {
        for select in [
            "COUNT(*)",
            "COUNT(DISTINCT c/uid/value)",
            "MIN(c/uid/value)",
            "MAX(c/uid/value)",
            "SUM(c/uid/value)",
            "AVG(c/uid/value)",
        ] {
            let column = parsed(select).select.columns.into_iter().next();
            assert!(
                matches!(column.map(|c| c.column), Some(ColumnExpr::Aggregate(_))),
                "{select}"
            );
        }
    }

    #[test]
    fn a_function_outside_aql_is_not_defined() {
        for select in [
            "MEDIAN(c/uid/value)",
            "STDDEV(c/uid/value)",
            "LENGTHS(c/name/value)",
            "COUNT_DISTINCT(c/uid/value)",
        ] {
            assert!(!single_row(&call(select)), "{select}");
        }
    }
}
