// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology rules of the exchange format (feature `fhir-r4`).
//!
//! Regulation (EU) 2025/327 Art 2(2)(f) defines interoperability as an
//! exchange "without changing the content of the data", so a code travels
//! as it was recorded, and a coding is added beside it only where it means
//! the same thing:
//!
//! - `transcode` (feature `openehr`): an openEHR `DV_TEXT` or
//!   `DV_CODED_TEXT` turned into FHIR codings and narrative. A code in a
//!   preferred system passes as recorded; an RM `TERM_MAPPING` whose `match`
//!   is `'='` adds a coding after the original; a broader, narrower or
//!   unknown match adds no coding and is written into the narrative; a code
//!   no preferred system carries follows Art 10(7) of the eHealth Network
//!   Guidelines on Patient Summary;
//! - [`ehn`]: the preferred code systems of that guideline's §4, the
//!   systems the rules prefer by default;
//! - `local` (feature `openehr`): the archetype-local terms of an
//!   operational template, by which a `local` code is rendered;
//! - `system` (feature `openehr`): the FHIR `Coding.system` an openEHR
//!   terminology id is written as, and none for an id with no known system;
//! - `withheld` (feature `openehr`): a term mapping a FHIRconnect run wrote
//!   as no coding, read back so the narrative carries it;
//! - [`expansion`]: the value set expansions a FHIR package carries;
//! - [`bindings`]: the `required` bindings of a profile checked against
//!   those expansions, every binding with no expansion named as not
//!   evaluated;
//! - [`Terminology`]: the optional FHIR terminology server, asked for
//!   `$translate`, `$lookup` and `$validate-code`
//!   (<https://hl7.org/fhir/R4/terminology-service.html>), which fails
//!   closed when none is configured;
//! - [`translation`]: each concept of a mapped resource that is in no
//!   preferred system, translated through that server, and left as
//!   recorded under Art 10(7) when none is configured.
//!
//! The crate reaches no server itself: [`TerminologyServer`] is the seam the
//! system that reaches one implements.

pub mod bindings;
pub mod ehn;
pub mod expansion;
#[cfg(feature = "openehr")]
pub mod local;
#[cfg(feature = "openehr")]
pub mod system;
#[cfg(feature = "openehr")]
pub mod transcode;
pub mod translation;
pub(crate) mod walk;
#[cfg(feature = "openehr")]
pub mod withheld;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use http::StatusCode;

/// The `ConceptMapEquivalence` codes a translation is taken for: the two
/// that state the target means the same as the source
/// (<https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>).
// NOTE: FHIR R4 ConceptMapEquivalence: `equivalent` and `equal` alone mean the same thing,
// and a FHIR coding asserts that, so a wider, narrower or inexact match is never one.
pub const ACCEPTED_EQUIVALENCES: [&str; 2] = ["equivalent", "equal"];

/// One match of a `$translate` answer: its `equivalence` and its `concept`,
/// each as the server sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The `ConceptMapEquivalence` code, when the server sent one.
    pub equivalence: Option<String>,
    /// The target concept, when the server sent one.
    pub concept: Option<Coding>,
}

/// A FHIR terminology server, as the system that reaches it implements it.
///
/// Each call asks one operation exactly what the component needs of it; a
/// refusal, a time-out or an answer that does not read is a
/// [`TerminologyError`], never an empty answer.
#[async_trait]
pub trait TerminologyServer: fmt::Debug + Send + Sync {
    /// Asks `ConceptMap/$translate` what the code `code` of the system
    /// `system` maps to in the system `target`
    /// (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>),
    /// answering every match of a `true` result with its equivalence, and
    /// none for a `false` one.
    ///
    /// # Errors
    ///
    /// A [`TerminologyError`] for a call that gives no answer.
    async fn translate(
        &self,
        system: &str,
        code: &str,
        target: &str,
    ) -> Result<Vec<Match>, TerminologyError>;

    /// Asks `CodeSystem/$lookup` for the display of the code `code` of the
    /// system `system`, whose `display` is `1..1`
    /// (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>).
    ///
    /// # Errors
    ///
    /// A [`TerminologyError`] for a call that gives no answer, and
    /// [`TerminologyError::NotFound`] for a code the server does not know.
    async fn lookup(&self, system: &str, code: &str) -> Result<String, TerminologyError>;

    /// Asks `ValueSet/$validate-code` whether `concept` is in the value set
    /// `value_set`, answering its `result`
    /// (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).
    ///
    /// # Errors
    ///
    /// A [`TerminologyError`] for a call that gives no answer, an answer
    /// with no `result` value included.
    async fn validate_code(
        &self,
        value_set: &str,
        concept: &Candidate,
    ) -> Result<bool, TerminologyError>;
}

/// A terminology operation, named in every refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    /// `ConceptMap/$translate`.
    Translate,
    /// `CodeSystem/$lookup`.
    Lookup,
    /// `ValueSet/$validate-code`.
    ValidateCode,
}

impl Operation {
    /// Returns the operation's path below the server's base, such as
    /// `ConceptMap/$translate`.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Translate => "ConceptMap/$translate",
            Self::Lookup => "CodeSystem/$lookup",
            Self::ValidateCode => "ValueSet/$validate-code",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.path())
    }
}

/// Why a terminology operation gave no answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TerminologyError {
    /// No terminology server is configured, so the operation cannot be
    /// asked.
    #[error("no terminology server is configured, so {operation} cannot be asked")]
    Unconfigured {
        /// The operation.
        operation: Operation,
    },
    /// The server answered with a status other than `200 OK`.
    #[error("the terminology server answered {operation} with {status}")]
    Status {
        /// The operation.
        operation: Operation,
        /// The status it answered with.
        status: StatusCode,
    },
    /// The server gave no answer in time.
    #[error("the terminology server did not answer {operation} in time")]
    Timeout {
        /// The operation.
        operation: Operation,
    },
    /// The server could not be reached.
    #[error("the terminology server could not be reached for {operation}")]
    Transport {
        /// The operation.
        operation: Operation,
        /// The transport failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The server does not know the code or its code system, which it
    /// states as a refusal of the operation.
    #[error("the terminology server does not know the code asked of {operation}")]
    NotFound {
        /// The operation.
        operation: Operation,
    },
    /// The answer does not read as the operation's `Parameters`.
    #[error("the terminology server's answer to {operation} does not read as its Parameters")]
    Answer {
        /// The operation.
        operation: Operation,
        /// The read failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// The terminology server a deployment configured, or none.
///
/// Every operation asked of [`Terminology::none`] fails closed with
/// [`TerminologyError::Unconfigured`].
#[derive(Debug, Clone, Default)]
pub struct Terminology {
    server: Option<Arc<dyn TerminologyServer>>,
}

impl Terminology {
    /// Returns the terminology of a deployment that configured no server.
    #[must_use]
    pub const fn none() -> Self {
        Self { server: None }
    }

    /// Returns the terminology answered by `server`.
    #[must_use]
    pub fn server(server: Arc<dyn TerminologyServer>) -> Self {
        Self {
            server: Some(server),
        }
    }

    /// Returns whether a server is configured.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.server.is_some()
    }

    /// Returns the server, or the refusal of `operation` when none is
    /// configured.
    fn ask(&self, operation: Operation) -> Result<&dyn TerminologyServer, TerminologyError> {
        self.server
            .as_deref()
            .ok_or(TerminologyError::Unconfigured { operation })
    }

    /// Translates the code `code` of the system `system` into the system
    /// `target`, answering only the codings the server matches as
    /// `equivalent` or `equal` ([`ACCEPTED_EQUIVALENCES`]).
    ///
    /// # Errors
    ///
    /// [`TerminologyError::Unconfigured`] with no server, and the server's
    /// [`TerminologyError`] for a call that gives no answer.
    pub async fn translate(
        &self,
        system: &str,
        code: &str,
        target: &str,
    ) -> Result<Vec<Coding>, TerminologyError> {
        let server = self.ask(Operation::Translate)?;
        let matches = server.translate(system, code, target).await?;
        Ok(matches
            .into_iter()
            .filter(|found| {
                found
                    .equivalence
                    .as_deref()
                    .is_some_and(|code| ACCEPTED_EQUIVALENCES.contains(&code))
            })
            .filter_map(|found| found.concept)
            .collect())
    }

    /// Looks up the preferred display of the code `code` of the system
    /// `system` with `$lookup`, whose `display` is `1..1`.
    ///
    /// # Errors
    ///
    /// [`TerminologyError::Unconfigured`] with no server, the server's
    /// [`TerminologyError`] for a call that gives no answer, and
    /// [`TerminologyError::Answer`] for an answer with an empty display.
    pub async fn display(&self, system: &str, code: &str) -> Result<String, TerminologyError> {
        let server = self.ask(Operation::Lookup)?;
        let display = server.lookup(system, code).await?;
        Some(display)
            .filter(|display| !display.is_empty())
            .ok_or_else(|| TerminologyError::Answer {
                operation: Operation::Lookup,
                source: Box::new(EmptyDisplay),
            })
    }

    /// Asks with `$validate-code` whether `concept` is in the value set
    /// `value_set`.
    ///
    /// # Errors
    ///
    /// [`TerminologyError::Unconfigured`] with no server, and the server's
    /// [`TerminologyError`] for a call that gives no answer, an answer with
    /// no `result` value included.
    pub async fn validate(
        &self,
        value_set: &str,
        concept: &Candidate,
    ) -> Result<bool, TerminologyError> {
        let server = self.ask(Operation::ValidateCode)?;
        server.validate_code(value_set, concept).await
    }
}

/// A concept a value set is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Candidate {
    /// A `code` primitive, whose system the element's binding implies.
    Code {
        /// The code.
        code: String,
        /// The `$validate-code` `context`: the profile's URL and the
        /// element, `{profile}#{element}`.
        context: String,
    },
    /// A `Coding`.
    Coding(Box<Coding>),
    /// A `CodeableConcept`.
    Concept(Box<CodeableConcept>),
}

/// Returns the codings of `response` matched as `equivalent` or `equal`, in
/// the server's order.
#[must_use]
pub fn accepted(response: &ConceptMapTranslateResponse) -> Vec<Coding> {
    if response.result.value != Some(true) {
        return Vec::new();
    }
    response
        .r#match
        .iter()
        .filter(|found| {
            found
                .equivalence
                .as_ref()
                .and_then(|code| code.value.as_deref())
                .is_some_and(|code| ACCEPTED_EQUIVALENCES.contains(&code))
        })
        .filter_map(|found| found.concept.clone())
        .collect()
}

/// A `$lookup` answer whose display is empty.
#[derive(Debug, thiserror::Error)]
#[error("the answer carries no display")]
struct EmptyDisplay;
