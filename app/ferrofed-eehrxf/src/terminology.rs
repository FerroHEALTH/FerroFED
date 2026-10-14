// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR terminology server the interoperability component asks, over
//! HTTP.
//!
//! [`FhirTerminology`] answers the component's terminology seam
//! ([`TerminologyServer`]) by posting each operation's `Parameters` to
//! `{base}/ConceptMap/$translate`, `{base}/CodeSystem/$lookup` or
//! `{base}/ValueSet/$validate-code`, the type-level invocation of FHIR R4
//! (<https://hl7.org/fhir/R4/operations.html>), and reading the answer as
//! that operation's `Parameters`. The request and the answer are the typed
//! operation contracts of `fhir-types`. The HTTP client is the one every
//! FHIR service of the gateway is reached with, with its credential and TLS
//! material. A status other than `200 OK`, a time-out, a transport failure
//! and an answer that does not read are each a [`TerminologyError`] naming
//! the operation, so no failed call is taken as an answer.

use std::time::Duration;

use async_trait::async_trait;
use eehrxf::terminology::Operation;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::TerminologyServer;
use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::fhir::ClientError;
use ferrofed_identity::fhir::Tls;
use ferrofed_identity::fhir::http_client;
use fhir_types::codec::Json;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupRequest;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupResponse;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use fhir_types::r4::operations::value_set_validate_code::ValueSetValidateCodeRequest;
use fhir_types::r4::operations::value_set_validate_code::ValueSetValidateCodeResponse;
use fhir_types::r4::parameters::Parameters;
use http::StatusCode;
use http::header::ACCEPT;
use http::header::CONTENT_TYPE;
use url::Url;

/// The media type of FHIR JSON (<https://hl7.org/fhir/R4/json.html>).
const FHIR_JSON: &str = "application/fhir+json";

// TODO(#925): reach the server through FerroBRIDGE's ferrobridge-term once it is published.
/// A FHIR R4 terminology server reached over HTTP.
#[derive(Debug, Clone)]
pub struct FhirTerminology {
    base: Url,
    client: reqwest::Client,
    timeout: Duration,
}

/// Why the client of a terminology server cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FhirTerminologyError {
    /// The base is no absolute URL.
    #[error("the terminology server's base is no absolute URL")]
    Base(#[source] url::ParseError),
    /// The HTTP client cannot be built.
    #[error("the HTTP client of the terminology server cannot be built")]
    Client(#[source] ClientError),
}

impl FhirTerminology {
    /// Returns the client of the server at `base`, authenticated with
    /// `auth`, over the TLS material `tls`, each call bounded by `timeout`.
    ///
    /// # Errors
    ///
    /// [`FhirTerminologyError::Base`] for a base that does not parse, and
    /// [`FhirTerminologyError::Client`] for a client that cannot be built.
    pub fn new(
        base: &str,
        auth: &Authentication,
        tls: &Tls,
        timeout: Duration,
    ) -> Result<Self, FhirTerminologyError> {
        // NOTE: RFC 3986 §5.2.3: a base without a trailing slash drops its last segment
        // on resolution, so the base is held as a directory.
        let directory = if base.ends_with('/') {
            base.to_owned()
        } else {
            format!("{base}/")
        };
        let base = Url::parse(&directory).map_err(FhirTerminologyError::Base)?;
        let client = http_client(auth, tls).map_err(FhirTerminologyError::Client)?;
        Ok(Self {
            base,
            client,
            timeout,
        })
    }

    /// Posts `parameters` to `operation` and reads the answer's
    /// `Parameters`.
    async fn post(
        &self,
        operation: Operation,
        parameters: &Parameters,
    ) -> Result<Parameters, TerminologyError> {
        let answer = |source: Box<dyn std::error::Error + Send + Sync>| TerminologyError::Answer {
            operation,
            source,
        };
        let url =
            self.base
                .join(operation.path())
                .map_err(|source| TerminologyError::Transport {
                    operation,
                    source: Box::new(source),
                })?;
        let body =
            serde_json::to_vec(parameters).map_err(|source| TerminologyError::Transport {
                operation,
                source: Box::new(source),
            })?;
        let sent = self
            .client
            .post(url)
            .header(CONTENT_TYPE, FHIR_JSON)
            .header(ACCEPT, FHIR_JSON)
            .timeout(self.timeout)
            .body(body)
            .send()
            .await;
        let response = sent.map_err(|source| failed(operation, source))?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(TerminologyError::Status { operation, status });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|source| failed(operation, source))?;
        let Value::Object(object) =
            serde_json::from_slice::<Value>(&bytes).map_err(|source| answer(Box::new(source)))?
        else {
            return Err(answer(Box::new(NotParameters)));
        };
        if object.get("resourceType").and_then(Value::as_str) != Some("Parameters") {
            return Err(answer(Box::new(NotParameters)));
        }
        Parameters::from_json(&object, &mut Path::root("Parameters"))
            .map_err(|source| answer(Box::new(source)))
    }
}

#[async_trait]
impl TerminologyServer for FhirTerminology {
    async fn translate(
        &self,
        request: ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        let operation = Operation::Translate;
        let answer = self.post(operation, &request.into_parameters()).await?;
        ConceptMapTranslateResponse::from_parameters(&answer).map_err(|source| {
            TerminologyError::Answer {
                operation,
                source: Box::new(source),
            }
        })
    }

    async fn lookup(
        &self,
        request: CodeSystemLookupRequest,
    ) -> Result<CodeSystemLookupResponse, TerminologyError> {
        let operation = Operation::Lookup;
        let answer = self.post(operation, &request.into_parameters()).await?;
        CodeSystemLookupResponse::from_parameters(&answer).map_err(|source| {
            TerminologyError::Answer {
                operation,
                source: Box::new(source),
            }
        })
    }

    async fn validate_code(
        &self,
        request: ValueSetValidateCodeRequest,
    ) -> Result<ValueSetValidateCodeResponse, TerminologyError> {
        let operation = Operation::ValidateCode;
        let answer = self.post(operation, &request.into_parameters()).await?;
        ValueSetValidateCodeResponse::from_parameters(&answer).map_err(|source| {
            TerminologyError::Answer {
                operation,
                source: Box::new(source),
            }
        })
    }
}

/// The refusal of a call that failed on the way: a time-out, or a
/// transport failure.
fn failed(operation: Operation, source: reqwest::Error) -> TerminologyError {
    if source.is_timeout() {
        TerminologyError::Timeout { operation }
    } else {
        TerminologyError::Transport {
            operation,
            source: Box::new(source),
        }
    }
}

/// An answer that is no `Parameters` resource.
#[derive(Debug, thiserror::Error)]
#[error("the answer is no Parameters resource")]
struct NotParameters;
