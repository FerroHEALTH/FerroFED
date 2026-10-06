// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Emergency access: an access the accessor declared, through its purpose of
//! use, as made to protect the vital interests of the data subject.
//!
//! Regulation (EU) 2025/327 Art 11(5) lets a healthcare provider or health
//! professional be granted access to data a natural person restricted
//! under Art 8 "where necessary in order to protect the vital interests of
//! the data subject", and asks that "such cases shall be logged in a clear
//! and understandable format and shall be easily accessible for the data
//! subject". The deployment declares which purposes of use assert such an
//! access, as [`EmergencyPurposes`], and a record whose accessor declared
//! one of them carries an [`Emergency`] naming the purposes that marked it.
//!
//! The mark states what the accessor asserted, never what the recording
//! system inferred: it is read from the declared purposes alone. Whether
//! restricted data were released is the decision of the system that holds
//! them, and Art 8 keeps the fact of a restriction from the recording
//! system, so the mark does not say that restricted data were reached.

use std::collections::BTreeSet;

use crate::record::Purpose;

/// The purposes of use a deployment declares as asserting an emergency
/// access (Art 11(5)), such as the HL7 v3 `ActReason` code `BTG`, "break
/// the glass".
///
/// A declared purpose matches a purpose the accessor declared only when the
/// two codes are equal and the two systems are equal, an absent system
/// matching only an absent one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmergencyPurposes {
    purposes: BTreeSet<Purpose>,
}

impl EmergencyPurposes {
    /// The set of `purposes`.
    ///
    /// # Errors
    ///
    /// [`EmergencyError::EmptyCode`] for a purpose with an empty code, and
    /// [`EmergencyError::EmptySystem`] for one that names an empty system.
    pub fn declare(purposes: &[Purpose]) -> Result<Self, EmergencyError> {
        for (index, purpose) in purposes.iter().enumerate() {
            if purpose.code.trim().is_empty() {
                return Err(EmergencyError::EmptyCode { index });
            }
            if purpose
                .system
                .as_deref()
                .is_some_and(|system| system.trim().is_empty())
            {
                return Err(EmergencyError::EmptySystem { index });
            }
        }
        Ok(Self {
            purposes: purposes.iter().cloned().collect(),
        })
    }

    /// Whether the set declares no purpose, so no access is marked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.purposes.is_empty()
    }

    /// The declared purposes, in order.
    pub fn purposes(&self) -> impl Iterator<Item = &Purpose> {
        self.purposes.iter()
    }

    /// The mark of an access whose accessor declared `declared`: the
    /// declared purposes this set holds, or `None` when it holds none of
    /// them.
    #[must_use]
    pub fn mark(&self, declared: &[Purpose]) -> Option<Emergency> {
        let marking: BTreeSet<&Purpose> = declared
            .iter()
            .filter(|purpose| self.purposes.contains(*purpose))
            .collect();
        (!marking.is_empty()).then(|| Emergency {
            purposes: marking.into_iter().cloned().collect(),
        })
    }
}

/// The mark of an emergency access: the purposes of use the accessor
/// declared that the deployment declares as asserting one (Art 11(5)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emergency {
    purposes: Vec<Purpose>,
}

impl Emergency {
    /// The purposes that marked the access, in order, each once.
    #[must_use]
    pub fn purposes(&self) -> &[Purpose] {
        &self.purposes
    }
}

/// Why a set of emergency purposes cannot be declared.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmergencyError {
    /// A purpose has an empty code.
    #[error("emergency purpose {index} has an empty code")]
    EmptyCode {
        /// The position of the purpose, from zero.
        index: usize,
    },
    /// A purpose names an empty system.
    #[error("emergency purpose {index} names an empty system")]
    EmptySystem {
        /// The position of the purpose, from zero.
        index: usize,
    },
}
