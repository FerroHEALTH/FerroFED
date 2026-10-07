// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The priority category of a received document, and the profiles it is
//! checked against.
//!
//! Each HL7 Europe document profile fixes the `Composition.type` of its
//! category with a `patternCodeableConcept`, such as LOINC `60591-5` in
//! `composition-eu-eps`, so a document says its category in the element its
//! profile constrains (<https://hl7.org/fhir/R4/profiling.html>).
//! [`ReceivedDocument::category`] reads that element against the categories
//! this build receives, and [`Category::profiles`] names the `Bundle` and
//! `Composition` profiles [`ReceivedDocument::check`] holds it to.

use crate::category::Category;
use crate::receive::ReceivedDocument;

/// The LOINC code system (<https://hl7.org/fhir/R4/loinc.html>).
pub const LOINC: &str = "http://loinc.org";

/// The `Composition.type` coding a category's document profile fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentType {
    /// The code system.
    pub system: &'static str,
    /// The code.
    pub code: &'static str,
}

/// The `Bundle` and `Composition` profiles of a category's documents, by
/// canonical URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profiles {
    /// The `Bundle` profile.
    pub bundle: &'static str,
    /// The `Composition` profile.
    pub composition: &'static str,
}

impl Category {
    /// Returns the `Composition.type` the category's document profile fixes,
    /// when this build receives documents of the category.
    #[must_use]
    pub const fn document_type(self) -> Option<DocumentType> {
        match self {
            // NOTE: HL7 Europe Patient Summary 1.0.0-ballot, composition-eu-eps: the
            // profile fixes Composition.type to LOINC 60591-5.
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => Some(DocumentType {
                system: LOINC,
                code: "60591-5",
            }),
            #[cfg_attr(
                not(any(
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "imaging",
                    feature = "laboratory",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "every category this build carries is received"
                )
            )]
            _ => None,
        }
    }

    /// Returns the profiles a document of the category is checked against,
    /// when this build receives documents of the category.
    #[must_use]
    pub const fn profiles(self) -> Option<Profiles> {
        match self {
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => Some(Profiles {
                bundle: "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps",
                composition: "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps",
            }),
            #[cfg_attr(
                not(any(
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "imaging",
                    feature = "laboratory",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "every category this build carries is received"
                )
            )]
            _ => None,
        }
    }
}

impl ReceivedDocument {
    /// Returns the category of the document: the one category this build
    /// receives whose document type a `Composition.type` coding carries.
    ///
    /// # Errors
    ///
    /// Returns [`Uncategorised::Unknown`] when no coding names a category
    /// this build receives, and [`Uncategorised::Several`] when the codings
    /// name more than one.
    pub fn category(&self) -> Result<Category, Uncategorised> {
        let codings = &self.composition().r#type.coding;
        let mut named = Category::ENABLED.iter().copied().filter(|category| {
            category.document_type().is_some_and(|wanted| {
                codings.iter().any(|coding| {
                    coding
                        .system
                        .as_ref()
                        .and_then(|system| system.value.as_deref())
                        == Some(wanted.system)
                        && coding.code.as_ref().and_then(|code| code.value.as_deref())
                            == Some(wanted.code)
                })
            })
        });
        match (named.next(), named.next()) {
            (Some(category), None) => Ok(category),
            (Some(_), Some(_)) => Err(Uncategorised::Several),
            (None, _) => Err(Uncategorised::Unknown),
        }
    }
}

/// Why a received document is of no one category this build receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Uncategorised {
    /// No `Composition.type` coding names a category this build receives.
    #[error("the document's Composition.type names no category this system receives")]
    Unknown,
    /// The `Composition.type` codings name more than one category.
    #[error("the document's Composition.type names more than one category")]
    Several,
}
