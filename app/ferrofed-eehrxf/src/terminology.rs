// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR terminology server the interoperability component asks, over
//! HTTP, through FerroBRIDGE's terminology client.
//!
//! [`BridgeTerminology`] answers the component's terminology seam
//! ([`TerminologyServer`]) with `ferrobridge_term::client::Client` in FHIR
//! R4: `ConceptMap/$translate`, `CodeSystem/$lookup` and
//! `ValueSet/$validate-code`, each posted as its `Parameters`
//! (<https://hl7.org/fhir/R4/operations.html>). The HTTP client is the one
//! every FHIR service of the gateway is reached with, with its credential
//! and its TLS material, mutual TLS and private trust roots included. A
//! refusal, a time-out, a transport failure and an answer that does not
//! read are each a [`TerminologyError`] naming the operation, so no failed
//! call is taken as an answer.

use std::time::Duration;

use async_trait::async_trait;
use eehrxf::terminology::Candidate;
use eehrxf::terminology::Match;
use eehrxf::terminology::Operation;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::TerminologyServer;
use ferrobridge_term::client::Client;
use ferrobridge_term::concept::Concept;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use ferrobridge_term::error::Error as TermError;
use ferrobridge_term::outcome::{
    Coded, LookupOutcome, TranslateOutcome, Translation, ValidateOutcome, Validation,
};
use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::fhir::ClientError;
use ferrofed_identity::fhir::Tls;
use ferrofed_identity::fhir::http_client;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::{Code, String as FhirString, Uri};
use url::Url;

/// A FHIR R4 terminology server reached over HTTP.
#[derive(Debug, Clone)]
pub struct BridgeTerminology {
    client: Client,
}

impl BridgeTerminology {
    /// Returns the terminology client, which FerroBRIDGE's validator asks
    /// the bindings of a received document it cannot decide.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }
}

/// Why the client of a terminology server cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BridgeTerminologyError {
    /// The base is no absolute URL.
    #[error("the terminology server's base is no absolute URL")]
    Base(#[source] url::ParseError),
    /// The HTTP client cannot be built.
    #[error("the HTTP client of the terminology server cannot be built")]
    Http(#[source] ClientError),
    /// The terminology client refuses the base.
    #[error("the terminology client cannot be built")]
    Client(#[source] Box<TermError>),
}

impl BridgeTerminology {
    /// Returns the client of the server at `base`, authenticated with
    /// `auth`, over the TLS material `tls`, each call bounded by `timeout`.
    ///
    /// Each call is sent once: no specification governs a retry here, and
    /// the face that asks fails closed on the first refusal (our own
    /// design).
    ///
    /// # Errors
    ///
    /// [`BridgeTerminologyError::Base`] for a base that does not parse,
    /// [`BridgeTerminologyError::Http`] for an HTTP client that cannot be
    /// built, and [`BridgeTerminologyError::Client`] for a base that cannot
    /// carry a path.
    pub fn new(
        base: &str,
        auth: &Authentication,
        tls: &Tls,
        timeout: Duration,
    ) -> Result<Self, BridgeTerminologyError> {
        let base = Url::parse(base).map_err(BridgeTerminologyError::Base)?;
        let http = http_client(auth, tls).map_err(BridgeTerminologyError::Http)?;
        let config = Config::new(base, WireVersion::R4)
            .with_timeout(timeout)
            .with_retry(RetryPolicy {
                max_attempts: 1,
                ..RetryPolicy::default()
            });
        let client = Client::with_http(config, http)
            .map_err(|error| BridgeTerminologyError::Client(Box::new(error)))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl TerminologyServer for BridgeTerminology {
    async fn translate(
        &self,
        system: &str,
        code: &str,
        target: &str,
    ) -> Result<Vec<Match>, TerminologyError> {
        let translation = Translation::new(system, code).with_target_system(target);
        let outcome = self
            .client
            .translate(&translation)
            .await
            .map_err(|error| refusal(Operation::Translate, error))?;
        match outcome {
            TranslateOutcome::Translated { matches, .. } => Ok(matches
                .into_iter()
                .map(|found| Match {
                    equivalence: found
                        .equivalence
                        .as_ref()
                        .map(|equivalence| equivalence.as_str().to_owned()),
                    concept: found.concept.as_ref().map(coding),
                })
                .collect()),
            // NOTE: FHIR R4 `$translate`: a `result` of false states that no match
            // exists, so it yields no coding.
            TranslateOutcome::NoMatch { .. } => Ok(Vec::new()),
            _ => Err(unknown(Operation::Translate)),
        }
    }

    async fn lookup(&self, system: &str, code: &str) -> Result<String, TerminologyError> {
        let outcome = self
            .client
            .lookup(system, code, None)
            .await
            .map_err(|error| refusal(Operation::Lookup, error))?;
        match outcome {
            LookupOutcome::Found { display, .. } => Ok(display),
            LookupOutcome::NotFound { .. } => Err(TerminologyError::NotFound {
                operation: Operation::Lookup,
            }),
            _ => Err(unknown(Operation::Lookup)),
        }
    }

    async fn validate_code(
        &self,
        value_set: &str,
        concept: &Candidate,
    ) -> Result<bool, TerminologyError> {
        let coded = match concept {
            Candidate::Code { code, context } => Coded::InContext {
                context: context.clone(),
                code: code.clone(),
                display: None,
            },
            Candidate::Coding(coding) => Coded::Coding(concept_of(coding)),
            Candidate::Concept(concept) => Coded::CodeableConcept {
                coding: concept.coding.iter().map(concept_of).collect(),
                text: concept.text.as_ref().and_then(|text| text.value.clone()),
            },
            _ => return Err(unknown(Operation::ValidateCode)),
        };
        let validation = Validation {
            value_set_url: value_set.to_owned(),
            value_set_version: None,
            date: None,
            coded,
        };
        let outcome = self
            .client
            .validate(&validation)
            .await
            .map_err(|error| refusal(Operation::ValidateCode, error))?;
        match outcome {
            ValidateOutcome::Valid { .. } => Ok(true),
            ValidateOutcome::Invalid { .. } => Ok(false),
            _ => Err(unknown(Operation::ValidateCode)),
        }
    }
}

/// Returns the refusal of `operation` that `error` describes: the
/// server's status for an answer it refused, a time-out, an answer that
/// does not read, or a call that did not reach the server.
fn refusal(operation: Operation, error: TermError) -> TerminologyError {
    if matches!(error, TermError::Timeout { .. }) {
        return TerminologyError::Timeout { operation };
    }
    if matches!(error, TermError::Body { .. }) {
        return TerminologyError::Answer {
            operation,
            source: Box::new(error),
        };
    }
    match error.upstream_status() {
        Some(status) => TerminologyError::Status { operation, status },
        None => TerminologyError::Transport {
            operation,
            source: Box::new(error),
        },
    }
}

/// Returns the FHIR R4 `Coding` of a concept the server answered.
fn coding(concept: &Concept) -> Coding {
    Coding {
        system: concept.system.as_deref().map(Uri::from),
        version: concept.version.as_deref().map(FhirString::from),
        code: concept.code.as_deref().map(Code::from),
        display: concept.display.as_deref().map(FhirString::from),
        ..Coding::default()
    }
}

/// Returns the concept a FHIR R4 `Coding` names.
fn concept_of(coding: &Coding) -> Concept {
    Concept {
        system: coding
            .system
            .as_ref()
            .and_then(|system| system.value.clone()),
        version: coding
            .version
            .as_ref()
            .and_then(|version| version.value.clone()),
        code: coding.code.as_ref().and_then(|code| code.value.clone()),
        display: coding
            .display
            .as_ref()
            .and_then(|display| display.value.clone()),
    }
}

/// Returns the refusal of `operation` for a form this client does not
/// read: a candidate it cannot send, or an outcome it cannot take.
fn unknown(operation: Operation) -> TerminologyError {
    TerminologyError::Answer {
        operation,
        source: Box::new(UnknownForm),
    }
}

/// A form this client does not read.
#[derive(Debug, thiserror::Error)]
#[error("a form this terminology client does not read")]
struct UnknownForm;
