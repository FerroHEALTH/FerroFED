// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two AQL forms that scope a query to one `ehr_id` (N29), brought to
//! the canonical one before anything reads the query.
//!
//! `FROM EHR e[ehr_id/value='…']` and `WHERE e/ehr_id/value = '…'` are
//! semantically equivalent (N29), and the canonical form of the
//! specification is the `WHERE` predicate (§7.1). [`canonical`] moves the
//! class predicate of an `EHR` into the top-level `AND` chain of `WHERE`, so
//! the rest of the rewrite, the hygiene value test included, reads one form,
//! and both forms reach a node as the same query.

use openehr_query::ast::{
    ClassExprOperand, CompareOperand, ContainsExpr, IdentifiedExpr, IdentifiedPath, NodePredicate,
    ObjectPath, PathPart, PathPredicate, PathPredicateOperand, Primitive, SelectQuery,
    StandardPredicate, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;

use super::rewrite::{fresh_name, variables};
use super::scan::Findings;

/// The one `ehr_id` the top-level `ehr_id` predicates of `findings` name,
/// for a query over exactly one `EHR` variable, or `None` (N29).
pub(super) fn of(findings: &Findings) -> Option<String> {
    let [_] = findings.ehr.as_slice() else {
        return None;
    };
    let mut named = findings.ehr_ids.iter();
    let first = named.next()?.as_ref()?;
    named
        .all(|other| other.as_ref() == Some(first))
        .then(|| first.clone())
}

/// Moves every `[ehr_id/value = '<literal>']` class predicate of an `EHR`
/// in the conjunctive containment into `WHERE` as
/// `<var>/ehr_id/value = '<literal>'` (§7.1, N29).
///
/// An `EHR` written with no variable is given a fresh one. A predicate of
/// any other shape, and an `EHR` under `OR` or `NOT CONTAINS`, is left as
/// written: moving it there would change which rows the query selects.
pub(super) fn canonical(query: &mut SelectQuery) {
    let mut moved = Vec::new();
    let mut taken = Vec::new();
    variables(&query.from, &mut taken);
    lift(&mut query.from, &mut taken, &mut moved);
    for leaf in moved {
        if !already_written(query.where_.as_ref(), &leaf) {
            query.where_ = Some(match query.where_.take() {
                Some(existing) => prepend(existing, leaf),
                None => WhereExpr::identified(leaf),
            });
        }
    }
}

/// Lifts the `ehr_id` predicates out of the conjunctive classes of `from`,
/// naming any unnamed `EHR` with a variable none of `taken` is.
fn lift(from: &mut ContainsExpr, taken: &mut Vec<String>, moved: &mut Vec<IdentifiedExpr>) {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            if let ClassExprOperand::Class {
                rm_type,
                variable,
                predicate,
            } = operand
                && rm_type == "EHR"
                && let Some(value) = predicate.as_ref().and_then(ehr_id_literal)
            {
                let name = variable
                    .get_or_insert_with(|| {
                        let name = fresh_name(taken);
                        taken.push(name.clone());
                        name
                    })
                    .clone();
                *predicate = None;
                moved.push(ehr_id_equals(&name, value));
            }
            if let Some(constraint) = contains
                && !constraint.negated
            {
                lift(&mut constraint.expr, taken, moved);
            }
        }
        ContainsExpr::And(left, right) => {
            lift(left, taken, moved);
            lift(right, taken, moved);
        }
        ContainsExpr::Or(..) => {}
    }
}

/// The literal of an `[ehr_id/value = '<literal>']` predicate, or `None` for
/// a predicate of any other shape.
fn ehr_id_literal(predicate: &PathPredicate) -> Option<String> {
    let standard = match predicate {
        PathPredicate::Standard(standard) => standard,
        PathPredicate::Node(node) => match node.as_ref() {
            NodePredicate::Standard(standard) => standard,
            _ => return None,
        },
        PathPredicate::Archetype(_) => return None,
    };
    let StandardPredicate { path, op, operand } = standard.as_ref();
    let names: Vec<&str> = path.parts.iter().map(|part| part.name.as_str()).collect();
    let bare = path.parts.iter().all(|part| part.predicate.is_none());
    match operand {
        PathPredicateOperand::Primitive(Primitive::String(value))
            if *op == CompOp::Eq && bare && names == ["ehr_id", "value"] =>
        {
            Some(value.clone())
        }
        _ => None,
    }
}

/// `<variable>/ehr_id/value = '<value>'`.
fn ehr_id_equals(variable: &str, value: String) -> IdentifiedExpr {
    let parts = ["ehr_id", "value"]
        .into_iter()
        .map(|name| PathPart {
            name: name.to_owned(),
            predicate: None,
        })
        .collect();
    IdentifiedExpr::Compare {
        lhs: CompareOperand::Path(IdentifiedPath::new(
            variable.to_owned(),
            None,
            Some(ObjectPath { parts }),
        )),
        op: CompOp::Eq,
        rhs: Terminal::Primitive(Primitive::String(value)),
    }
}

/// Whether the top-level `AND` chain of `where_` already holds `leaf`.
fn already_written(where_: Option<&WhereExpr>, leaf: &IdentifiedExpr) -> bool {
    match where_ {
        Some(WhereExpr::And(left, right)) => {
            already_written(Some(left), leaf) || already_written(Some(right), leaf)
        }
        Some(WhereExpr::Identified(written, _)) => written == leaf,
        Some(WhereExpr::Not(_) | WhereExpr::Or(..)) | None => false,
    }
}

/// `leaf AND existing`, with `leaf` as the first leaf of the top-level `AND`
/// chain, the tree a parse of the canonical form builds.
fn prepend(existing: WhereExpr, leaf: IdentifiedExpr) -> WhereExpr {
    match existing {
        WhereExpr::And(left, right) => WhereExpr::And(Box::new(prepend(*left, leaf)), right),
        first => WhereExpr::And(Box::new(WhereExpr::identified(leaf)), Box::new(first)),
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::parser::parse_str;
    use openehr_query::printer::to_aql;

    use super::canonical;

    const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

    fn canonical_of(aql: &str) -> String {
        let mut query = parse_str(aql).unwrap();
        canonical(&mut query);
        to_aql(&query)
    }

    fn printed(aql: &str) -> String {
        to_aql(&parse_str(aql).unwrap())
    }

    #[test]
    fn the_class_predicate_becomes_the_where_predicate() {
        let from = format!(
            "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c \
             WHERE c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1' \
             AND c/name/value = 'Visit'"
        );
        let where_ = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_id/value = '{EHR_ID}' \
             AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1' \
             AND c/name/value = 'Visit'"
        );
        assert_eq!(printed(&where_), canonical_of(&from));
    }

    #[test]
    fn a_query_with_no_where_clause_gains_one() {
        let from = format!(
            "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c"
        );
        let where_ = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'"
        );
        assert_eq!(printed(&where_), canonical_of(&from));
    }

    #[test]
    fn an_unnamed_ehr_is_given_a_fresh_variable() {
        let from =
            format!("SELECT e/uid/value FROM EHR[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION e");
        let where_ = format!(
            "SELECT e/uid/value FROM EHR e1 CONTAINS COMPOSITION e WHERE e1/ehr_id/value = '{EHR_ID}'"
        );
        assert_eq!(printed(&where_), canonical_of(&from));
    }

    #[test]
    fn the_canonical_form_is_left_as_written() {
        let where_ = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'"
        );
        assert_eq!(printed(&where_), canonical_of(&where_));
    }

    #[test]
    fn both_forms_together_are_one_predicate() {
        let both = format!(
            "SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] CONTAINS COMPOSITION c \
             WHERE e/ehr_id/value = '{EHR_ID}'"
        );
        let where_ = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'"
        );
        assert_eq!(printed(&where_), canonical_of(&both));
    }

    #[test]
    fn a_predicate_of_another_shape_stays_in_from() {
        for aql in [
            format!(
                "SELECT c/uid/value FROM EHR e[ehr_id/value!='{EHR_ID}'] CONTAINS COMPOSITION c"
            ),
            "SELECT c/uid/value FROM EHR e[ehr_id/value=1] CONTAINS COMPOSITION c".to_owned(),
            format!(
                "SELECT c/uid/value FROM EHR e[system_id/value='{EHR_ID}'] CONTAINS COMPOSITION c"
            ),
            format!("SELECT c/uid/value FROM COMPOSITION c[ehr_id/value='{EHR_ID}']"),
        ] {
            assert_eq!(printed(&aql), canonical_of(&aql), "{aql}");
        }
    }

    #[test]
    fn an_ehr_under_or_or_not_contains_stays_in_from() {
        for aql in [
            format!(
                "SELECT c/uid/value FROM COMPOSITION c NOT CONTAINS EHR e[ehr_id/value='{EHR_ID}']"
            ),
            format!("SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_ID}'] OR COMPOSITION c"),
        ] {
            let query = parse_str(&aql).unwrap();
            let mut moved = query.clone();
            canonical(&mut moved);
            assert_eq!(query, moved, "{aql}");
        }
    }
}
