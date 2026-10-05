// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stored-query definition, admitted to a federated stored-query registry
//! (§12.7, N44).
//!
//! A registry expands a stored definition into the ordinary pipeline of §7
//! when it is invoked by name, "exactly as if the client had submitted the
//! text inline" (§12.7). [`Definition::admit`] therefore runs the analysis
//! of a façade query over the text once, at storage time, with every
//! `$parameter` standing in for a value the invocation will bind: a text the
//! rewrite refuses whatever is bound to it is refused before it is held.
//!
//! The analysis also reports where the patient comes from. A definition
//! whose patient predicate compares with a `$parameter` holds no patient
//! identifier; one that compares with a literal holds the identifier itself,
//! and [`Definition::subject`] says so with the byte range it was written at,
//! never the value (§5.4.3). The held text is the canonical print of the
//! parsed query, so nothing the parser drops, a comment included, is held.
//!
//! # Examples
//!
//! ```
//! use openehr_federation::aql::definition::{Definition, SubjectOrigin};
//! use openehr_federation::aql::{Context, Targeting};
//!
//! let context = Context::new(Targeting::AskAll).with_default_namespace("urn:oid:2.999.1");
//! let stored = Definition::admit(
//!     "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
//!      WHERE e/ehr_status/subject/external_ref/id/value = $patient -- a comment",
//!     &context,
//! )?;
//! assert_eq!(Some(SubjectOrigin::Parameter), stored.subject());
//! assert!(!stored.aql().contains("comment"));
//! # Ok::<(), openehr_federation::aql::refusal::Refusal>(())
//! ```

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::ops::Range;

use openehr_query::ast::{Primitive, SelectQuery};
use openehr_query::bind::{FaultKind, Parameters, bind};
use openehr_query::federation::to_federated_aql;

use super::directive::FacadeQuery;
use super::refusal::Refusal;
use super::{Analysis, Context, Paging, Targeting};

/// Where a definition's patient identifier comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubjectOrigin {
    /// A `$parameter`, bound by each invocation's `query_parameters`.
    Parameter,
    /// A literal written in the definition's text.
    Literal {
        /// Where the patient predicate was written, when the parser gave a
        /// position.
        at: Option<Range<usize>>,
    },
}

/// A stored-query definition that analyses as a façade query (§12.7, §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    aql: String,
    subject: Option<SubjectOrigin>,
}

impl Definition {
    /// Parses and analyses `aql` as a façade query under `context`, with
    /// every `$parameter` standing in for the value an invocation binds.
    ///
    /// The analysis is the one [`super::analyse`] runs, under the targeting
    /// that refuses least: one directed endpoint. Whether an aggregate or a
    /// function outside AQL reaches more than one node depends on the
    /// invocation's targeting (§8.4, §11.6.3), so only that invocation can
    /// refuse it.
    ///
    /// # Errors
    /// The [`Refusal`] of [`super::analyse`] that holds whatever values the
    /// parameters are bound to: a text that is not AQL, a patient predicate
    /// that cannot be reduced to one `ehr_id` scope per node, a patient
    /// identifier that would survive into a node query, a missing namespace
    /// when the deployment declares no default, and the like.
    pub fn admit(aql: &str, context: &Context) -> Result<Self, Refusal> {
        let facade = FacadeQuery::parse(aql)?;
        let held = to_federated_aql(facade.federated());
        let names = parameters(&facade.federated().query);
        let context = context.clone().with_targeting(Targeting::Directed {
            endpoints: NonZeroUsize::MIN,
        });
        // NOTE: no specification governs this: our own design; two disjoint
        // placeholder sets tell a parameter from a literal that equals one.
        let mut origins = [None, None];
        for (slot, set) in origins.iter_mut().zip(unwritten(aql)) {
            let (bound, placeholders) = placeholders(&names, set);
            let analysis = facade
                .clone()
                .analyse(&bound, Paging::default(), &context)?;
            *slot = origin(&analysis, &placeholders);
        }
        let subject = match origins {
            [
                Some(SubjectOrigin::Parameter),
                Some(SubjectOrigin::Parameter),
            ] => Some(SubjectOrigin::Parameter),
            [Some(SubjectOrigin::Literal { at }), _] | [_, Some(SubjectOrigin::Literal { at })] => {
                Some(SubjectOrigin::Literal { at })
            }
            [None, _] | [_, None] => None,
        };
        Ok(Self { aql: held, subject })
    }

    /// The text the registry holds: the canonical print of the parsed query,
    /// its directive included (§8.1).
    #[must_use]
    pub fn aql(&self) -> &str {
        &self.aql
    }

    /// Where the patient comes from, or `None` for a definition that names
    /// no patient.
    #[must_use]
    pub fn subject(&self) -> Option<SubjectOrigin> {
        self.subject.clone()
    }
}

/// The names of the parameters `query` uses, each once, in order of first
/// use.
fn parameters(query: &SelectQuery) -> Vec<String> {
    let mut probe = query.clone();
    match bind(&mut probe, &Parameters::new()) {
        Ok(()) => Vec::new(),
        Err(error) => error
            .faults
            .into_iter()
            .filter(|fault| fault.kind == FaultKind::Unbound)
            .map(|fault| fault.name)
            .collect(),
    }
}

/// The stem every placeholder value carries, before its set and its index.
const PLACEHOLDER: &str = "ferrofed_parameter_";

/// The first sets whose placeholders `aql` does not spell anywhere, so no
/// literal of the text equals one and a placeholder is never mistaken for a
/// value the text holds.
fn unwritten(aql: &str) -> impl Iterator<Item = u64> + use<'_> {
    (0_u64..).filter(move |set| !aql.contains(&format!("{PLACEHOLDER}{set}_")))
}

/// One placeholder value for every name in `names`, from the set `set`, and
/// the values themselves.
///
/// Each value is one archetype identifier, a string that binds at every
/// position the grammar admits a parameter (`openehr_query::bind::Position`).
fn placeholders(names: &[String], set: u64) -> (Parameters, BTreeSet<String>) {
    let mut bound = Parameters::new();
    let mut values = BTreeSet::new();
    for (index, name) in names.iter().enumerate() {
        let value = format!("openEHR-EHR-CLUSTER.{PLACEHOLDER}{set}_{index}.v1");
        bound.insert(name, Primitive::String(value.clone()));
        values.insert(value);
    }
    (bound, values)
}

/// Where the patient of `analysis` comes from, given the `placeholders` its
/// parameters were bound to.
fn origin(analysis: &Analysis, placeholders: &BTreeSet<String>) -> Option<SubjectOrigin> {
    let Analysis::Patient(query) = analysis else {
        return None;
    };
    if placeholders.contains(query.subject().value()) {
        return Some(SubjectOrigin::Parameter);
    }
    let at = query.stripped().iter().flatten().next().cloned();
    Some(SubjectOrigin::Literal { at })
}
