// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shape every stored section query of the exchange format shares.
//!
//! A section query selects, once per member, every whole composition that
//! contains one of a set of archetypes, with its uid and its template id,
//! because a FHIRconnect mapping is chosen per template:
//!
//! ```text
//! SELECT DISTINCT c AS composition, c/uid/value AS uid,
//!   c/archetype_details/template_id/value AS template_id
//! FROM EHR e CONTAINS COMPOSITION c
//!   CONTAINS (OBSERVATION a0[openEHR-EHR-OBSERVATION.laboratory_test_result.v1] OR …)
//! WHERE e/ehr_status/subject/external_ref/id/value = $patient
//!   AND e/ehr_status/subject/external_ref/namespace = $namespace
//! ```
//!
//! `DISTINCT` returns a composition once however many of its entries match
//! (AQL §DISTINCT). The patient is a parameter, so the text holds no
//! identifier, and the rewrite replaces the two predicates with each
//! member's own `ehr_id` (§7.1, §5.4.1, N33). Each category names its
//! archetypes from an openEHR template ([`crate::patient_summary`],
//! [`crate::laboratory`]).

use openehr_query::ast::{
    ArchetypePredicate, ClassExprOperand, ColumnExpr, CompareOperand, ContainsConstraint,
    ContainsExpr, IdentifiedExpr, IdentifiedPath, ObjectPath, PathPart, PathPredicate,
    SelectClause, SelectExpr, SelectQuery, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;

/// The parameter that carries the patient identifier, as its
/// `query_parameters` key names it.
pub const PATIENT: &str = "patient";

/// The parameter that carries the namespace that issued the identifier
/// (§5.2), as its `query_parameters` key names it.
pub const ISSUER: &str = "namespace";

/// One archetype a section selects compositions by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Archetype {
    /// The RM class of the archetype's root, the class the `CONTAINS`
    /// expression names.
    pub class: &'static str,
    /// The archetype id.
    pub id: &'static str,
}

/// Returns the section query that selects the compositions containing at
/// least one of `archetypes`, or `None` when `archetypes` is empty.
#[must_use]
pub fn query(archetypes: &[Archetype]) -> Option<SelectQuery> {
    let column = |path: IdentifiedPath, alias: &str| SelectExpr {
        column: ColumnExpr::Path(path),
        alias: Some(alias.to_owned()),
    };
    let entries = entries(archetypes)?;
    Some(SelectQuery {
        select: SelectClause {
            distinct: true,
            top: None,
            columns: vec![
                column(identified("c", &[]), "composition"),
                column(identified("c", &["uid", "value"]), "uid"),
                column(
                    identified("c", &["archetype_details", "template_id", "value"]),
                    "template_id",
                ),
            ],
        },
        from: contained(
            class("EHR", "e", None),
            Some(contained(class("COMPOSITION", "c", None), Some(entries))),
        ),
        where_: Some(WhereExpr::And(
            Box::new(subject(&["id", "value"], PATIENT)),
            Box::new(subject(&["namespace"], ISSUER)),
        )),
        order_by: Vec::new(),
        limit: None,
    })
}

/// The `CONTAINS` expression under the composition: one class expression
/// per archetype, joined by `OR`, or `None` for no archetype.
fn entries(archetypes: &[Archetype]) -> Option<ContainsExpr> {
    let (first, rest) = archetypes.split_first()?;
    Some(
        rest.iter()
            .zip(1_usize..)
            .fold(leaf(first, 0), |joined, (archetype, index)| {
                ContainsExpr::Or(Box::new(joined), Box::new(leaf(archetype, index)))
            }),
    )
}

/// The class expression of `archetype`, bound to the variable `a{index}`.
fn leaf(archetype: &Archetype, index: usize) -> ContainsExpr {
    let predicate = PathPredicate::Archetype(ArchetypePredicate::Hrid(archetype.id.to_owned()));
    contained(
        class(archetype.class, &format!("a{index}"), Some(predicate)),
        None,
    )
}

/// The path `root/part/…`, with no predicate.
fn identified(root: &str, parts: &[&str]) -> IdentifiedPath {
    let path = (!parts.is_empty()).then(|| ObjectPath {
        parts: parts
            .iter()
            .map(|name| PathPart {
                name: (*name).to_owned(),
                predicate: None,
            })
            .collect(),
    });
    IdentifiedPath::new(root.to_owned(), None, path)
}

/// The class expression `rm_type variable[predicate]`.
fn class(rm_type: &str, variable: &str, predicate: Option<PathPredicate>) -> ClassExprOperand {
    ClassExprOperand::Class {
        rm_type: rm_type.to_owned(),
        variable: Some(variable.to_owned()),
        predicate,
    }
}

/// `operand`, containing `inner` when given.
fn contained(operand: ClassExprOperand, inner: Option<ContainsExpr>) -> ContainsExpr {
    ContainsExpr::Contained {
        operand,
        contains: inner.map(|expr| {
            Box::new(ContainsConstraint {
                negated: false,
                expr,
            })
        }),
    }
}

/// `e/ehr_status/subject/external_ref/{tail} = ${parameter}`, the AST
/// holding a parameter with its `$` as the lexer reads it.
fn subject(tail: &[&str], parameter: &str) -> WhereExpr {
    let mut parts = vec!["ehr_status", "subject", "external_ref"];
    parts.extend_from_slice(tail);
    WhereExpr::identified(IdentifiedExpr::Compare {
        lhs: CompareOperand::Path(identified("e", &parts)),
        op: CompOp::Eq,
        rhs: Terminal::Parameter(format!("${parameter}")),
    })
}
