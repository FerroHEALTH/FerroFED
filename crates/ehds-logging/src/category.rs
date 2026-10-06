// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The categories of personal electronic health data an access record names
//! (Regulation (EU) 2025/327 Annex II 3.2(c)).
//!
//! Art 14(1) fixes six priority categories, points (a) to (f), and lets a
//! Member State add categories in its national law (Art 14(1) third
//! subparagraph). A [`Category`] is one of the six, or a national category a
//! deployment declares by its code. No adopted act fixes how a category is
//! written in a log, so each is written by the code [`Category::code`] gives
//! (no specification governs the codes: our own design).

use std::fmt;
use std::str::FromStr;

/// A category of personal electronic health data (Art 14(1)).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Category {
    /// Art 14(1)(a), patient summaries: `patient-summary`.
    PatientSummary,
    /// Art 14(1)(b), electronic prescriptions: `electronic-prescription`.
    ElectronicPrescription,
    /// Art 14(1)(c), electronic dispensations: `electronic-dispensation`.
    ElectronicDispensation,
    /// Art 14(1)(d), medical imaging studies and related imaging reports:
    /// `medical-imaging`.
    MedicalImaging,
    /// Art 14(1)(e), medical test results, including laboratory and other
    /// diagnostic results and related reports: `medical-test-result`.
    MedicalTestResult,
    /// Art 14(1)(f), discharge reports: `discharge-report`.
    DischargeReport,
    /// A category national law adds (Art 14(1) third subparagraph), by the
    /// code the deployment declares for it.
    National(NationalCode),
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

    /// The code the category is written as.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::PatientSummary => "patient-summary",
            Self::ElectronicPrescription => "electronic-prescription",
            Self::ElectronicDispensation => "electronic-dispensation",
            Self::MedicalImaging => "medical-imaging",
            Self::MedicalTestResult => "medical-test-result",
            Self::DischargeReport => "discharge-report",
            Self::National(code) => code.as_str(),
        }
    }

    /// The priority category `code` names, or `None` for any other code.
    #[must_use]
    pub fn priority(code: &str) -> Option<Self> {
        Self::PRIORITY
            .into_iter()
            .find(|category| category.code() == code)
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// The code of a national category: lower-case ASCII letters, digits and
/// `-`, at most 64 characters, and none of the priority categories' codes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NationalCode(String);

impl NationalCode {
    /// The code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a code names no category.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodeError {
    /// The code is empty, longer than 64 characters, or holds a character
    /// other than a lower-case ASCII letter, a digit or `-`.
    #[error("a category code is 1 to 64 lower-case ASCII letters, digits and `-`")]
    Malformed,
    /// A national code repeats a priority category's code.
    #[error("the national category code repeats the priority category `{0}`")]
    Priority(Category),
}

impl FromStr for NationalCode {
    type Err = CodeError;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        let formed = !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !formed {
            return Err(CodeError::Malformed);
        }
        match Category::priority(code) {
            Some(priority) => Err(CodeError::Priority(priority)),
            None => Ok(Self(code.to_owned())),
        }
    }
}
