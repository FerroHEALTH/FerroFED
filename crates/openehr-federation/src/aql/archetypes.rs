// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The archetype and template ids a bound query constrains its data to: what
//! a query says it reads, for a reader that classifies the data by its model
//! ids.
//!
//! An archetype predicate scopes "the data source from which the query result
//! data is to be retrieved" (AQL §Archetype predicate), and AQL scopes a
//! query by template with `archetype_details/template_id/value` (AQL §Class
//! expressions). A constraint counts only where it admits data: one under
//! `NOT CONTAINS` or in a `WHERE` term under `NOT` names data the query
//! excludes (AQL §Containment), and only `=` names an id; a `LIKE` or
//! `matches` pattern names a set no reader can enumerate. The branches of an
//! `OR` or an `AND` of containments are all counted. No specification
//! governs what a classifier reads: our own design.

use std::collections::BTreeSet;

use openehr_query::ast::{
    ArchetypePredicate, ClassExprOperand, CompareOperand, ContainsExpr, IdentifiedExpr,
    NodePredicate, ObjectPath, PathPredicate, PathPredicateOperand, Primitive, SelectQuery,
    StandardPredicate, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;

use super::Analysis;

/// The attribute path of an archetype node id.
const ARCHETYPE_NODE_ID: &[&str] = &["archetype_node_id"];

/// The attribute path of a template id.
const TEMPLATE_ID: &[&str] = &["archetype_details", "template_id", "value"];

impl Analysis {
    /// The archetype and template ids the bound façade query constrains its
    /// data to ([`Constrained`]).
    #[must_use]
    pub fn constrained(&self) -> &Constrained {
        match self {
            Self::Patient(query) => &query.constrained,
            Self::Unscoped(query) => &query.constrained,
        }
    }

    /// Holds `constrained` as the constraints of this analysis.
    pub(super) fn constrain(&mut self, constrained: Constrained) {
        match self {
            Self::Patient(query) => query.constrained = constrained,
            Self::Unscoped(query) => query.constrained = constrained,
        }
    }
}

/// The archetype ids and template ids a query constrains its data to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Constrained {
    archetypes: BTreeSet<String>,
    templates: BTreeSet<String>,
}

impl Constrained {
    /// The constraints of the bound `query`.
    #[must_use]
    pub fn of(query: &SelectQuery) -> Self {
        let mut constrained = Self::default();
        constrained.containment(&query.from);
        if let Some(condition) = &query.where_ {
            constrained.condition(condition);
        }
        constrained
    }

    /// The archetype ids, in order.
    #[must_use]
    pub fn archetypes(&self) -> &BTreeSet<String> {
        &self.archetypes
    }

    /// The template ids, in order.
    #[must_use]
    pub fn templates(&self) -> &BTreeSet<String> {
        &self.templates
    }

    /// Whether the query names neither.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.archetypes.is_empty() && self.templates.is_empty()
    }

    fn containment(&mut self, expr: &ContainsExpr) {
        match expr {
            ContainsExpr::Contained { operand, contains } => {
                if let ClassExprOperand::Class {
                    predicate: Some(predicate),
                    ..
                } = operand
                {
                    self.path_predicate(predicate);
                }
                if let Some(constraint) = contains
                    && !constraint.negated
                {
                    self.containment(&constraint.expr);
                }
            }
            ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
                self.containment(left);
                self.containment(right);
            }
        }
    }

    fn path_predicate(&mut self, predicate: &PathPredicate) {
        match predicate {
            PathPredicate::Archetype(ArchetypePredicate::Hrid(id)) => {
                self.archetypes.insert(id.clone());
            }
            PathPredicate::Archetype(ArchetypePredicate::Parameter(_)) => {}
            PathPredicate::Standard(standard) => self.standard(standard),
            PathPredicate::Node(node) => self.node_predicate(node),
        }
    }

    fn node_predicate(&mut self, predicate: &NodePredicate) {
        match predicate {
            NodePredicate::Archetype { hrid, .. } => {
                self.archetypes.insert(hrid.clone());
            }
            NodePredicate::Standard(standard) => self.standard(standard),
            NodePredicate::And(left, right) | NodePredicate::Or(left, right) => {
                self.node_predicate(left);
                self.node_predicate(right);
            }
            NodePredicate::Code { .. }
            | NodePredicate::Parameter(_)
            | NodePredicate::MatchesRegex { .. } => {}
        }
    }

    fn standard(&mut self, standard: &StandardPredicate) {
        if standard.op != CompOp::Eq {
            return;
        }
        if let PathPredicateOperand::Primitive(Primitive::String(value)) = &standard.operand {
            self.named(&standard.path, value);
        }
    }

    fn condition(&mut self, condition: &WhereExpr) {
        match condition {
            WhereExpr::Identified(
                IdentifiedExpr::Compare {
                    lhs: CompareOperand::Path(path),
                    op: CompOp::Eq,
                    rhs: Terminal::Primitive(Primitive::String(value)),
                },
                _,
            ) => {
                if let Some(attribute) = &path.path {
                    self.named(attribute, value);
                }
            }
            WhereExpr::And(left, right) | WhereExpr::Or(left, right) => {
                self.condition(left);
                self.condition(right);
            }
            WhereExpr::Identified(..) | WhereExpr::Not(_) => {}
        }
    }

    /// Records `value` when `path` is the archetype node id or the template
    /// id of the object it is read from.
    fn named(&mut self, path: &ObjectPath, value: &str) {
        let bare = path.parts.iter().all(|part| part.predicate.is_none());
        let names: Vec<&str> = path.parts.iter().map(|part| part.name.as_str()).collect();
        if !bare {
            return;
        }
        if names == ARCHETYPE_NODE_ID {
            self.archetypes.insert(value.to_owned());
        } else if names == TEMPLATE_ID {
            self.templates.insert(value.to_owned());
        }
    }
}
