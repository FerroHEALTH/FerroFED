// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored query of the laboratory report.
//!
//! A laboratory report is one document per report (Regulation (EU)
//! 2025/327 Art 14(1)(e)): the HL7 Europe Laboratory Report's
//! `Composition-eu-lab` heads it and its `DiagnosticReport-eu-lab` carries
//! the body. The openEHR side is one composition per report, so the
//! category has one stored query, held read-only under the reserved
//! namespace at [`VERSION`] (§12.7, N44), in the shape of
//! [`section_query`]: every whole composition that contains one of
//! [`ARCHETYPES`], with its uid and its template id.
//!
//! The archetypes are the entries the openEHR international Clinical
//! Knowledge Manager's laboratory test result template names (CKM cid
//! `1013.26.408` "Generic lab test result example simple", asset version 3,
//! DRAFT, vendored under `docs/specs/openehr-ckm-laboratory/`). The
//! template is rooted at `openEHR-EHR-COMPOSITION.report-result.v1`; the
//! query names its entry alone and no composition archetype, so a CDR that
//! records the result under another report composition still answers.

use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use openehr_query::ast::SelectQuery;
use openehr_query::printer;

use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};

/// The query's name in the reserved namespace.
pub const SLUG: &str = "laboratory-report";

/// The Xt-EHR element path of the report body the query feeds, the row of
/// the laboratory report crosswalk.
pub const ELEMENT: &str = "EHDSLaboratoryReport.body";

/// The entry archetypes a laboratory report composition contains, at least
/// one of them, as the template names them.
pub const ARCHETYPES: &[Archetype] = &[Archetype {
    class: "OBSERVATION",
    id: "openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
}];

/// Returns the query's qualified name,
/// `eu.ferrofed.eehrxf::laboratory-report`.
///
/// # Errors
///
/// The [`QueryNameError`] of [`QueryName::new`], which the name does not
/// meet.
pub fn name() -> Result<QueryName, QueryNameError> {
    QueryName::new(&format!("{NAMESPACE}::{SLUG}"))
}

/// Returns the query as an AQL syntax tree.
///
/// # Panics
///
/// Never: [`ARCHETYPES`] names an archetype.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "ARCHETYPES names one archetype, which a test holds"
)]
pub fn query() -> SelectQuery {
    section_query::query(ARCHETYPES).expect("the laboratory report should name an archetype")
}

/// Returns the query as AQL text, printed from its syntax tree.
#[must_use]
pub fn aql() -> String {
    printer::to_aql(&query())
}

/// Returns the query as the registry holds it: under its reserved name, at
/// [`VERSION`], saved at [`SAVED`].
///
/// # Errors
///
/// The [`QueryNameError`] of [`name`].
pub fn definition() -> Result<StoredDefinition, QueryNameError> {
    Ok(StoredDefinition::new(name()?, VERSION, aql(), SAVED))
}
