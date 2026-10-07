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

use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::traverse::functions::NoMappingFunctions;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;
use fhirconnect::operations::run::Settings;
use openehr_rm::v1_2::common::archetyped::feeder_audit::FeederAudit;
use openehr_rm::v1_2::common::archetyped::feeder_audit_details::FeederAuditDetails;
use openehr_rm::v1_2::composition::composition::Composition;
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
            &NoMappingFunctions,
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
