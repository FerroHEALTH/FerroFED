// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL rewrite of §7.1 (feature `aql`): the reference implementation's
//! golden cases adjudicated against the specification, FerroFED's own strict
//! corpus where no specification governs a case, and the identifier-hygiene property of
//! §5.4.1 and N33.
#![cfg(feature = "aql")]

mod aggregate;
mod attribute;
mod dedup;
mod directive;
mod distinct;
mod entry;
mod folding;
mod function;
mod golden;
mod guard;
mod hygiene;
mod offset;
mod order;
mod rules;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Context, Paging, PatientQuery, Targeting, analyse};
use openehr_query::bind::Parameters;

/// The synthetic issuing namespace the fixtures resolve in, under the example
/// OID arc.
pub(crate) const NAMESPACE: &str = "urn:oid:2.999.1";

/// A resolved `ehr_id`, synthetic.
pub(crate) const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// An ask-all deployment that declares the fixture namespace as its default.
pub(crate) fn ask_all() -> Context {
    Context::new(Targeting::AskAll).with_default_namespace(NAMESPACE)
}

/// Analyses `aql` with no parameters and no paging members.
pub(crate) fn analysed(aql: &str, context: &Context) -> Result<Analysis, Refusal> {
    analyse(aql, &Parameters::new(), Paging::default(), context)
}

/// The patient analysis of `aql`, which must name a patient.
pub(crate) fn patient(aql: &str) -> PatientQuery {
    match analysed(aql, &ask_all()) {
        Ok(Analysis::Patient(query)) => query,
        other => panic!("expected a patient query, got {other:?}"),
    }
}

/// The node query `aql` produces for [`EHR_ID`].
pub(crate) fn node_aql(aql: &str) -> String {
    patient(aql)
        .for_node(&HierObjectId::new(EHR_ID).expect("the fixture ehr_id is a HIER_OBJECT_ID"))
        .aql()
        .to_owned()
}

/// The refusal `aql` draws in an ask-all deployment with the fixture default
/// namespace.
pub(crate) fn refused(aql: &str) -> Refusal {
    match analysed(aql, &ask_all()) {
        Err(refusal) => refusal,
        Ok(analysis) => panic!("expected a refusal, got {analysis:?}"),
    }
}

/// Asserts that two AQL texts parse to the same tree; spacing and
/// parenthesisation are the printer's.
pub(crate) fn assert_same_aql(actual: &str, expected: &str) {
    let actual_tree = openehr_query::parser::parse_str(actual).expect("the node query is AQL");
    let expected_tree =
        openehr_query::parser::parse_str(expected).expect("the expected query is AQL");
    assert_eq!(
        actual_tree, expected_tree,
        "node query {actual}\nexpected {expected}"
    );
}
