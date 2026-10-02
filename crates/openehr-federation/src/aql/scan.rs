// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read-only pass over a bound façade query: where the patient is, and
//! what the rewrite may and may not touch (§5.4, §7.1).

use std::ops::Range;

use openehr_query::ast::{
    AggregateCall, ClassExprOperand, ColumnExpr, CompareOperand, ContainsExpr, FunctionCall,
    IdentifiedExpr, IdentifiedPath, OrderByExpr, Primitive, SelectClause, SelectQuery, Span,
    Terminal, WhereExpr,
};
use openehr_query::lexer::CompOp;
use openehr_query::visit::{
    Visit, walk_aggregate_call, walk_contains_expr, walk_function_call, walk_identified_expr,
    walk_identified_path, walk_order_by_expr, walk_select_expr,
};

use super::fold;
use super::function;
use super::refusal::{Refusal, Unreducible};
use super::subject::{SubjectPath, ehr_id_path, identifier_bearing, patient_path, subject_path};

/// A literal the query compares a subject path with, and where.
#[derive(Debug, Clone)]
pub(super) struct Found {
    /// The ordinal of the top-level `AND` leaf it was written in.
    pub(super) leaf: usize,
    /// The literal.
    pub(super) value: String,
    /// Where the leaf was written.
    pub(super) at: Option<Range<usize>>,
}

/// A selected column that is the resolution input, not node data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Input {
    /// `…/external_ref/id/value`.
    Id,
    /// `…/external_ref/namespace`.
    Namespace,
}

/// What the scan found in a query it did not refuse.
#[derive(Debug, Default)]
pub(super) struct Findings {
    /// The variables the `FROM` clause binds to `EHR`.
    pub(super) ehr: Vec<String>,
    /// The identifier predicates of the top-level `AND` chain.
    pub(super) ids: Vec<Found>,
    /// The namespace predicates of the top-level `AND` chain.
    pub(super) namespaces: Vec<Found>,
    /// The selected columns that are resolution input, by column index.
    pub(super) inputs: Vec<(usize, Input, Option<Range<usize>>)>,
    /// Whether a top-level leaf already scopes the query to an `ehr_id` (N29).
    pub(super) ehr_scoped: bool,
    /// The first aggregate, if the query has one.
    pub(super) aggregate: Option<Hit>,
    /// The first call of a function AQL 1.1.0 does not define, if the query
    /// has one.
    pub(super) undefined: Option<Hit>,
}

/// A place in the query: the byte range it was written at, when known.
#[derive(Debug, Clone)]
pub(super) struct Hit {
    /// The byte range, or `None` for a node with no source position.
    pub(super) at: Option<Range<usize>>,
}

/// Scans `query`, or returns the first refusal it meets.
pub(super) fn scan(query: &SelectQuery) -> Result<Findings, Refusal> {
    let mut ehr = Vec::new();
    ehr_variables(&query.from, &mut ehr);
    let mut scan = Scan {
        findings: Findings {
            ehr,
            ..Findings::default()
        },
        refusal: None,
        context: Context::Select,
        leaves: 0,
        at: None,
    };
    scan.visit_select_query(query);
    match scan.refusal {
        Some(refusal) => Err(refusal),
        None => Ok(scan.findings),
    }
}

/// The variables bound to the `EHR` class anywhere in the containment.
fn ehr_variables(from: &ContainsExpr, out: &mut Vec<String>) {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            if let ClassExprOperand::Class {
                rm_type,
                variable: Some(variable),
                ..
            } = operand
                && rm_type == "EHR"
            {
                out.push(variable.clone());
            }
            if let Some(constraint) = contains {
                ehr_variables(&constraint.expr, out);
            }
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            ehr_variables(left, out);
            ehr_variables(right, out);
        }
    }
}

/// Which clause the scan is in, which decides what a subject path there means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Select,
    From,
    Where,
    OrderBy,
    /// Inside a function call or an aggregate, in any clause.
    Expression,
}

struct Scan {
    findings: Findings,
    refusal: Option<Refusal>,
    context: Context,
    leaves: usize,
    at: Option<Range<usize>>,
}

impl Scan {
    fn refuse(&mut self, refusal: Refusal) {
        if self.refusal.is_none() {
            self.refusal = Some(refusal);
        }
    }

    fn unreducible(&mut self, reason: Unreducible, at: Option<Range<usize>>) {
        self.refuse(Refusal::Unreducible { reason, at });
    }

    /// Walks a `WHERE` tree, `top` while the scan is still in the top-level
    /// `AND` chain.
    fn where_tree(&mut self, node: &WhereExpr, top: bool) {
        match node {
            WhereExpr::Identified(expr, span) => self.leaf(expr, span, top),
            WhereExpr::And(left, right) => {
                self.where_tree(left, top);
                self.where_tree(right, top);
            }
            WhereExpr::Or(left, right) => {
                self.where_tree(left, false);
                self.where_tree(right, false);
            }
            WhereExpr::Not(inner) => self.where_tree(inner, false),
        }
    }

    /// Classifies one `WHERE` condition.
    fn leaf(&mut self, expr: &IdentifiedExpr, span: &Span, top: bool) {
        let at = span.bytes();
        let leaf = self.leaves;
        if top {
            self.leaves = self.leaves.saturating_add(1);
        }
        self.at.clone_from(&at);
        let ehr = self.findings.ehr.clone();
        if let IdentifiedExpr::Compare {
            lhs: CompareOperand::Path(path),
            op,
            rhs,
        } = expr
        {
            if let Some(kind) = patient_path(path, &ehr) {
                self.subject_predicate(kind, *op, rhs, top, leaf, at);
                return;
            }
            if top && *op == CompOp::Eq && ehr_id_path(path, &ehr) {
                self.findings.ehr_scoped = true;
            }
        }
        if let IdentifiedExpr::Compare {
            rhs: Terminal::Path(path),
            ..
        } = expr
            && patient_path(path, &ehr).is_some()
        {
            self.unreducible(Unreducible::NotALiteral, at);
            return;
        }
        if let IdentifiedExpr::Exists(path)
        | IdentifiedExpr::Like { path, .. }
        | IdentifiedExpr::Matches { path, .. } = expr
            && patient_path(path, &ehr).is_some()
        {
            let reason = if top {
                Unreducible::NotEquality
            } else {
                Unreducible::NotConjunctive
            };
            self.unreducible(reason, at);
            return;
        }
        walk_identified_expr(self, expr);
    }

    /// A comparison whose left side is a subject path.
    fn subject_predicate(
        &mut self,
        kind: SubjectPath,
        op: CompOp,
        rhs: &Terminal,
        top: bool,
        leaf: usize,
        at: Option<Range<usize>>,
    ) {
        if !top {
            return self.unreducible(Unreducible::NotConjunctive, at);
        }
        if kind == SubjectPath::Other {
            return self.unreducible(Unreducible::OtherSubjectPath, at);
        }
        if op != CompOp::Eq {
            return self.unreducible(Unreducible::NotEquality, at);
        }
        let value = match rhs {
            Terminal::Primitive(Primitive::String(value)) => value.clone(),
            Terminal::Primitive(_) => return self.refuse(Refusal::IdentifierNotString { at }),
            Terminal::Parameter(_) | Terminal::Path(_) | Terminal::Function(_) => {
                return self.unreducible(Unreducible::NotALiteral, at);
            }
        };
        let found = Found { leaf, value, at };
        match kind {
            SubjectPath::Id => self.findings.ids.push(found),
            SubjectPath::Namespace => self.findings.namespaces.push(found),
            SubjectPath::Other => {}
        }
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_select_query(&mut self, node: &'ast SelectQuery) {
        self.context = Context::Select;
        self.visit_select_clause(&node.select);
        self.context = Context::From;
        self.visit_contains_expr(&node.from);
        self.context = Context::Where;
        if let Some(where_) = &node.where_ {
            self.where_tree(where_, true);
        }
        self.context = Context::OrderBy;
        for term in &node.order_by {
            self.visit_order_by_expr(term);
        }
    }

    fn visit_select_clause(&mut self, node: &'ast SelectClause) {
        let ehr = self.findings.ehr.clone();
        for (index, column) in node.columns.iter().enumerate() {
            if let ColumnExpr::Path(path) = &column.column {
                let input = match subject_path(path, &ehr) {
                    Some(SubjectPath::Id) => Some(Input::Id),
                    Some(SubjectPath::Namespace) => Some(Input::Namespace),
                    Some(SubjectPath::Other) | None => None,
                };
                if let Some(input) = input {
                    self.findings.inputs.push((index, input, path.span.bytes()));
                    continue;
                }
            }
            walk_select_expr(self, column);
        }
    }

    fn visit_contains_expr(&mut self, node: &'ast ContainsExpr) {
        walk_contains_expr(self, node);
    }

    fn visit_order_by_expr(&mut self, node: &'ast OrderByExpr) {
        self.at = node.path.span.bytes();
        walk_order_by_expr(self, node);
    }

    fn visit_function_call(&mut self, node: &'ast FunctionCall) {
        if self.findings.undefined.is_none() && !function::single_row(node) {
            let at = first_path(node).or_else(|| self.at.clone());
            self.findings.undefined = Some(Hit { at });
        }
        let outer = self.context;
        self.context = Context::Expression;
        walk_function_call(self, node);
        self.context = outer;
    }

    fn visit_aggregate_call(&mut self, node: &'ast AggregateCall) {
        let at = match node {
            AggregateCall::Count { path, .. } => path.as_ref().and_then(|p| p.span.bytes()),
            AggregateCall::Stat { path, .. } => path.span.bytes(),
        };
        if self.findings.aggregate.is_none() {
            self.findings.aggregate = Some(Hit { at });
        }
        let outer = self.context;
        self.context = Context::Expression;
        walk_aggregate_call(self, node);
        self.context = outer;
    }

    fn visit_identified_path(&mut self, node: &'ast IdentifiedPath) {
        let ehr = self.findings.ehr.clone();
        let at = node.span.bytes().or_else(|| self.at.clone());
        if patient_path(node, &ehr).is_some() {
            match self.context {
                Context::Select => self.refuse(Refusal::SubjectProjection { at }),
                Context::OrderBy => self.refuse(Refusal::SubjectOrdering { at }),
                Context::Expression => self.unreducible(Unreducible::InsideAnExpression, at),
                Context::From | Context::Where => {
                    self.unreducible(Unreducible::OtherSubjectPath, at);
                }
            }
        }
        walk_identified_path(self, node);
    }
}

/// Where the first path among a call's arguments was written, nested calls
/// included.
pub(super) fn first_path(call: &FunctionCall) -> Option<Range<usize>> {
    let FunctionCall::Named { args, .. } = call else {
        return None;
    };
    args.iter().find_map(|arg| match arg {
        Terminal::Path(path) => path.span.bytes(),
        Terminal::Function(inner) => first_path(inner),
        Terminal::Primitive(_) | Terminal::Parameter(_) => None,
    })
}

/// How the identifier would reach a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeakKind {
    /// A text of the query contains it, or a string function over literals
    /// folds to a text that does (§5.4.1 "in any position").
    Value,
    /// A string function over a literal that the gateway cannot fold is
    /// compared with an identifier-bearing path, so the node could compute
    /// the identifier (§5.4.1 "in any position", §5.4.2).
    Unfoldable,
}

/// Where and how the identifier would reach a node.
#[derive(Debug, Clone)]
pub(super) struct Leak {
    /// How.
    pub(super) kind: LeakKind,
    /// Where.
    pub(super) at: Option<Range<usize>>,
}

/// Every written text of `query` that could carry the identifier to a node:
/// the literals, the codes, names and patterns, the identifiers of the query
/// itself, and the folded value of every string function over literals
/// (§5.4.1 "in any position").
pub(super) fn reaches(query: &SelectQuery, value: &str) -> Option<Leak> {
    let mut search = Search {
        value,
        leak: None,
        at: None,
    };
    search.visit_select_query(query);
    search.leak
}

struct Search<'v> {
    value: &'v str,
    leak: Option<Leak>,
    at: Option<Range<usize>>,
}

impl Search<'_> {
    fn check(&mut self, text: &str) {
        if text.contains(self.value) {
            self.found(LeakKind::Value);
        }
    }

    fn found(&mut self, kind: LeakKind) {
        if self.leak.is_none() {
            self.leak = Some(Leak {
                kind,
                at: self.at.clone(),
            });
        }
    }

    /// Refuses a string function over a literal that does not fold when it is
    /// compared with a path that carries an identifier.
    fn unfoldable(&mut self, path: &IdentifiedPath, call: &FunctionCall) {
        if identifier_bearing(path) && fold::over_a_literal(call) && fold::fold(call).is_none() {
            self.found(LeakKind::Unfoldable);
        }
    }
}

impl<'ast> Visit<'ast> for Search<'_> {
    fn visit_select_expr(&mut self, node: &'ast openehr_query::ast::SelectExpr) {
        if let Some(alias) = &node.alias {
            self.check(alias);
        }
        walk_select_expr(self, node);
    }

    fn visit_top(&mut self, node: &'ast openehr_query::ast::Top) {
        self.check(&node.count.to_string());
    }

    fn visit_class_expr_operand(&mut self, node: &'ast ClassExprOperand) {
        match node {
            ClassExprOperand::Class {
                rm_type, variable, ..
            } => {
                self.check(rm_type);
                if let Some(variable) = variable {
                    self.check(variable);
                }
            }
            ClassExprOperand::Version { variable, .. } => {
                if let Some(variable) = variable {
                    self.check(variable);
                }
            }
        }
        openehr_query::visit::walk_class_expr_operand(self, node);
    }

    fn visit_where_expr(&mut self, node: &'ast WhereExpr) {
        if let WhereExpr::Identified(_, span) = node {
            self.at = span.bytes();
        }
        openehr_query::visit::walk_where_expr(self, node);
    }

    fn visit_limit(&mut self, node: &'ast openehr_query::ast::Limit) {
        self.check(&node.limit.to_string());
        if let Some(offset) = node.offset {
            self.check(&offset.to_string());
        }
    }

    fn visit_identified_path(&mut self, node: &'ast IdentifiedPath) {
        if node.span.bytes().is_some() {
            self.at = node.span.bytes();
        }
        self.check(&node.root);
        walk_identified_path(self, node);
    }

    fn visit_path_part(&mut self, node: &'ast openehr_query::ast::PathPart) {
        self.check(&node.name);
        openehr_query::visit::walk_path_part(self, node);
    }

    fn visit_archetype_predicate(&mut self, node: &'ast openehr_query::ast::ArchetypePredicate) {
        match node {
            openehr_query::ast::ArchetypePredicate::Hrid(text)
            | openehr_query::ast::ArchetypePredicate::Parameter(text) => self.check(text),
        }
    }

    fn visit_node_predicate(&mut self, node: &'ast openehr_query::ast::NodePredicate) {
        match node {
            openehr_query::ast::NodePredicate::Code { code: text, .. }
            | openehr_query::ast::NodePredicate::Archetype { hrid: text, .. }
            | openehr_query::ast::NodePredicate::Parameter(text)
            | openehr_query::ast::NodePredicate::MatchesRegex { regex: text, .. } => {
                self.check(text);
            }
            openehr_query::ast::NodePredicate::Standard(_)
            | openehr_query::ast::NodePredicate::And(..)
            | openehr_query::ast::NodePredicate::Or(..) => {}
        }
        openehr_query::visit::walk_node_predicate(self, node);
    }

    fn visit_node_name_constraint(&mut self, node: &'ast openehr_query::ast::NodeNameConstraint) {
        match node {
            openehr_query::ast::NodeNameConstraint::String(text)
            | openehr_query::ast::NodeNameConstraint::Parameter(text)
            | openehr_query::ast::NodeNameConstraint::TermCode(text)
            | openehr_query::ast::NodeNameConstraint::Code(text) => self.check(text),
        }
    }

    fn visit_path_predicate_operand(
        &mut self,
        node: &'ast openehr_query::ast::PathPredicateOperand,
    ) {
        match node {
            openehr_query::ast::PathPredicateOperand::Parameter(text)
            | openehr_query::ast::PathPredicateOperand::Code(text) => self.check(text),
            openehr_query::ast::PathPredicateOperand::Primitive(_)
            | openehr_query::ast::PathPredicateOperand::Path(_) => {}
        }
        openehr_query::visit::walk_path_predicate_operand(self, node);
    }

    fn visit_terminal(&mut self, node: &'ast Terminal) {
        if let Terminal::Parameter(text) = node {
            self.check(text);
        }
        openehr_query::visit::walk_terminal(self, node);
    }

    fn visit_like_operand(&mut self, node: &'ast openehr_query::ast::LikeOperand) {
        match node {
            openehr_query::ast::LikeOperand::String(text)
            | openehr_query::ast::LikeOperand::Parameter(text) => self.check(text),
        }
    }

    fn visit_matches_operand(&mut self, node: &'ast openehr_query::ast::MatchesOperand) {
        if let openehr_query::ast::MatchesOperand::Uri(uri) = node {
            self.check(uri);
        }
        openehr_query::visit::walk_matches_operand(self, node);
    }

    fn visit_value_list_item(&mut self, node: &'ast openehr_query::ast::ValueListItem) {
        if let openehr_query::ast::ValueListItem::Parameter(text) = node {
            self.check(text);
        }
        openehr_query::visit::walk_value_list_item(self, node);
    }

    fn visit_function_call(&mut self, node: &'ast FunctionCall) {
        if let FunctionCall::Named { name, .. } = node {
            self.check(name);
        }
        if let Some(folded) = fold::fold(node) {
            self.check(&folded);
        }
        walk_function_call(self, node);
    }

    fn visit_identified_expr(&mut self, node: &'ast IdentifiedExpr) {
        match node {
            IdentifiedExpr::Compare {
                lhs: CompareOperand::Path(path),
                rhs: Terminal::Function(call),
                ..
            }
            | IdentifiedExpr::Compare {
                lhs: CompareOperand::Function(call),
                rhs: Terminal::Path(path),
                ..
            } => self.unfoldable(path, call),
            IdentifiedExpr::Compare { .. }
            | IdentifiedExpr::Exists(_)
            | IdentifiedExpr::Like { .. }
            | IdentifiedExpr::Matches { .. }
            | IdentifiedExpr::Resolved(_) => {}
        }
        walk_identified_expr(self, node);
    }

    fn visit_terminology_function(&mut self, node: &'ast openehr_query::ast::TerminologyFunction) {
        self.check(&node.operation);
        self.check(&node.arg2);
        self.check(&node.arg3);
    }

    fn visit_primitive(&mut self, node: &'ast Primitive) {
        match node {
            Primitive::String(text) => self.check(text),
            Primitive::Integer(number) => self.check(&number.to_string()),
            Primitive::Real(number) => self.check(&number.to_string()),
            Primitive::Boolean(_) | Primitive::Null => {}
        }
    }
}
