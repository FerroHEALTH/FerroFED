// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The archetype and template ids a bound query constrains its data to: what
//! a query says it reads, for a reader that classifies the data by its model
//! ids.
//!
//! The ids are read from the bound `openehr-query` syntax tree the rewrite
//! uses, so a comment, which the parser drops, or a string compared with
//! another path names nothing. An archetype predicate scopes "the data
//! source from which the query result data is to be retrieved" (AQL
//! §Archetype predicate), and AQL scopes a query by template with
//! `archetype_details/template_id/value` (AQL §Class expressions). A
//! constraint counts only where it admits data: one under `NOT CONTAINS` or
//! under `NOT` names data the query excludes (AQL §Containment), and only
//! `=` with a string names an id; a `LIKE` or `matches` pattern names a set
//! no reader can enumerate. The ids of every branch of an `OR` are kept.
//!
//! [`Constrained::every_root_bound`] says whether every class the query
//! reads data from is bound to an id: by its own predicate, by an `=` of the
//! top-level `AND` chain of `WHERE`, or by a class it is contained in. A
//! class bound by neither, or by a form this reader does not understand, is
//! not, and a classifier then cannot know what the query read. `EHR` and
//! `VERSION` hold the data of the classes they contain and are read through
//! them. No specification governs what a classifier reads: our own design.

use std::collections::BTreeSet;

use openehr_query::ast::{
    ArchetypePredicate, ClassExprOperand, CompareOperand, ContainsExpr, IdentifiedExpr,
    IdentifiedPath, NodePredicate, ObjectPath, PathPredicate, PathPredicateOperand, Primitive,
    SelectQuery, StandardPredicate, Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;

use super::Analysis;

/// The attribute path of an archetype node id.
const ARCHETYPE_NODE_ID: &[&str] = &["archetype_node_id"];

/// The attribute path of a template id.
const TEMPLATE_ID: &[&str] = &["archetype_details", "template_id", "value"];

/// The class that holds every other class of a query.
const EHR: &str = "EHR";

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

/// The archetype ids and template ids a query constrains its data to, and
/// whether they bind every class it reads.
///
/// The default binds nothing: a query not yet read is not known to be bound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Constrained {
    archetypes: BTreeSet<String>,
    templates: BTreeSet<String>,
    every_root_bound: bool,
}

impl Constrained {
    /// The constraints of the bound `query`.
    #[must_use]
    pub fn of(query: &SelectQuery) -> Self {
        let mut constrained = Self {
            every_root_bound: true,
            ..Self::default()
        };
        let mut bound = BTreeSet::new();
        if let Some(condition) = &query.where_ {
            constrained.condition(condition, true, &mut bound);
        }
        constrained.containment(&query.from, false, &bound);
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

    /// Whether every class the query reads data from is bound to an id.
    #[must_use]
    pub fn every_root_bound(&self) -> bool {
        self.every_root_bound
    }

    /// Reads the containment `expr`, whose classes are bound already when
    /// `covered`, the variables in `bound` bound by `WHERE`.
    fn containment(&mut self, expr: &ContainsExpr, covered: bool, bound: &BTreeSet<String>) {
        match expr {
            ContainsExpr::Contained { operand, contains } => {
                let (container, binds) = match operand {
                    ClassExprOperand::Class { rm_type, .. } if rm_type == EHR => (true, false),
                    ClassExprOperand::Class {
                        variable,
                        predicate,
                        ..
                    } => {
                        let by_predicate = predicate
                            .as_ref()
                            .is_some_and(|predicate| self.path_predicate(predicate));
                        let by_condition = variable
                            .as_ref()
                            .is_some_and(|variable| bound.contains(variable));
                        (false, by_predicate || by_condition)
                    }
                    ClassExprOperand::Version { .. } => (true, false),
                };
                if !container && !binds && !covered {
                    self.every_root_bound = false;
                }
                if let Some(constraint) = contains
                    && !constraint.negated
                {
                    self.containment(&constraint.expr, covered || binds, bound);
                }
            }
            ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
                self.containment(left, covered, bound);
                self.containment(right, covered, bound);
            }
        }
    }

    /// Reads a class predicate, and returns whether it binds the class.
    fn path_predicate(&mut self, predicate: &PathPredicate) -> bool {
        match predicate {
            PathPredicate::Archetype(ArchetypePredicate::Hrid(id)) => {
                self.archetypes.insert(id.clone());
                true
            }
            PathPredicate::Archetype(ArchetypePredicate::Parameter(_)) => false,
            PathPredicate::Standard(standard) => self.standard(standard),
            PathPredicate::Node(node) => self.node_predicate(node),
        }
    }

    /// Reads a node predicate, and returns whether it binds the class: both
    /// branches of an `OR`, either term of an `AND`.
    fn node_predicate(&mut self, predicate: &NodePredicate) -> bool {
        match predicate {
            NodePredicate::Archetype { hrid, .. } => {
                self.archetypes.insert(hrid.clone());
                true
            }
            NodePredicate::Standard(standard) => self.standard(standard),
            NodePredicate::And(left, right) => {
                let left = self.node_predicate(left);
                let right = self.node_predicate(right);
                left || right
            }
            NodePredicate::Or(left, right) => {
                let left = self.node_predicate(left);
                let right = self.node_predicate(right);
                left && right
            }
            NodePredicate::Code { .. }
            | NodePredicate::Parameter(_)
            | NodePredicate::MatchesRegex { .. } => false,
        }
    }

    /// Reads a standard predicate, and returns whether it binds the class.
    fn standard(&mut self, standard: &StandardPredicate) -> bool {
        if standard.op != CompOp::Eq {
            return false;
        }
        match &standard.operand {
            PathPredicateOperand::Primitive(Primitive::String(value)) => {
                self.named(&standard.path, value)
            }
            PathPredicateOperand::Primitive(_)
            | PathPredicateOperand::Path(_)
            | PathPredicateOperand::Parameter(_)
            | PathPredicateOperand::Code(_) => false,
        }
    }

    /// Reads `condition`, the variables an `=` binds in the top-level `AND`
    /// chain, while `top` holds, going to `bound`.
    fn condition(&mut self, condition: &WhereExpr, top: bool, bound: &mut BTreeSet<String>) {
        match condition {
            WhereExpr::Identified(
                IdentifiedExpr::Compare {
                    lhs: CompareOperand::Path(path),
                    op: CompOp::Eq,
                    rhs: Terminal::Primitive(Primitive::String(value)),
                },
                _,
            ) => {
                if self.compared(path, value) && top {
                    bound.insert(path.root.clone());
                }
            }
            WhereExpr::And(left, right) => {
                self.condition(left, top, bound);
                self.condition(right, top, bound);
            }
            WhereExpr::Or(left, right) => {
                self.condition(left, false, bound);
                self.condition(right, false, bound);
            }
            WhereExpr::Identified(..) | WhereExpr::Not(_) => {}
        }
    }

    /// Reads `path = value`, and returns whether it names an id of the
    /// object `path` is rooted at.
    fn compared(&mut self, path: &IdentifiedPath, value: &str) -> bool {
        match (&path.predicate, &path.path) {
            (None, Some(attribute)) => self.named(attribute, value),
            _ => false,
        }
    }

    /// Records `value` when `path` is the archetype node id or the template
    /// id of the object it is read from, and returns whether it is.
    fn named(&mut self, path: &ObjectPath, value: &str) -> bool {
        if path.parts.iter().any(|part| part.predicate.is_some()) {
            return false;
        }
        let names: Vec<&str> = path.parts.iter().map(|part| part.name.as_str()).collect();
        if names == ARCHETYPE_NODE_ID {
            self.archetypes.insert(value.to_owned());
            true
        } else if names == TEMPLATE_ID {
            self.templates.insert(value.to_owned());
            true
        } else {
            false
        }
    }
}
