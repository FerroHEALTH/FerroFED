// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Receiving a document in the exchange format (feature `fhir-r4`).
//!
//! Annex II 2.2 and 2.3 of Regulation (EU) 2025/327 ask an EHR system that
//! stores, intermediates or gives access to personal electronic health data
//! to "be able to receive" them in the European exchange format, by means of
//! this component. A [`ReceivedDocument`] is that receipt: a FHIR R4 document
//! `Bundle`, decoded strictly against the R4 resource definitions, held to
//! the R4 document rules, with its subject resolved to the `Patient` entry
//! and its original text kept byte for byte.
//!
//! - [`conform`]: the document checked against the profiles of its category,
//!   read from the vendored package;
//! - `openehr` (feature `openehr`): the document mapped into one openEHR
//!   composition by FHIRconnect, the original kept beside the mapped content.
//!
//! Nothing here reaches a CDR. Which member stores the composition, and how
//! it is reached, is the system's to decide.
//!
//! # Examples
//!
//! ```no_run
//! use eehrxf::receive::ReceivedDocument;
//!
//! let text = std::fs::read_to_string("document.json")?;
//! let document = ReceivedDocument::read(&text)?;
//! assert_eq!(document.original(), text);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod conform;
mod entries;
#[cfg(feature = "openehr")]
pub mod openehr;
mod references;
mod strict;

use fhir_types::codec::DecodeError;
use fhir_types::codec::EncodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::composition::Composition;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;

/// The `Bundle.type` of a document
/// (<https://hl7.org/fhir/R4/valueset-bundle-type.html>).
pub const DOCUMENT: &str = "document";

/// A document received in the exchange format, read and held to the R4
/// document rules.
#[derive(Debug, Clone)]
pub struct ReceivedDocument {
    original: String,
    tree: Object,
    bundle: Bundle,
    composition: Box<Composition>,
    patient: Box<Patient>,
}

impl ReceivedDocument {
    /// Reads a document from its FHIR JSON text.
    ///
    /// The text is read once. An object that repeats a name is refused
    /// first, because two JSON readers may keep different values for it
    /// (RFC 8259 §4). The text is then decoded against the R4 `Bundle` and
    /// the definitions of every resource it carries, which refuse an unknown
    /// property, a value of the wrong type and an invalid primitive
    /// (<https://hl7.org/fhir/R4/json.html>), and the decoded document must
    /// encode back to the JSON it was read from, so nothing the model does
    /// not hold is left in the text. Every later rule, the profile check and
    /// the mapping read that decoded document and nothing else.
    ///
    /// The `Bundle` must then be a `document` with the rules the R4 `Bundle`
    /// states for one: an `identifier` with a system and a value (`bdl-9`), a
    /// `timestamp` (`bdl-10`), `fullUrl`s that are given once (`bdl-7`), not
    /// version specific (`bdl-8`) and agree with their resource, and a
    /// `Composition` as its first entry (`bdl-11`)
    /// (<https://hl7.org/fhir/R4/bundle.html>). The `Composition.subject`
    /// must name the one `Patient` entry the document is about, and every
    /// other `subject` and `patient` reference in the document must name it
    /// too.
    ///
    /// # Errors
    ///
    /// Returns [`ReceiveError`] naming the first rule the text breaks. No
    /// refusal's message quotes a value of the document.
    pub fn read(text: &str) -> Result<Self, ReceiveError> {
        strict::unique_names(text).map_err(|source| {
            if source.is_data() {
                ReceiveError::RepeatedName { source }
            } else {
                ReceiveError::Json { source }
            }
        })?;
        let value: Value =
            serde_json::from_str(text).map_err(|source| ReceiveError::Json { source })?;
        let Value::Object(read) = value else {
            return Err(ReceiveError::NotAnObject);
        };
        if read.get("resourceType").and_then(Value::as_str) != Some("Bundle") {
            return Err(ReceiveError::NotABundle);
        }
        let bundle = Bundle::from_json(&read, &mut Path::root("Bundle"))
            .map_err(|source| ReceiveError::Decode { source })?;
        let tree = Json::to_json(&bundle).map_err(|source| ReceiveError::Encode { source })?;
        if tree != read {
            return Err(ReceiveError::Unfaithful);
        }
        if bundle.r#type.value.as_deref() != Some(DOCUMENT) {
            return Err(ReceiveError::NotADocument {
                found: bundle.r#type.value.clone(),
            });
        }
        let identified = bundle.identifier.as_ref().is_some_and(|identifier| {
            identifier
                .system
                .as_ref()
                .is_some_and(|system| system.value.is_some())
                && identifier
                    .value
                    .as_ref()
                    .is_some_and(|value| value.value.is_some())
        });
        if !identified {
            return Err(ReceiveError::Unidentified);
        }
        if bundle
            .timestamp
            .as_ref()
            .and_then(|timestamp| timestamp.value.as_ref())
            .is_none()
        {
            return Err(ReceiveError::Undated);
        }
        entries::known_resources(&tree, "Bundle")?;
        entries::full_urls(&tree)?;
        let Some(Resource::Composition(composition)) = bundle
            .entry
            .first()
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(ReceiveError::NoComposition);
        };
        let composition = composition.clone();
        let index = entries::patient(&tree)?;
        let Some(Resource::Patient(patient)) = bundle
            .entry
            .get(index)
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(ReceiveError::SubjectUnresolved);
        };
        let patient = patient.clone();
        Ok(Self {
            original: text.to_owned(),
            tree,
            bundle,
            composition,
            patient,
        })
    }

    /// Returns the document's text exactly as it was received.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Returns the decoded document.
    #[must_use]
    pub const fn bundle(&self) -> &Bundle {
        &self.bundle
    }

    /// Returns the document's `Composition`, its first entry.
    #[must_use]
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    /// Returns the `Patient` the document is about, the entry its
    /// `Composition.subject` names.
    #[must_use]
    pub fn patient(&self) -> &Patient {
        &self.patient
    }

    /// Returns the JSON the decoded document encodes as, equal to the JSON
    /// it was read from.
    pub(crate) const fn tree(&self) -> &Object {
        &self.tree
    }
}

/// Why a received text is not a document this component reads.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReceiveError {
    /// The text is not JSON.
    #[error("the document is not JSON")]
    Json {
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// An object of the document repeats a name, which two JSON readers may
    /// read as two different documents (RFC 8259 §4).
    #[error("an object of the document repeats a name")]
    RepeatedName {
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// The JSON is not an object.
    #[error("the document is not a JSON object")]
    NotAnObject,
    /// The object's `resourceType` is not `Bundle`.
    #[error("the document is not a Bundle")]
    NotABundle,
    /// The Bundle does not decode against the R4 definitions.
    #[error("the Bundle does not decode against FHIR R4")]
    Decode {
        /// The decode failure, located by element path.
        #[source]
        source: DecodeError,
    },
    /// The decoded Bundle cannot be encoded as JSON again.
    #[error("the decoded Bundle cannot be encoded as JSON")]
    Encode {
        /// The encode failure.
        #[source]
        source: EncodeError,
    },
    /// The decoded Bundle encodes as JSON other than the text it was read
    /// from: the text holds something the R4 model does not.
    #[error("the document reads differently through the R4 model")]
    Unfaithful,
    /// The Bundle's type is not `document`.
    #[error("the Bundle is of type {found:?}, not a document")]
    NotADocument {
        /// The type the Bundle carries, a code of the R4 value set.
        found: Option<String>,
    },
    /// The document carries no identifier with a system and a value
    /// (`bdl-9`).
    #[error("the document carries no identifier with a system and a value (bdl-9)")]
    Unidentified,
    /// The document carries no timestamp (`bdl-10`).
    #[error("the document carries no timestamp (bdl-10)")]
    Undated,
    /// The document's first entry is no `Composition` (`bdl-11`).
    #[error("the document's first entry is no Composition (bdl-11)")]
    NoComposition,
    /// The `Composition` names no subject.
    #[error("the document's Composition names no subject")]
    NoSubject,
    /// The subject resolves to no single `Patient` entry of the document.
    #[error("the document's subject is no single Patient entry of the document")]
    SubjectUnresolved,
    /// A resource of the document is of a type R4 does not define.
    #[error("{location} is a resource of a type FHIR R4 does not define")]
    UnknownResource {
        /// The element path of the resource.
        location: String,
    },
    /// An entry's `fullUrl` is version specific (`bdl-8`).
    #[error("entry {entry} has a version-specific fullUrl (bdl-8)")]
    VersionedFullUrl {
        /// The entry's index.
        entry: usize,
    },
    /// An entry repeats the `fullUrl` and version of an earlier one
    /// (`bdl-7`).
    #[error("entry {entry} repeats the fullUrl of an earlier entry (bdl-7)")]
    DuplicateFullUrl {
        /// The entry's index.
        entry: usize,
    },
    /// An entry's REST-style `fullUrl` disagrees with its resource's type or
    /// id.
    #[error("entry {entry} has a fullUrl that disagrees with its resource's type or id")]
    FullUrlMismatch {
        /// The entry's index.
        entry: usize,
    },
    /// An entry other than the subject is a `Patient`.
    #[error("entry {entry} is a second Patient")]
    SeveralPatients {
        /// The entry's index.
        entry: usize,
    },
    /// A resource of the document contains a `Patient`.
    #[error("{location} contains a Patient")]
    ContainedPatient {
        /// The element path of the contained resource.
        location: String,
    },
    /// A subject element (`subject`, `patient`, `beneficiary`, `for`) names
    /// anything but the document's `Patient` entry by reference.
    #[error("{location} names a subject other than the document's Patient entry")]
    SubjectMismatch {
        /// The element path of the reference.
        location: String,
    },
    /// A reference names, or could name, a patient other than the document's
    /// `Patient` entry: it resolves to no entry, and its path or `type` says
    /// `Patient` or nothing says what it names.
    #[error("{location} could name a patient other than the document's Patient entry")]
    PatientReference {
        /// The element path of the reference.
        location: String,
    },
    /// A reference's `type` disagrees with the resource it resolves to.
    #[error("{location} declares a type its target does not have")]
    ReferenceType {
        /// The element path of the reference.
        location: String,
    },
    /// A local `#id` reference names no resource contained in its resource.
    #[error("{location} names no contained resource")]
    LocalUnresolved {
        /// The element path of the reference.
        location: String,
    },
    /// An element the R4 element table does not describe, which no rule can
    /// read.
    #[error("{location} is an element the R4 element table does not describe")]
    Unreadable {
        /// The element path.
        location: String,
    },
}
