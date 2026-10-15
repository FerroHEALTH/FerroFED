// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The analysis of a parsed façade query: the directive's ENDPOINT
//! attributes taken out, the paging applied, and the node query of a patient
//! query or of an unscoped one built with the Tier order it is merged under
//! (§7.1, §8.1, §11.6).

use openehr_its::rest::generated::query::ResultSetColumn;
use openehr_query::ast::{ColumnExpr, SelectQuery};
use openehr_query::bind::{Parameters, bind};
use openehr_query::federation::Directive;
use openehr_query::printer::to_aql;

use super::archetypes::Constrained;
use super::directive::{self, Stripped};
use super::refusal::{Indecomposable, Refusal};
use super::scan::{self, Findings, Input};
use super::subject::{self, Subject};
use super::{
    Analysis, ColumnSource, Context, NodeQuery, Paging, PatientQuery, Targeting, UnscopedQuery,
    aggregate, paging, render_columns, rewrite, scope,
};

/// Analyses the parsed façade `query`, whose `directive` the federation parse
/// lifted out (§7.1, §8.1).
pub(super) fn analyse_tree(
    mut query: SelectQuery,
    directive: Option<&Directive>,
    parameters: &Parameters,
    paging: Paging,
    context: &Context,
) -> Result<Analysis, Refusal> {
    bind(&mut query, parameters).map_err(Refusal::Parameters)?;
    scope::canonical(&mut query);
    let constrained = Constrained::of(&query);
    let facade_columns = render_columns(&query.select);
    let Stripped {
        selected: endpoint,
        ordered: by_endpoint,
    } = match directive {
        Some(directive) => directive::strip_endpoint(&mut query, directive)?,
        None => Stripped::default(),
    };
    let findings = scan::scan(&query)?;
    // NOTE: §11.6.3 [[aggregate-block]] forbids per-node aggregate rows, and nothing tells the
    // gateway whether a function outside AQL aggregates, so a fan-out refuses it (N14).
    if let Some(found) = &findings.undefined
        && !context.targeting.single_endpoint()
    {
        return Err(Refusal::UndefinedFunction {
            at: found.at.clone(),
        });
    }
    let skip = paging::page(&mut query, paging, context.offset, !by_endpoint.is_empty())?;
    let columns = render_columns(&query.select);
    let recombination = match &findings.aggregate {
        Some(found) if !context.targeting.single_endpoint() => {
            Some(aggregate::decompose(&mut query, context, found.at.clone())?)
        }
        Some(_) | None => None,
    };
    // NOTE: §11.6.3, an ENDPOINT attribute beside an aggregate groups the rows by endpoint, a
    // dimension that spans nodes, so the one recombined row has no endpoint to name.
    if let (Some(_), Some(first)) = (&recombination, endpoint.first()) {
        return Err(Refusal::Indecomposable {
            reason: Indecomposable::PlainColumn,
            at: first.at.clone(),
        });
    }
    let plain = findings.aggregate.is_none();
    let ordered = rewrite::Rows::of(plain, context.dedup, !by_endpoint.is_empty());
    let distinct = query.select.distinct;
    let mut analysis = match subject::named(&findings, context)? {
        Some((subject, consumed)) => {
            patient(query, &findings, subject, &consumed, columns, ordered)?
        }
        None => unscoped(query, &findings, context, columns, ordered)?,
    };
    if !endpoint.is_empty() {
        analysis.select_endpoint_attributes(facade_columns, &endpoint);
    }
    analysis.constrain(constrained);
    let (order, recombined, sources) = match &mut analysis {
        Analysis::Patient(query) => (
            &mut query.order,
            &mut query.recombination,
            query.sources.as_slice(),
        ),
        Analysis::Unscoped(query) => (
            &mut query.order,
            &mut query.recombination,
            query.node.columns.as_slice(),
        ),
    };
    let mut shaped = std::mem::take(order).with_offset(skip);
    if distinct {
        shaped = shaped.with_distinct(visible(sources));
    }
    *order = shaped;
    *recombined = recombination;
    analysis.order_by_endpoint(&by_endpoint, plain)?;
    Ok(analysis)
}

/// The node columns the client sees, in façade order: the columns a row is
/// distinct on (N13). The subject columns are one constant for the whole
/// answer, and a column the rewrite adds is never seen, so neither is one.
/// An ENDPOINT attribute is no node column: the merge holds the attributes
/// beside each endpoint's rows, and they join the tuple there.
fn visible(sources: &[ColumnSource]) -> Vec<usize> {
    sources
        .iter()
        .filter_map(|source| match source {
            ColumnSource::Node(column) => Some(*column),
            ColumnSource::Subject | ColumnSource::Namespace | ColumnSource::Endpoint(_) => None,
        })
        .collect()
}

fn patient(
    query: SelectQuery,
    findings: &Findings,
    subject: Subject,
    consumed: &[usize],
    columns: Vec<ResultSetColumn>,
    ordered: rewrite::Rows,
) -> Result<Analysis, Refusal> {
    let inputs: Vec<usize> = findings.inputs.iter().map(|(index, _, _)| *index).collect();
    let stripped = findings
        .ids
        .iter()
        .chain(&findings.namespaces)
        .map(|found| found.at.clone())
        .collect();
    let mut dispatched = query.clone();
    rewrite::strip_where(&mut dispatched, consumed, None);
    rewrite::strip_columns(&mut dispatched, &inputs);
    rewrite::order_for(&mut dispatched, ordered, true)?;
    if let Some(leak) = scan::reaches(&dispatched, subject.value()) {
        return Err(match leak.kind {
            scan::LeakKind::Value => Refusal::IdentifierElsewhere { at: leak.at },
            scan::LeakKind::Unfoldable => Refusal::UnfoldableFunction { at: leak.at },
        });
    }
    let mut template = query;
    let ehr = rewrite::ehr_variable(&mut template, &findings.ehr);
    let within_one_ehr = scope::within_one_ehr(&template.from, &ehr);
    rewrite::strip_where(&mut template, consumed, Some(&ehr));
    rewrite::strip_columns(&mut template, &inputs);
    rewrite::keep_a_column(&mut template, &ehr);
    let order = rewrite::order_for(&mut template, ordered, true)?;
    let mut node = 0_usize;
    let sources = (0..columns.len())
        .map(|index| {
            match findings
                .inputs
                .iter()
                .find(|(column, _, _)| *column == index)
            {
                Some((_, Input::Id, _)) => ColumnSource::Subject,
                Some((_, Input::Namespace, _)) => ColumnSource::Namespace,
                None => {
                    let source = ColumnSource::Node(node);
                    node = node.saturating_add(1);
                    source
                }
            }
        })
        .collect();
    Ok(Analysis::Patient(PatientQuery {
        subject,
        template: Box::new(template),
        columns,
        sources,
        stripped,
        order,
        recombination: None,
        within_one_ehr,
        constrained: Constrained::default(),
        sorted_by: Vec::new(),
    }))
}

fn unscoped(
    mut query: SelectQuery,
    findings: &Findings,
    context: &Context,
    columns: Vec<ResultSetColumn>,
    ordered: rewrite::Rows,
) -> Result<Analysis, Refusal> {
    // NOTE: §12.5.1, N29: one ehr_id names its owner, so that query's node set is defined.
    let ehr_scope = scope::of(findings);
    if context.targeting == Targeting::Localized && ehr_scope.is_none() {
        return Err(Refusal::NodeSetUndefined);
    }
    let within_one_ehr = ehr_scope.is_some()
        && matches!(findings.ehr.as_slice(), [only] if scope::within_one_ehr(&query.from, only));
    let ehr_columns = query
        .select
        .columns
        .iter()
        .enumerate()
        .filter(|(_, column)| {
            matches!(&column.column, ColumnExpr::Path(path) if subject::projects_ehr_id(path, &findings.ehr))
        })
        .map(|(index, _)| index)
        .collect();
    if query.select.columns.is_empty() {
        // NOTE: no specification governs a query that selects only ENDPOINT attributes (§9.3):
        // our own design, each node answers one EHR column per row it holds.
        let ehr = rewrite::ehr_variable(&mut query, &findings.ehr);
        rewrite::keep_a_column(&mut query, &ehr);
    }
    let order = rewrite::order_for(&mut query, ordered, false)?;
    let sources = (0..columns.len()).map(ColumnSource::Node).collect();
    Ok(Analysis::Unscoped(UnscopedQuery {
        node: NodeQuery {
            aql: to_aql(&query),
            columns: sources,
            limit: order.limit(),
        },
        columns,
        ehr_scoped: findings.ehr_scoped,
        ehr_scope,
        order,
        recombination: None,
        within_one_ehr,
        ehr_columns,
        constrained: Constrained::default(),
        sorted_by: Vec::new(),
    }))
}
