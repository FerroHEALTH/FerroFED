// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The term mappings a FHIRconnect run wrote as no coding, read back so the
//! narrative carries them (feature `openehr`).
//!
//! FHIRconnect writes an RM `TERM_MAPPING` whose `match` is `'='` as a
//! coding after the original, and reports one whose `match` is `'>'`,
//! `'<'` or `'?'` as a located loss, because such a mapping is "not
//! mappable to FHIR" (FHIRconnect 1.0.0, `types-of-mappings/data-type/TERM_MAPPING.adoc`).
//! The exchange format's rule for those is the one [`transcode`] applies:
//! no coding, and the mapping written into the narrative (eHealth Network
//! Guidelines on Patient Summary, Release 3.4, Art 10(3)). [`Withheld`]
//! reads such a loss, and [`Withheld::narrative`] is its narrative line.
//!
//! [`transcode`]: crate::terminology::transcode

use fhirconnect::engine::outcome::{Loss, SkipReason, Warning};

use crate::terminology::transcode::Relation;

/// A term mapping the run wrote as no coding, because its `match` states a
/// broader, a narrower or an unknown relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withheld {
    relation: Relation,
    terminology: String,
    code: String,
    element: Option<String>,
}

impl Withheld {
    /// Returns the withheld mapping `warning` declares, or `None` for any
    /// other loss.
    ///
    /// A `match` outside `'>'`, `'<'` and `'?'` is none, because only those
    /// are withheld: `'='` is written as a coding.
    #[must_use]
    pub fn of(warning: &Warning) -> Option<Self> {
        let Loss::Skipped {
            reason:
                SkipReason::TermMappingMatch {
                    match_code,
                    terminology,
                    code,
                },
            ..
        } = warning.loss()
        else {
            return None;
        };
        Some(Self {
            relation: Relation::of(*match_code)?,
            terminology: terminology.clone(),
            code: code.clone(),
            element: warning.location().fhir().map(str::to_owned),
        })
    }

    /// Returns the relation the mapping states.
    #[must_use]
    pub const fn relation(&self) -> Relation {
        self.relation
    }

    /// Returns the name of the target's terminology id.
    #[must_use]
    pub fn terminology(&self) -> &str {
        &self.terminology
    }

    /// Returns the target's code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the `FHIRPath` of the element the coding was withheld from, when
    /// the run located one.
    #[must_use]
    pub fn element(&self) -> Option<&str> {
        self.element.as_deref()
    }

    /// Returns the narrative line of the mapping: the element, the relation
    /// in words and the target, in the form [`transcode`] writes a mapping no
    /// coding carries.
    ///
    /// [`transcode`]: crate::terminology::transcode
    #[must_use]
    pub fn narrative(&self) -> String {
        let element = self.element.as_deref().unwrap_or("A coded value");
        format!(
            "{element}: {} {} {}",
            self.relation.words(),
            self.terminology,
            self.code
        )
    }
}
