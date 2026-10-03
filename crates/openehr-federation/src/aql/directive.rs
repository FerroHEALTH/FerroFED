// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `FROM ENDPOINT` and `ORGANISATION` directive of §8.1 (N11): the
//! columns selected through its variable, which no node is asked for.
//!
//! `openehr-query`'s federation parse lifts the directive out of the query
//! before the strict AQL parse, so the directive itself can never reach a
//! node query. Its variable stays in the remaining query wherever the client
//! wrote it, and this pass takes it out: a selected path through the variable
//! is an ENDPOINT attribute the Tier adds to the rows (§9.3, N12), and the
//! variable anywhere else is refused.

use std::ops::Range;

use openehr_its::rest::generated::query::ResultSetColumn;
use openehr_query::ast::{
    ClassExprOperand, ColumnExpr, ContainsExpr, IdentifiedPath, SelectClause, SelectQuery,
};
use openehr_query::bind::Parameters;
use openehr_query::federation::{Directive, Federated, parse_federated};
use openehr_query::visit::{Visit, walk_identified_path};

use super::refusal::Refusal;
use super::{Analysis, ColumnSource, Context, Paging, rewrite};
use crate::attribute::EndpointAttribute;

/// A façade query as parsed: the `FROM ENDPOINT` or `ORGANISATION`
/// directive when it carries one (§8.1), and the strict AQL that remains.
///
/// The directive is lifted out by `openehr-query`'s federation parse, so no
/// node query printed from the remainder can carry it (N7). The caller reads
/// [`FacadeQuery::directive`], selects the node set it names, and analyses
/// the query under [`super::Targeting::Directed`] at that set.
#[derive(Debug, Clone)]
pub struct FacadeQuery {
    federated: Federated,
}

impl FacadeQuery {
    /// Parses a façade query, with or without the directive.
    ///
    /// # Errors
    /// [`Refusal::NotAql`] when the query is not AQL 1.1.0, or carries a
    /// malformed directive: an empty or unterminated list, an identifier that
    /// is not a string, or no `CONTAINS` after it.
    pub fn parse(aql: &str) -> Result<Self, Refusal> {
        parse_federated(aql)
            .map(|federated| Self { federated })
            .map_err(|error| Refusal::NotAql {
                at: super::first_fault(&error),
            })
    }

    /// The directive, or `None` for an undirected query (§8.1).
    #[must_use]
    pub fn directive(&self) -> Option<&Directive> {
        self.federated.directive.as_ref()
    }

    /// The parse: the directive and the strict AQL that remains.
    pub(super) fn federated(&self) -> &Federated {
        &self.federated
    }

    /// Analyses the query and prepares the node queries, as [`super::analyse`]
    /// does.
    ///
    /// The columns selected through the directive's variable stay in the
    /// façade's `columns[]` as [`ColumnSource::Endpoint`] and are asked of no
    /// node (§9.3).
    ///
    /// # Errors
    /// The refusals of [`super::analyse`]; [`Refusal::EndpointVariable`] when
    /// the directive's variable is used other than as a selected column;
    /// [`Refusal::EndpointAttributeUnknown`] when a selected path through it
    /// is no §9.3 attribute; and [`Refusal::EndpointNameCollision`] when an
    /// ENDPOINT attribute column is named as an EHR-derived column is (N18).
    pub fn analyse(
        self,
        parameters: &Parameters,
        paging: Paging,
        context: &Context,
    ) -> Result<Analysis, Refusal> {
        let Federated { directive, query } = self.federated;
        super::analyse_tree(query, directive.as_ref(), parameters, paging, context)
    }
}

impl Analysis {
    /// The ENDPOINT attributes the query selects, each once, in the order of
    /// their first column (§9.3, N12): the values the gateway takes from the
    /// registry for every endpoint it asks. Empty when the query selects
    /// none, and the rows then carry no endpoint column (N17).
    #[must_use]
    pub fn attributes(&self) -> Vec<EndpointAttribute> {
        let mut attributes = Vec::new();
        for source in self.sources() {
            if let ColumnSource::Endpoint(attribute) = source
                && !attributes.contains(attribute)
            {
                attributes.push(*attribute);
            }
        }
        attributes
    }

    /// Puts the ENDPOINT attribute columns back at their façade positions:
    /// `columns[]` becomes the façade's own, and each of those columns is the
    /// [`ColumnSource::Endpoint`] of its attribute (§9.3, N17).
    pub(super) fn select_endpoint_attributes(
        &mut self,
        facade: Vec<ResultSetColumn>,
        selected: &[Selected],
    ) {
        let (columns, sources) = match self {
            Self::Patient(query) => (&mut query.columns, &mut query.sources),
            Self::Unscoped(query) => (&mut query.columns, &mut query.node.columns),
        };
        let mut node = std::mem::take(sources).into_iter();
        *sources = (0..facade.len())
            .filter_map(
                |index| match selected.iter().find(|column| column.position == index) {
                    Some(column) => Some(ColumnSource::Endpoint(column.attribute)),
                    None => node.next(),
                },
            )
            .collect();
        *columns = facade;
    }
}

/// A column the façade query selects through the directive's variable.
#[derive(Debug, Clone)]
pub(super) struct Selected {
    /// Its position among the façade's columns.
    pub(super) position: usize,
    /// The ENDPOINT attribute it selects (§9.3).
    pub(super) attribute: EndpointAttribute,
    /// Where it was written.
    pub(super) at: Option<Range<usize>>,
}

/// Removes the columns `query` selects through the directive's variable, and
/// returns them in façade order.
///
/// # Errors
/// [`Refusal::EndpointVariable`] when the `FROM` clause also binds the
/// variable, or a path through it appears anywhere but as a selected column;
/// [`Refusal::EndpointAttributeUnknown`] for a selected path that is no
/// §9.3 attribute; and [`Refusal::EndpointNameCollision`] for an attribute
/// column named as an EHR-derived column is (N18, CP-35).
pub(super) fn strip_endpoint_columns(
    query: &mut SelectQuery,
    directive: &Directive,
) -> Result<Vec<Selected>, Refusal> {
    let Some(variable) = directive.variable.as_deref() else {
        return Ok(Vec::new());
    };
    if binds(&query.from, variable) {
        return Err(Refusal::EndpointVariable {
            at: directive.span.bytes(),
        });
    }
    let mut selected = Vec::new();
    for (position, column) in query.select.columns.iter().enumerate() {
        if let ColumnExpr::Path(path) = &column.column
            && path.root == variable
        {
            let at = path.span.bytes();
            let attribute =
                attribute(path).ok_or(Refusal::EndpointAttributeUnknown { at: at.clone() })?;
            selected.push(Selected {
                position,
                attribute,
                at,
            });
        }
    }
    collision(&query.select, &selected)?;
    let positions: Vec<usize> = selected.iter().map(|column| column.position).collect();
    rewrite::strip_columns(query, &positions);
    let mut search = Through {
        variable,
        at: None,
        found: false,
    };
    search.visit_select_query(query);
    if search.found {
        return Err(Refusal::EndpointVariable { at: search.at });
    }
    Ok(selected)
}

/// The attribute `path` selects: one attribute name after the variable, with
/// no predicate on either (§9.3).
fn attribute(path: &IdentifiedPath) -> Option<EndpointAttribute> {
    if path.predicate.is_some() {
        return None;
    }
    match path.path.as_ref()?.parts.as_slice() {
        [part] if part.predicate.is_none() => EndpointAttribute::from_name(&part.name),
        _ => None,
    }
}

/// Refuses an ENDPOINT attribute column that carries the name of an
/// EHR-derived column (N18, CP-35).
///
/// A column is named by its alias, or `#<index>` without one, so only two
/// aliases can collide, and an alias resolves the collision as N18 requires
/// unless the client gives the same one to both columns.
fn collision(select: &SelectClause, selected: &[Selected]) -> Result<(), Refusal> {
    let names = super::render_columns(select);
    let endpoint = |index: usize| selected.iter().any(|column| column.position == index);
    for column in selected {
        let Some(name) = names.get(column.position).map(|rendered| &rendered.name) else {
            continue;
        };
        let shadowed = names
            .iter()
            .enumerate()
            .any(|(index, other)| !endpoint(index) && other.name == *name);
        if shadowed {
            return Err(Refusal::EndpointNameCollision {
                at: column.at.clone(),
            });
        }
    }
    Ok(())
}

/// Whether the containment binds `variable` to a class or a version.
fn binds(from: &ContainsExpr, variable: &str) -> bool {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            let bound = match operand {
                ClassExprOperand::Class {
                    variable: Some(name),
                    ..
                }
                | ClassExprOperand::Version {
                    variable: Some(name),
                    ..
                } => name == variable,
                ClassExprOperand::Class { variable: None, .. }
                | ClassExprOperand::Version { variable: None, .. } => false,
            };
            bound
                || contains
                    .as_ref()
                    .is_some_and(|constraint| binds(&constraint.expr, variable))
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            binds(left, variable) || binds(right, variable)
        }
    }
}

/// The first path through the directive's variable left in a query.
struct Through<'v> {
    variable: &'v str,
    at: Option<Range<usize>>,
    found: bool,
}

impl<'ast> Visit<'ast> for Through<'_> {
    fn visit_identified_path(&mut self, node: &'ast IdentifiedPath) {
        if !self.found && node.root == self.variable {
            self.found = true;
            self.at = node.span.bytes();
        }
        walk_identified_path(self, node);
    }
}
