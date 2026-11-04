// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries of a member that serves electronic prescriptions and
//! dispensations from its openEHR data (Regulation (EU) 2025/327 Art
//! 14(1)(b) and (c)), the map mode of the FHIR face.
//!
//! A prescription is one medication order and a dispensation one medication
//! action at its dispensed step, as the medication archetypes of the openEHR
//! international Clinical Knowledge Manager record them (vendored under
//! `docs/specs/openehr-ckm-medication/`). Each has its own query, held
//! read-only under the reserved namespace at [`VERSION`] (§12.7, N44), in
//! the shape of [`section_query`]: every whole composition that contains
//! the entry, with its uid and its template id.
//!
//! A prescription is served only by the member that owns it (A102 point 1):
//! the gateway reads the `creating_system_id` of each composition's uid, and
//! the order query leaves out every composition with a `feeder_audit`, the
//! "audit trail from non-openEHR system of original commit of information"
//! an imported copy carries (RM 1.1.0, `LOCATABLE.feeder_audit`). The
//! action query keeps them: the
//! dispensation the face writes is itself a composition whose
//! `FEEDER_AUDIT.original_content` keeps the received dispense, so the
//! filter would hide every dispensation received through the gateway.

use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use openehr_query::ast::{IdentifiedExpr, SelectQuery, WhereExpr};
use openehr_query::printer;

use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};

/// The medication order, one prescription item.
pub const ORDER: Archetype = Archetype {
    class: "INSTRUCTION",
    id: "openEHR-EHR-INSTRUCTION.medication_order.v3",
};

/// The medication action, one step of the management of an order, a
/// dispensation among them.
pub const ACTION: Archetype = Archetype {
    class: "ACTION",
    id: "openEHR-EHR-ACTION.medication.v1",
};

/// One of the two medication queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Medication {
    /// The compositions holding a medication order the member owns.
    Orders,
    /// The compositions holding a medication action.
    Actions,
}

impl Medication {
    /// Both queries, in the order the registry holds them.
    pub const ALL: [Self; 2] = [Self::Orders, Self::Actions];

    /// The query's name in the reserved namespace.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Orders => "medication-orders",
            Self::Actions => "medication-actions",
        }
    }

    /// The entry archetype the query selects compositions by.
    #[must_use]
    pub const fn archetype(self) -> Archetype {
        match self {
            Self::Orders => ORDER,
            Self::Actions => ACTION,
        }
    }

    /// Returns the query's qualified name, such as
    /// `eu.ferrofed.eehrxf::medication-orders`.
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of [`QueryName::new`], which the name does
    /// not meet.
    pub fn name(self) -> Result<QueryName, QueryNameError> {
        QueryName::new(&format!("{NAMESPACE}::{}", self.slug()))
    }

    /// Returns the query as an AQL syntax tree.
    ///
    /// # Panics
    ///
    /// Never: the query names one archetype.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "archetype() names one archetype, which a test holds"
    )]
    pub fn query(self) -> SelectQuery {
        let mut query = section_query::query(&[self.archetype()], &[])
            .expect("a medication query should name an archetype");
        if self == Self::Orders {
            // NOTE: AQL 1.1.0 §EXISTS; A102 point 1: a composition with a feeder_audit is an
            // imported copy, which the member does not own.
            let imported = WhereExpr::Not(Box::new(WhereExpr::identified(IdentifiedExpr::Exists(
                section_query::identified("c", &["feeder_audit"]),
            ))));
            query.where_ = Some(match query.where_.take() {
                Some(patient) => WhereExpr::And(Box::new(patient), Box::new(imported)),
                None => imported,
            });
        }
        query
    }

    /// Returns the query as AQL text, printed from its syntax tree.
    #[must_use]
    pub fn aql(self) -> String {
        printer::to_aql(&self.query(), None)
    }

    /// Returns the query as the registry holds it: under its reserved name,
    /// at [`VERSION`], saved at [`SAVED`].
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of [`Medication::name`].
    pub fn definition(self) -> Result<StoredDefinition, QueryNameError> {
        Ok(StoredDefinition::new(
            self.name()?,
            VERSION,
            self.aql(),
            SAVED,
        ))
    }
}

/// Returns both medication queries as the registry holds them.
///
/// # Errors
///
/// The [`QueryNameError`] of a query's name, which no query meets.
pub fn definitions() -> Result<Vec<StoredDefinition>, QueryNameError> {
    Medication::ALL
        .iter()
        .map(|query| query.definition())
        .collect()
}
