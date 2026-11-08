// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a summary cannot be assembled.

use eehrxf::document::DocumentError;
use eehrxf::mapping::MappingError;
use eehrxf::terminology::translation::TranslationError;
use eehrxf::transformation::ReportError;

use crate::summary::mappings::MappingsError;

/// Why the document cannot be assembled.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssemblyError {
    /// A section has no EPS slot.
    #[error(transparent)]
    Slot(#[from] MappingsError),
    /// A mapping refused a composition a member answered with.
    #[error("the mapping of a composition of the template {template} was refused")]
    Mapping {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: MappingError,
    },
    /// The document could not be written.
    #[error(transparent)]
    Document(#[from] DocumentError),
    /// A crosswalk gap names no EPS section slot.
    #[error("the gap {element} names no section of the patient summary composition")]
    Gap {
        /// The Xt-EHR element path of the gap.
        element: &'static str,
    },
    /// A section answer names an origin that was not given.
    #[error("a section answer names origin {index}, which was not given")]
    Origin {
        /// The index.
        index: usize,
    },
    /// The transformation of a composition a member answered with could not
    /// be counted.
    #[error("the transformation of a composition of the template {template} could not be counted")]
    Report {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: ReportError,
    },
    /// The codes of a mapped resource could not be translated, because the
    /// terminology server refused or did not answer, which fails the
    /// document rather than leaving a code untranslated.
    #[error(
        "the codes mapped from a composition of the template {template} could not be translated"
    )]
    Translation {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: TranslationError,
    },
    /// The header names the patient by no family name, given name or text,
    /// which the HL7 Europe Patient Summary `Patient` requires (`ips-pat-1`).
    #[error("the patient header holds no name, which the EPS Patient requires (ips-pat-1)")]
    Unnamed,
}
