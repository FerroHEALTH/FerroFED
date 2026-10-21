// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document mapped into openEHR, the original kept beside it
//! (feature `openehr`).
//!
//! FHIRconnect's `$toopenehr` maps one Bundle into one composition through
//! the context whose profile the Bundle's resources claim
//! (<https://sevkohler.github.io/FHIRconnect-spec/build/site/FHIRconnect/v1.0.0/basics/main.html>).
//! [`Mapping::to_openehr`] runs it in process over the received document,
//! then keeps the document itself in the composition: the openEHR RM lets
//! `FEEDER_AUDIT.original_content` carry "the original content item itself
//! ... included inline", and puts a single one at the composition node to
//! establish "basic equivalent between the whole Composition and the whole
//! document" (RM Release 1.1.0, Common IM, §Original Content,
//! <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
//! What the mapping does not carry into structured content is therefore
//! still stored, and what the run declared lost travels beside the
//! composition as an `OperationOutcome`.

use std::fmt;

use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;
use fhirconnect::operations::run::Settings;
use openehr_base::containers::NonEmptyVec;
use openehr_rm::v1_2::common::archetyped::feeder_audit::FeederAudit;
use openehr_rm::v1_2::common::archetyped::feeder_audit_details::FeederAuditDetails;
use openehr_rm::v1_2::common::generic::party_identified::{PartyIdentified, PartyIdentifiedData};
use openehr_rm::v1_2::common::generic::party_proxy::PartyProxy;
use openehr_rm::v1_2::common::generic::party_self::PartySelf;
use openehr_rm::v1_2::composition::composition::Composition;
use openehr_rm::v1_2::composition::content::content_item::ContentItem;
use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;
use openehr_rm::v1_2::data_types::encapsulated::dv_encapsulated::DvEncapsulated;
use openehr_rm::v1_2::data_types::encapsulated::dv_parsable::DvParsable;

use crate::mapping::Mapping;
use crate::receive::ReceivedDocument;

/// The media type of FHIR JSON (<https://hl7.org/fhir/R4/json.html>), the
/// `DV_PARSABLE.formalism` the original document is kept under.
pub const FHIR_JSON: &str = "application/fhir+json";

/// The openEHR composition a received document maps to.
#[derive(Debug, Clone)]
pub struct ReceivedComposition {
    model: Composition,
    composition: String,
    template_id: String,
    outcome: Option<OperationOutcome>,
}

// NOTE: no specification governs this: our own design; the canonical JSON is written from
// the model, so two compositions are equal when their canonical JSON is.
impl PartialEq for ReceivedComposition {
    fn eq(&self, other: &Self) -> bool {
        self.composition == other.composition
            && self.template_id == other.template_id
            && self.outcome == other.outcome
    }
}

impl Eq for ReceivedComposition {}

impl ReceivedComposition {
    /// Returns the composition in canonical JSON, the body of an ITS-REST
    /// `composition_create`.
    #[must_use]
    pub fn composition(&self) -> &str {
        &self.composition
    }

    /// Returns the composition as the RM model holds it, the typed body the
    /// `openehr-its` client sends to `composition_create`.
    #[must_use]
    pub const fn model(&self) -> &Composition {
        &self.model
    }

    /// Returns the template the composition is of.
    #[must_use]
    pub fn template_id(&self) -> &str {
        &self.template_id
    }

    /// Returns what the run declared lost, when it declared a loss.
    #[must_use]
    pub const fn outcome(&self) -> Option<&OperationOutcome> {
        self.outcome.as_ref()
    }
}

impl Mapping {
    /// Maps a received document into one openEHR composition, the document
    /// itself kept in the composition's `FEEDER_AUDIT.original_content`.
    ///
    /// The whole document Bundle is the `$toopenehr` input, so the context
    /// that maps it is the one whose profile its resources claim, and a
    /// mapping may follow the document's references into its other entries.
    /// The composition is read back as RM 1.2 canonical JSON, the original
    /// document is set as a `DV_PARSABLE` with the formalism [`FHIR_JSON`],
    /// and it is written out again. `settings` names the engine's device,
    /// the system its `FEEDER_AUDIT` records.
    ///
    /// # Errors
    ///
    /// Returns [`ReceiveMappingError::Run`] when the engine refuses the
    /// document, [`ReceiveMappingError::Composition`] when its composition
    /// does not read as RM canonical JSON, [`ReceiveMappingError::Untemplated`]
    /// when it names no template, [`ReceiveMappingError::OriginalTaken`] when
    /// the mapping already wrote an original content, and
    /// [`ReceiveMappingError::Encode`] when it cannot be written out.
    pub fn to_openehr(
        &self,
        document: &ReceivedDocument,
        settings: &Settings,
    ) -> Result<ReceivedComposition, ReceiveMappingError> {
        let request = ToOpenehrRequest::new(document.bundle().clone());
        let response = run::to_openehr(
            self.programs(),
            &SCHEMAS,
            &Seams::default(),
            settings,
            &request,
        )
        .map_err(|source| ReceiveMappingError::Run {
            source: Box::new(source),
        })?;
        let mut composition: Composition = serde_json::from_str(response.composition())
            .map_err(|source| ReceiveMappingError::Composition { source })?;
        let template_id = composition
            .archetype_details
            .as_ref()
            .and_then(|details| details.template_id.as_ref())
            .map(|template| template.value.clone())
            .ok_or(ReceiveMappingError::Untemplated)?;
        let audit = composition.feeder_audit.get_or_insert_with(|| FeederAudit {
            originating_system_item_ids: None,
            feeder_system_item_ids: None,
            original_content: None,
            originating_system_audit: Box::new(FeederAuditDetails {
                system_id: settings.device().to_owned(),
                location: None,
                subject: None,
                provider: None,
                time: None,
                version_id: None,
                other_details: None,
            }),
            feeder_system_audit: None,
        });
        if audit.original_content.is_some() {
            return Err(ReceiveMappingError::OriginalTaken);
        }
        audit.original_content = Some(DvEncapsulated::DvParsable(DvParsable {
            charset: None,
            language: None,
            value: document.original().to_owned(),
            formalism: FHIR_JSON.to_owned(),
        }));
        let canonical = serde_json::to_string(&composition)
            .map_err(|source| ReceiveMappingError::Encode { source })?;
        Ok(ReceivedComposition {
            model: composition,
            composition: canonical,
            template_id,
            outcome: response.outcome().cloned(),
        })
    }
}

impl ReceivedComposition {
    /// Returns this composition with every entry's provider the subject of
    /// the record, for a document the subject inserted.
    ///
    /// The openEHR RM defines `ENTRY.provider` as "the agent who provided
    /// the information", which "is usually the patient or the clinician"
    /// (RM Release 1.1.0, EHR IM, §Entry and its Subclasses,
    /// <https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_entry_class>),
    /// and refers to the subject of the record with a `PARTY_SELF` (Common
    /// IM, §`PARTY_SELF` and Referring to the Patient from the EHR,
    /// <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_party_self_class>).
    /// Each entry, in the composition and in every section, gets a
    /// `PARTY_SELF` with no `external_ref`, so the mark names no identifier
    /// of the patient. A provider the mapping wrote is replaced; the
    /// document as received stays in the original content.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectInsertionError::NoEntry`] when the composition holds
    /// no entry, [`SubjectInsertionError::GenericEntry`] when it holds a
    /// `GENERIC_ENTRY`, which has no provider, and
    /// [`SubjectInsertionError::Encode`] when it cannot be written out.
    pub fn inserted_by_subject(self) -> Result<Self, SubjectInsertionError> {
        self.provided_by(&PartyProxy::PartySelf(PartySelf { external_ref: None }))
    }

    /// Returns this composition with every entry's provider `representative`,
    /// for a document a representative of the record's subject inserted for
    /// them.
    ///
    /// The Common IM refers to the subject of the record with a
    /// `PARTY_SELF`, "while `PARTY_IDENTIFIED` is used for all other
    /// situations" (RM Release 1.1.0, Common IM, §Referring to Demographic
    /// Entities,
    /// <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_referring_to_demographic_entities>),
    /// so each entry, in the composition and in every section, names the
    /// representative as a `PARTY_IDENTIFIED`, never as the subject. A
    /// provider the mapping wrote is replaced; the document as received
    /// stays in the original content.
    ///
    /// # Errors
    ///
    /// As [`ReceivedComposition::inserted_by_subject`].
    pub fn inserted_by_representative(
        self,
        representative: &Representative,
    ) -> Result<Self, SubjectInsertionError> {
        self.provided_by(&PartyProxy::PartyIdentified(
            PartyIdentified::PartyIdentified(representative.party.clone()),
        ))
    }

    /// Returns this composition with every entry's provider `provider`.
    fn provided_by(mut self, provider: &PartyProxy) -> Result<Self, SubjectInsertionError> {
        let marked = match self.model.content.as_mut() {
            Some(content) => mark(content, provider)?,
            None => 0,
        };
        if marked == 0 {
            return Err(SubjectInsertionError::NoEntry);
        }
        self.composition = serde_json::to_string(&self.model)
            .map_err(|source| SubjectInsertionError::Encode { source })?;
        Ok(self)
    }
}

/// A natural person who acts for the subject of a record, as a proxy or a
/// legal representative (Regulation (EU) 2025/327 Art 4(2)), named as a
/// `PARTY_IDENTIFIED`.
///
/// It holds at least one of a name and an identifier, as `PARTY_IDENTIFIED`
/// requires when it carries no `external_ref` (RM Release 1.1.0, Common IM,
/// `PARTY_IDENTIFIED` invariant `Basic_validity`). `Debug` shows which of
/// the two it holds and neither value.
#[derive(Clone, PartialEq)]
pub struct Representative {
    /// The representative as the provider every entry names.
    party: PartyIdentifiedData,
}

impl Representative {
    /// Returns the representative named `name` and identified by
    /// `identifier`, or `None` when neither is given.
    ///
    /// An empty name, or an identifier whose `id` is empty, counts as not
    /// given (RM Release 1.1.0, Common IM, `PARTY_IDENTIFIED` invariant
    /// `Name_valid`; Data Types IM, `DV_IDENTIFIER` invariant `Id_valid`).
    #[must_use]
    pub fn new(name: Option<String>, identifier: Option<DvIdentifier>) -> Option<Self> {
        let name = name.filter(|name| !name.is_empty());
        let identifiers = identifier
            .filter(|identifier| !identifier.id.is_empty())
            .map(NonEmptyVec::of);
        if name.is_none() && identifiers.is_none() {
            return None;
        }
        Some(Self {
            party: PartyIdentifiedData {
                external_ref: None,
                name,
                identifiers,
            },
        })
    }
}

impl fmt::Debug for Representative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Representative")
            .field("name", &self.party.name.is_some())
            .field("identifier", &self.party.identifiers.is_some())
            .finish()
    }
}

/// Sets the provider of every entry among `items`, and in their sections,
/// to `provider`, and returns how many it set.
fn mark(items: &mut [ContentItem], provider: &PartyProxy) -> Result<usize, SubjectInsertionError> {
    let mut marked = 0_usize;
    for item in items {
        let named = match item {
            ContentItem::Section(section) => {
                let within = match section.items.as_mut() {
                    Some(items) => mark(items, provider)?,
                    None => 0,
                };
                marked = marked.saturating_add(within);
                continue;
            }
            ContentItem::GenericEntry(_) => return Err(SubjectInsertionError::GenericEntry),
            ContentItem::Action(entry) => &mut entry.provider,
            ContentItem::AdminEntry(entry) => &mut entry.provider,
            ContentItem::Evaluation(entry) => &mut entry.provider,
            ContentItem::Instruction(entry) => &mut entry.provider,
            ContentItem::Observation(entry) => &mut entry.provider,
        };
        *named = Some(provider.clone());
        marked = marked.saturating_add(1);
    }
    Ok(marked)
}

/// Why a composition cannot be marked as inserted by its subject.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SubjectInsertionError {
    /// The composition holds no entry, so nothing in it can carry the mark.
    #[error("the composition holds no entry a provider can be named in")]
    NoEntry,
    /// The composition holds a `GENERIC_ENTRY`, which has no provider.
    #[error("the composition holds a GENERIC_ENTRY, which names no provider")]
    GenericEntry,
    /// The marked composition cannot be written as canonical JSON.
    #[error("the marked composition cannot be written as canonical JSON")]
    Encode {
        /// The write failure.
        #[source]
        source: serde_json::Error,
    },
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
mod tests {
    use std::error::Error;

    use openehr_rm::v1_2::composition::composition::Composition;

    use super::{ReceivedComposition, SubjectInsertionError};

    /// A synthetic composition whose `content` member is `content`.
    fn composition(content: &str) -> Result<ReceivedComposition, Box<dyn Error>> {
        let text = format!(
            r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Synthetic" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.synthetic.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.synthetic.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "synthetic.v1" }},
    "rm_version": "1.1.0"
  }},
  "language": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }}, "code_string": "en" }},
  "territory": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }}, "code_string": "NL" }},
  "category": {{ "_type": "DV_CODED_TEXT", "value": "persistent", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "431" }} }},
  "composer": {{ "_type": "PARTY_SELF" }}{content}
}}"#
        );
        let model: Composition = serde_json::from_str(&text)?;
        Ok(ReceivedComposition {
            composition: serde_json::to_string(&model)?,
            model,
            template_id: String::from("synthetic.v1"),
            outcome: None,
        })
    }

    #[test]
    fn a_composition_with_no_entry_cannot_carry_the_mark() -> Result<(), Box<dyn Error>> {
        let refused = composition("")?.inserted_by_subject();
        assert!(
            matches!(refused, Err(SubjectInsertionError::NoEntry)),
            "{refused:?}"
        );
        let empty = composition(
            r#",
  "content": [{ "_type": "SECTION", "name": { "_type": "DV_TEXT", "value": "Empty" }, "archetype_node_id": "openEHR-EHR-SECTION.synthetic.v1" }]"#,
        )?;
        let refused = empty.inserted_by_subject();
        assert!(
            matches!(refused, Err(SubjectInsertionError::NoEntry)),
            "a section with no entry carries no mark: {refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_generic_entry_cannot_carry_the_mark() -> Result<(), Box<dyn Error>> {
        let generic = composition(
            r#",
  "content": [{
    "_type": "GENERIC_ENTRY",
    "name": { "_type": "DV_TEXT", "value": "Legacy" },
    "archetype_node_id": "openEHR-EHR-GENERIC_ENTRY.synthetic.v1",
    "data": { "_type": "ELEMENT", "name": { "_type": "DV_TEXT", "value": "Item" }, "archetype_node_id": "at0001" }
  }]"#,
        )?;
        let refused = generic.inserted_by_subject();
        assert!(
            matches!(refused, Err(SubjectInsertionError::GenericEntry)),
            "{refused:?}"
        );
        Ok(())
    }
}

/// Why a received document gives no composition.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReceiveMappingError {
    /// The FHIRconnect engine refused the document: no context maps it, it
    /// names several subjects, or it carries an element the mapping cannot
    /// carry.
    #[error("the mapping run was refused")]
    Run {
        /// The engine's refusal.
        #[source]
        source: Box<OperationError>,
    },
    /// The engine's composition does not read as RM canonical JSON.
    #[error("the mapped composition does not read as RM canonical JSON")]
    Composition {
        /// The read failure.
        #[source]
        source: serde_json::Error,
    },
    /// The mapped composition names no template.
    #[error("the mapped composition names no template")]
    Untemplated,
    /// The mapping already wrote the composition's original content.
    #[error("the mapping already wrote the composition's original content")]
    OriginalTaken,
    /// The composition cannot be written as canonical JSON.
    #[error("the composition cannot be written as canonical JSON")]
    Encode {
        /// The write failure.
        #[source]
        source: serde_json::Error,
    },
}
