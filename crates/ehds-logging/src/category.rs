// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The categories of personal electronic health data an access record names
//! (Regulation (EU) 2025/327 Annex II 3.2(c)).
//!
//! Art 14(1) fixes six priority categories, points (a) to (f), and lets a
//! Member State add categories in its national law (Art 14(1) third
//! subparagraph). A [`Category`] is one of the six, or a national category a
//! deployment declares. Each is written as a coded value, the way a FHIR R4
//! `Coding` is: a [`system`](Category::system), the
//! [`version`](Category::version) of that system when known, and a
//! [`code`](Category::code) in it.
//!
//! The six priority categories carry the codes of HL7 Europe's
//! `EEHRxFDocumentPriorityCategoryCS` ([`PRIORITY_SYSTEM`], version
//! [`PRIORITY_VERSION`], `hl7.fhir.eu.health-data-api` 1.0.0-ballot), whose
//! displays are the Art 14(1) terms and whose definitions are the Annex I
//! text. That code system is case-sensitive, so a code is compared exactly. A
//! national category carries the system the Member State, or the deployment
//! in its place, names, and the national code as it stands.
//!
//! Where a record or a configuration holds a category as one string, it is
//! the FHIR R4 search token form, `<system>|<code>`, which
//! [`Category::token`] writes and [`Reference`] reads.

use std::fmt;
use std::str::FromStr;

/// The canonical URL of the code system of the six priority categories.
///
/// It is HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`
/// (`hl7.fhir.eu.health-data-api` 1.0.0-ballot,
/// `CodeSystem/eehrxf-document-priority-category-cs`).
pub const PRIORITY_SYSTEM: &str =
    "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";

/// The version of [`PRIORITY_SYSTEM`] the priority codes are read from.
pub const PRIORITY_VERSION: &str = "1.0.0-ballot";

/// A category of personal electronic health data (Art 14(1)).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Category {
    /// Art 14(1)(a), patient summaries: `Patient-Summaries`.
    PatientSummary,
    /// Art 14(1)(b), electronic prescriptions: `Electronic-Prescriptions`.
    ElectronicPrescription,
    /// Art 14(1)(c), electronic dispensations: `Electronic-Dispensations`.
    ElectronicDispensation,
    /// Art 14(1)(d), medical imaging studies and related imaging reports:
    /// `Medical-Imaging`.
    MedicalImaging,
    /// Art 14(1)(e), medical test results, including laboratory and other
    /// diagnostic results and related reports: `Laboratory-Reports`.
    MedicalTestResult,
    /// Art 14(1)(f), discharge reports: `Discharge-Reports`.
    DischargeReport,
    /// A category national law adds (Art 14(1) third subparagraph), by the
    /// system and code the deployment declares for it.
    National(NationalCategory),
}

impl Category {
    /// The six priority categories of Art 14(1), points (a) to (f), in order.
    pub const PRIORITY: [Self; 6] = [
        Self::PatientSummary,
        Self::ElectronicPrescription,
        Self::ElectronicDispensation,
        Self::MedicalImaging,
        Self::MedicalTestResult,
        Self::DischargeReport,
    ];

    /// The code system the category's code is defined in: [`PRIORITY_SYSTEM`]
    /// for a priority category, the declared system for a national one.
    #[must_use]
    pub fn system(&self) -> &str {
        match self {
            Self::National(national) => national.system(),
            _ => PRIORITY_SYSTEM,
        }
    }

    /// The version of the code system, when it is known:
    /// [`PRIORITY_VERSION`] for a priority category, `None` for a national
    /// one.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::National(_) => None,
            _ => Some(PRIORITY_VERSION),
        }
    }

    /// The code of the category in its [`system`](Category::system).
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::PatientSummary => "Patient-Summaries",
            Self::ElectronicPrescription => "Electronic-Prescriptions",
            Self::ElectronicDispensation => "Electronic-Dispensations",
            Self::MedicalImaging => "Medical-Imaging",
            Self::MedicalTestResult => "Laboratory-Reports",
            Self::DischargeReport => "Discharge-Reports",
            Self::National(national) => national.code(),
        }
    }

    /// The category as one string, `<system>|<code>`, the FHIR R4 search
    /// token form.
    #[must_use]
    pub fn token(&self) -> String {
        format!("{}|{}", self.system(), self.code())
    }

    /// The priority category `code` names in [`PRIORITY_SYSTEM`], compared
    /// exactly, or `None` for any other code.
    #[must_use]
    pub fn priority(code: &str) -> Option<Self> {
        Self::PRIORITY
            .into_iter()
            .find(|category| category.code() == code)
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}", self.system(), self.code())
    }
}

/// A category national law adds: the system that defines it and its code.
///
/// It is written `<system>|<code>`. The system is an absolute URI that holds
/// no `|` and is not [`PRIORITY_SYSTEM`]; the code is a FHIR R4 `code`: at
/// least one character, no leading or trailing whitespace, and no whitespace
/// other than single spaces (FHIR R4 Datatypes, `code`), and no character
/// below U+0020 (FHIR R4 Datatypes, `string`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NationalCategory {
    system: String,
    code: String,
}

impl NationalCategory {
    /// The national category `code` in `system`.
    ///
    /// # Errors
    ///
    /// [`CodeError::System`] for a system that is no absolute URI or holds a
    /// `|`, [`CodeError::Priority`] for [`PRIORITY_SYSTEM`], and
    /// [`CodeError::Code`] for a code that is no FHIR `code`.
    pub fn new(system: &str, code: &str) -> Result<Self, CodeError> {
        if system == PRIORITY_SYSTEM {
            return Err(CodeError::Priority);
        }
        if !is_absolute_uri(system) {
            return Err(CodeError::System);
        }
        if !is_fhir_code(code) {
            return Err(CodeError::Code);
        }
        Ok(Self {
            system: system.to_owned(),
            code: code.to_owned(),
        })
    }

    /// The system.
    #[must_use]
    pub fn system(&self) -> &str {
        &self.system
    }

    /// The code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

impl FromStr for NationalCategory {
    type Err = CodeError;

    /// Reads `<system>|<code>`.
    fn from_str(token: &str) -> Result<Self, Self::Err> {
        let (system, code) = token.split_once('|').ok_or(CodeError::NoSystem)?;
        Self::new(system, code)
    }
}

/// Why a national category is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodeError {
    /// The category names no system: it is not written `<system>|<code>`.
    #[error("a national category is written <system>|<code>")]
    NoSystem,
    /// The system is no absolute URI, or it holds a `|`.
    #[error("the system of a national category is an absolute URI without `|`")]
    System,
    /// The system is the code system of the priority categories.
    #[error("a national category names a system of its own, never {PRIORITY_SYSTEM}")]
    Priority,
    /// The code is empty, has leading or trailing whitespace, holds
    /// whitespace other than single spaces, or holds a character below U+0020.
    #[error(
        "a national category code is a FHIR code: no leading, trailing or repeated whitespace, and no character below U+0020"
    )]
    Code,
}

/// How a configuration names a category: a bare priority code, or
/// `<system>|<code>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference<'a> {
    /// A bare code, which names a priority category.
    Bare(&'a str),
    /// A code in a system.
    Coded {
        /// The system.
        system: &'a str,
        /// The code.
        code: &'a str,
    },
}

impl<'a> Reference<'a> {
    /// Reads `text`: `<system>|<code>` when it holds a `|`, a bare code
    /// otherwise.
    #[must_use]
    pub fn read(text: &'a str) -> Self {
        match text.split_once('|') {
            Some((system, code)) => Self::Coded { system, code },
            None => Self::Bare(text),
        }
    }

    /// The priority category this names, or `None` when it names another.
    #[must_use]
    pub fn priority(self) -> Option<Category> {
        match self {
            Self::Bare(code)
            | Self::Coded {
                system: PRIORITY_SYSTEM,
                code,
            } => Category::priority(code),
            Self::Coded { .. } => None,
        }
    }

    /// Whether this names `national`.
    #[must_use]
    pub fn names(self, national: &NationalCategory) -> bool {
        matches!(self, Self::Coded { system, code } if system == national.system() && code == national.code())
    }
}

/// Whether `system` is an absolute URI with no `|` and no whitespace: a
/// scheme of RFC 3986 §3.1 followed by `:` and at least one character.
fn is_absolute_uri(system: &str) -> bool {
    let Some((scheme, rest)) = system.split_once(':') else {
        return false;
    };
    let mut letters = scheme.chars();
    let scheme_formed = letters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    scheme_formed
        && !rest.is_empty()
        && !system
            .chars()
            .any(|c| c == '|' || c.is_whitespace() || c.is_control())
}

/// Whether `code` is a FHIR R4 `code` (`[^\s]+(\s[^\s]+)*`, the whitespace a
/// single space) with no character below U+0020.
fn is_fhir_code(code: &str) -> bool {
    !code.is_empty()
        && !code.starts_with(' ')
        && !code.ends_with(' ')
        && !code.contains("  ")
        && !code
            .chars()
            .any(|c| c < ' ' || (c.is_whitespace() && c != ' '))
}
