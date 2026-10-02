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
use openehr_query::ast::{ClassExprOperand, ColumnExpr, ContainsExpr, IdentifiedPath, SelectQuery};
use openehr_query::bind::Parameters;
use openehr_query::federation::{Directive, Federated, parse_federated};
use openehr_query::visit::{Visit, walk_identified_path};

use super::refusal::Refusal;
use super::{Analysis, ColumnSource, Context, Paging, rewrite};

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

    /// Analyses the query and prepares the node queries, as [`super::analyse`]
    /// does.
    ///
    /// The columns selected through the directive's variable stay in the
    /// façade's `columns[]` as [`ColumnSource::Endpoint`] and are asked of no
    /// node (§9.3).
    ///
    /// # Errors
    /// The refusals of [`super::analyse`], and [`Refusal::EndpointVariable`] when
    /// the directive's variable is used other than as a selected column.
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
    /// Puts the ENDPOINT attribute columns back at their façade `positions`:
    /// `columns[]` becomes the façade's own, and each of those columns is
    /// [`ColumnSource::Endpoint`] (§9.3, N17).
    pub(super) fn select_endpoint_attributes(
        &mut self,
        facade: Vec<ResultSetColumn>,
        positions: &[usize],
    ) {
        let (columns, sources) = match self {
            Self::Patient(query) => (&mut query.columns, &mut query.sources),
            Self::Unscoped(query) => (&mut query.columns, &mut query.node.columns),
        };
        let mut node = std::mem::take(sources).into_iter();
        *sources = (0..facade.len())
            .filter_map(|index| {
                if positions.contains(&index) {
                    Some(ColumnSource::Endpoint)
                } else {
                    node.next()
                }
            })
            .collect();
        *columns = facade;
    }
}

/// Removes the columns `query` selects through the directive's variable, and
/// returns their façade positions in order.
///
/// # Errors
/// [`Refusal::EndpointVariable`] when the `FROM` clause also binds the
/// variable, or a path through it appears anywhere but as a selected column.
pub(super) fn strip_endpoint_columns(
    query: &mut SelectQuery,
    directive: &Directive,
) -> Result<Vec<usize>, Refusal> {
    let Some(variable) = directive.variable.as_deref() else {
        return Ok(Vec::new());
    };
    if binds(&query.from, variable) {
        return Err(Refusal::EndpointVariable {
            at: directive.span.bytes(),
        });
    }
    let positions: Vec<usize> = query
        .select
        .columns
        .iter()
        .enumerate()
        .filter_map(|(index, column)| match &column.column {
            ColumnExpr::Path(path) if path.root == variable => Some(index),
            ColumnExpr::Path(_)
            | ColumnExpr::Primitive(_)
            | ColumnExpr::Aggregate(_)
            | ColumnExpr::Function(_) => None,
        })
        .collect();
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
    Ok(positions)
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
