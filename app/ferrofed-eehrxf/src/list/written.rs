// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entry of a report the gateway writes on read ([`Written`]).
//!
//! The entry is the description the European interoperability component
//! writes of the member's composition before the report is written
//! ([`DocumentMetadata::describe_at`]).

use eehrxf::document::DocumentError;
use eehrxf::document::metadata::{DocumentMetadata, Header};
use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::DocumentReference;
use fhir_types::r4::identifier::Identifier;

use super::{
    HOSPITAL_DISCHARGE_SUMMARY, IMAGING_STUDY, LABORATORY_REPORT, LOINC, Listing, URI_SYSTEM,
};
use crate::face::FHIR_JSON;
use crate::report::Written;
use crate::summary::Organisation;

/// Why a member's composition gives no entry.
#[derive(Debug, thiserror::Error)]
pub enum Undescribed {
    /// The composition is not JSON.
    #[error("the composition is not JSON")]
    Json(#[source] serde_json::Error),
    /// The component refused the description.
    #[error("the composition cannot be described")]
    Document(#[source] DocumentError),
}

/// A member's composition the gateway writes as a report.
#[derive(Debug, Clone, Copy)]
pub struct Writing<'a> {
    /// The category of the report.
    pub written: Written,
    /// The composition, in the member's canonical JSON.
    pub composition: &'a str,
    /// The operator, who writes the report.
    pub operator: &'a Organisation,
    /// The member that holds the composition.
    pub custodian: &'a Organisation,
}

/// Returns the entry, with the logical id `id` and the unique id `unique`,
/// a `urn:uuid:` URN, of the report `writing` names, read at `url`, a URL
/// the gateway signs relative to the FHIR base.
///
/// The document is in the category's `Bundle` profile
/// ([`Written::bundle`]) and of the type the list names it by, a code of
/// MyHealth@EU `eHDSIDocumentTypes`, which `DocumentReference-minimal` binds
/// as required: LOINC `11502-2` for a laboratory report, `34105-7` for a
/// discharge report and `18748-4` for an imaging report. Its
/// version, language and period are the composition's (openEHR RM 1.1.0
/// `COMPOSITION`).
///
/// # Errors
///
/// [`Undescribed`] for a composition that is not JSON, or that carries no
/// `uid` naming a version, which draft `EEHRxF` Annex III requires.
pub fn entry(
    writing: Writing<'_>,
    (id, unique): (&str, &str),
    listing: Listing<'_>,
    url: &str,
) -> Result<DocumentReference, Undescribed> {
    let (code, display, title) = match writing.written {
        Written::Discharge => (
            HOSPITAL_DISCHARGE_SUMMARY,
            "Hospital Discharge summary",
            "Discharge report written by the gateway from the member's record",
        ),
        Written::Imaging => (
            IMAGING_STUDY,
            "Diagnostic imaging study",
            "Imaging report written by the gateway from the member's record",
        ),
        Written::Laboratory => (
            LABORATORY_REPORT,
            "Laboratory report",
            "Laboratory report written by the gateway from the member's record",
        ),
    };
    let concept = |display: Option<&str>| CodeableConcept {
        coding: vec![Coding {
            system: Some(LOINC.into()),
            code: Some(code.into()),
            display: display.map(Into::into),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    };
    let composition = serde_json::from_str(writing.composition).map_err(Undescribed::Json)?;
    // NOTE: draft EEHRxF Annex III Table 1 documentCategory names no value set, so the category is
    // the LOINC document type the list names the report by.
    let metadata = DocumentMetadata::from_composition(
        &composition,
        concept(None),
        Some(writing.written.priority()),
    )
    .map_err(Undescribed::Document)?;
    // NOTE: no specification governs this: our own design; the operator writes the report on read,
    // so it is the author the description names.
    let header = Header::new(
        Identifier {
            system: Some(URI_SYSTEM.into()),
            value: Some(unique.into()),
            ..Identifier::default()
        },
        super::subject(listing),
        listing.date.to_owned(),
        concept(Some(display)),
        title.to_owned(),
        super::organisation(writing.operator),
    )
    .with_custodian(super::organisation(writing.custodian));
    metadata
        .describe_at(
            &header,
            id,
            Attachment {
                content_type: Some(FHIR_JSON.into()),
                url: Some(url.into()),
                title: Some(title.into()),
                ..Attachment::default()
            },
        )
        .map_err(Undescribed::Document)
}
