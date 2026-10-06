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
#[cfg(feature = "openehr")]
pub mod openehr;

use fhir_types::codec::DecodeError;
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
    /// The text is decoded against the R4 `Bundle` and the definitions of
    /// every resource it carries, which refuse an unknown property, a value
    /// of the wrong type and an invalid primitive
    /// (<https://hl7.org/fhir/R4/json.html>). The `Bundle` must then be a
    /// `document` with the rules the R4 `Bundle` states for one: an
    /// `identifier` with a system and a value (`bdl-9`), a `timestamp`
    /// (`bdl-10`) and a `Composition` as its first entry (`bdl-11`)
    /// (<https://hl7.org/fhir/R4/bundle.html>). The `Composition.subject`
    /// must name the one `Patient` entry the document is about.
    ///
    /// # Errors
    ///
    /// Returns [`ReceiveError`] naming the first rule the text breaks. No
    /// variant carries a value of the document.
    pub fn read(text: &str) -> Result<Self, ReceiveError> {
        let value: Value =
            serde_json::from_str(text).map_err(|source| ReceiveError::Json { source })?;
        let Value::Object(tree) = value else {
            return Err(ReceiveError::NotAnObject);
        };
        if tree.get("resourceType").and_then(Value::as_str) != Some("Bundle") {
            return Err(ReceiveError::NotABundle);
        }
        let bundle = Bundle::from_json(&tree, &mut Path::root("Bundle"))
            .map_err(|source| ReceiveError::Decode { source })?;
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
        let Some(Resource::Composition(composition)) = bundle
            .entry
            .first()
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(ReceiveError::NoComposition);
        };
        let composition = composition.clone();
        let patient = subject(&bundle, &composition)?;
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

    /// Returns the document as the JSON tree it was read from.
    pub(crate) const fn tree(&self) -> &Object {
        &self.tree
    }
}

/// Returns the `Patient` entry `composition.subject` names.
///
/// A reference inside a Bundle resolves against the entries' `fullUrl`s: an
/// absolute reference equals one, and a relative `Patient/<id>` ends one
/// on the same server base (<https://hl7.org/fhir/R4/bundle.html#references>).
// NOTE: Regulation (EU) 2025/327 Art 13(3) registers data under the patient's
// identification data, so a document whose subject is no Patient entry of its
// own Bundle cannot be registered (no specification states this refusal: our own design).
fn subject(bundle: &Bundle, composition: &Composition) -> Result<Box<Patient>, ReceiveError> {
    let reference = composition
        .subject
        .as_ref()
        .and_then(|subject| subject.reference.as_ref())
        .and_then(|reference| reference.value.as_deref())
        .ok_or(ReceiveError::NoSubject)?;
    let relative = format!("/{reference}");
    let mut named = bundle.entry.iter().filter(|entry| {
        entry
            .full_url
            .as_ref()
            .and_then(|url| url.value.as_deref())
            .is_some_and(|url| url == reference || url.ends_with(&relative))
    });
    let (Some(entry), None) = (named.next(), named.next()) else {
        return Err(ReceiveError::SubjectUnresolved);
    };
    match entry.resource {
        Some(Resource::Patient(ref patient)) => Ok(patient.clone()),
        _ => Err(ReceiveError::SubjectUnresolved),
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
}
