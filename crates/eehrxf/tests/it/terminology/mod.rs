// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology rules: the transcoding of an openEHR text item, the
//! `required` bindings held against vendored expansions, the displays of a
//! received document, and a terminology server that fails closed.
//!
//! Every code system and code here is synthetic, under `example.org`
//! (RFC 6761 §6.5).

mod bindings;
mod displays;
mod ehn;
#[cfg(feature = "openehr")]
mod local;
mod server;
#[cfg(feature = "openehr")]
mod system;
#[cfg(feature = "openehr")]
mod transcode;
mod translation;
#[cfg(feature = "openehr")]
mod version;
#[cfg(feature = "openehr")]
mod withheld;

use async_trait::async_trait;
use eehrxf::terminology::Candidate;
use eehrxf::terminology::Match;
use eehrxf::terminology::Operation;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::TerminologyServer;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Uri;
use http::StatusCode;

/// A preferred code system.
pub(crate) const PREFERRED: &str = "http://example.org/fhir/CodeSystem/preferred";

/// A code system no deployment prefers.
pub(crate) const LOCAL: &str = "http://example.org/fhir/CodeSystem/local";

/// What the scripted server answers every operation with.
#[derive(Debug, Clone)]
pub(crate) enum Script {
    /// `$translate` answers these matches, each an equivalence, a system
    /// and a code.
    Translate(Vec<(&'static str, &'static str, &'static str)>),
    /// `$lookup` answers this display.
    Display(&'static str),
    /// `$validate-code` answers this result.
    Valid(bool),
    /// Every operation is refused with this status.
    Refuse(StatusCode),
}

/// A terminology server that answers from a script.
#[derive(Debug, Clone)]
pub(crate) struct Scripted(pub(crate) Script);

impl Scripted {
    /// The refusal of `operation` the script gives, for an operation it
    /// does not answer.
    fn refusal(&self, operation: Operation) -> TerminologyError {
        let status = match self.0 {
            Script::Refuse(status) => status,
            _ => StatusCode::NOT_IMPLEMENTED,
        };
        TerminologyError::Status { operation, status }
    }
}

#[async_trait]
impl TerminologyServer for Scripted {
    async fn translate(
        &self,
        _system: &str,
        _code: &str,
        _target: &str,
    ) -> Result<Vec<Match>, TerminologyError> {
        let Script::Translate(matches) = &self.0 else {
            return Err(self.refusal(Operation::Translate));
        };
        Ok(matches
            .iter()
            .map(|(equivalence, system, code)| Match {
                equivalence: Some((*equivalence).to_owned()),
                concept: Some(coding(system, code)),
            })
            .collect())
    }

    async fn lookup(&self, _system: &str, _code: &str) -> Result<String, TerminologyError> {
        let Script::Display(display) = self.0 else {
            return Err(self.refusal(Operation::Lookup));
        };
        Ok(display.to_owned())
    }

    async fn validate_code(
        &self,
        _value_set: &str,
        _concept: &Candidate,
    ) -> Result<bool, TerminologyError> {
        let Script::Valid(result) = self.0 else {
            return Err(self.refusal(Operation::ValidateCode));
        };
        Ok(result)
    }
}

/// A coding of `system` and `code`.
pub(crate) fn coding(system: &str, code: &str) -> Coding {
    Coding {
        system: Some(Uri::from(system)),
        code: Some(Code::from(code)),
        ..Coding::default()
    }
}
