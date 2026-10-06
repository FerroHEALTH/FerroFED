// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 face of the European exchange format (Regulation (EU)
//! 2025/327 Annex II 2.1).
//!
//! It holds what a request for a patient summary names, and the
//! `CapabilityStatement` and the `OperationOutcome` the face answers with.
//!
//! The summary is asked for with the International Patient Summary 2.0.0
//! `$summary` operation on `Patient` at the type level
//! (`OperationDefinition-summary`): `identifier` names the patient as
//! `system|value` and `profile` the composition profile the document
//! follows. A request by the patient's logical id is not offered, since the
//! face holds no `Patient` resource, and a request that names the patient by
//! anything other than an identifier with its system, such as demographics,
//! is refused, so the patient is always the one an identity service has
//! confirmed (§5.2, N32).

use std::fmt;

use fhir_types::r4::capability_statement::{
    CapabilityStatement, CapabilityStatementRest, CapabilityStatementRestResource,
    CapabilityStatementRestResourceOperation, CapabilityStatementSoftware,
};
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameterValue};

/// The canonical URL of the International Patient Summary `$summary`
/// operation.
pub const SUMMARY_OPERATION: &str = "http://hl7.org/fhir/uv/ips/OperationDefinition/summary";

/// The canonical URL of the composition profile the document follows.
pub const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The version of the vendored HL7 Europe Patient Summary package.
pub const EPS_VERSION: &str = "1.0.0-ballot";

/// The FHIR JSON media type (FHIR R4 `http.html#mime-type`).
pub const FHIR_JSON: &str = "application/fhir+json";

/// The `_format` values that ask for FHIR JSON.
const JSON_FORMATS: [&str; 3] = ["json", "application/json", FHIR_JSON];

/// The patient a summary request names.
#[derive(Clone, PartialEq, Eq)]
pub struct SummaryRequest {
    system: String,
    value: String,
}

impl fmt::Debug for SummaryRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SummaryRequest").finish_non_exhaustive()
    }
}

/// Why a summary request is refused, naming no value it carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The request names no `identifier`.
    #[error("Patient/$summary names the patient by identifier=system|value")]
    NoIdentifier,
    /// The `identifier` is given more than once.
    #[error("Patient/$summary takes one identifier")]
    Repeated,
    /// The `identifier` has no system or no value.
    #[error("the identifier names its system and its value as system|value")]
    Unqualified,
    /// The `profile` names a profile other than the EPS composition.
    #[error("the summary follows {EPS_COMPOSITION} alone")]
    Profile,
    /// The request carries a parameter the operation does not take, such
    /// as a demographic one.
    #[error("Patient/$summary takes identifier, profile and _format alone")]
    Parameter,
    /// The `_format` asks for something other than FHIR JSON.
    #[error("the face answers application/fhir+json alone")]
    Format,
    /// The body is no FHIR `Parameters` resource.
    #[error("the body is no FHIR Parameters resource")]
    Body,
}

impl SummaryRequest {
    /// Returns the identifier system.
    #[must_use]
    pub fn system(&self) -> &str {
        &self.system
    }

    /// Returns the identifier value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Reads the request from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`Refusal`] the request is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, Refusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        Self::from_pairs(pairs.map(|(name, value)| (name.into_owned(), value.into_owned())))
    }

    /// Reads the request from `body`, a FHIR `Parameters` resource in JSON,
    /// the body of a `POST`.
    ///
    /// # Errors
    ///
    /// [`Refusal::Body`] for a body that is no `Parameters` resource, and
    /// the [`Refusal`] the request is otherwise answered with.
    pub fn from_parameters(body: &[u8]) -> Result<Self, Refusal> {
        let parameters: Parameters =
            serde_json::from_slice(body).map_err(|_quoted| Refusal::Body)?;
        let mut pairs = Vec::new();
        for parameter in parameters.parameter {
            let name = parameter.name.value.unwrap_or_default();
            let value = match parameter.value {
                Some(ParametersParameterValue::String(text)) => text.value,
                Some(ParametersParameterValue::Canonical(text)) => text.value,
                Some(ParametersParameterValue::Uri(text)) => text.value,
                _ => None,
            };
            let value = value.ok_or(Refusal::Parameter)?;
            pairs.push((name, value));
        }
        Self::from_pairs(pairs.into_iter())
    }

    /// Reads the request from its named parameters.
    fn from_pairs(pairs: impl Iterator<Item = (String, String)>) -> Result<Self, Refusal> {
        let mut identifier = None;
        for (name, value) in pairs {
            match name.as_str() {
                "identifier" => {
                    if identifier.replace(value).is_some() {
                        return Err(Refusal::Repeated);
                    }
                }
                "profile" => {
                    let (url, version) = value
                        .split_once('|')
                        .map_or((value.as_str(), None), |(url, version)| {
                            (url, Some(version))
                        });
                    if url != EPS_COMPOSITION || version.is_some_and(|held| held != EPS_VERSION) {
                        return Err(Refusal::Profile);
                    }
                }
                "_format" => {
                    if !JSON_FORMATS.contains(&value.as_str()) {
                        return Err(Refusal::Format);
                    }
                }
                _ => return Err(Refusal::Parameter),
            }
        }
        let identifier = identifier.ok_or(Refusal::NoIdentifier)?;
        let (system, value) = identifier.split_once('|').ok_or(Refusal::Unqualified)?;
        if system.is_empty() || value.is_empty() {
            return Err(Refusal::Unqualified);
        }
        Ok(Self {
            system: system.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// Returns the face's `CapabilityStatement`, written at `date`, for the
/// product `product` at `version`: a server that serves `$summary` on
/// `Patient` in FHIR JSON.
#[must_use]
pub fn capability(product: &str, version: &str, date: &str) -> CapabilityStatement {
    CapabilityStatement {
        status: "active".into(),
        date: date.into(),
        kind: "instance".into(),
        software: Some(CapabilityStatementSoftware {
            name: product.into(),
            version: Some(version.into()),
            ..CapabilityStatementSoftware::default()
        }),
        fhir_version: "4.0.1".into(),
        format: vec![FHIR_JSON.into()],
        rest: vec![CapabilityStatementRest {
            mode: "server".into(),
            resource: vec![CapabilityStatementRestResource {
                r#type: "Patient".into(),
                operation: vec![CapabilityStatementRestResourceOperation {
                    name: "summary".into(),
                    definition: SUMMARY_OPERATION.into(),
                    ..CapabilityStatementRestResourceOperation::default()
                }],
                ..CapabilityStatementRestResource::default()
            }],
            ..CapabilityStatementRest::default()
        }],
        ..CapabilityStatement::default()
    }
}

/// Returns the `OperationOutcome` of one error `issue`, of the FHIR R4
/// `issue-type` `code`, with the text `diagnostics`.
#[must_use]
pub fn outcome(code: &str, diagnostics: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "error".into(),
            code: code.into(),
            diagnostics: Some(diagnostics.into()),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Refusal, SummaryRequest};

    #[test]
    fn an_identifier_with_its_system_names_the_patient() {
        let read = SummaryRequest::from_query(Some(
            "identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&_format=json",
        ))
        .unwrap();
        assert_eq!(read.system(), "urn:oid:2.999.1");
        assert_eq!(read.value(), "synthetic-1");
    }

    #[test]
    fn every_other_form_is_refused() {
        for (query, refusal) in [
            ("", Refusal::NoIdentifier),
            ("identifier=synthetic-1", Refusal::Unqualified),
            ("identifier=%7Csynthetic-1", Refusal::Unqualified),
            ("identifier=urn:x%7C", Refusal::Unqualified),
            ("identifier=a%7Cb&identifier=a%7Cc", Refusal::Repeated),
            ("identifier=a%7Cb&family=Synthetic", Refusal::Parameter),
            ("identifier=a%7Cb&birthdate=1970-01-01", Refusal::Parameter),
            (
                "identifier=a%7Cb&profile=http://hl7.org/fhir/uv/ips/StructureDefinition/Composition-uv-ips",
                Refusal::Profile,
            ),
            (
                "identifier=a%7Cb&profile=http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps%7C9.9.9",
                Refusal::Profile,
            ),
            ("identifier=a%7Cb&_format=xml", Refusal::Format),
        ] {
            assert_eq!(
                SummaryRequest::from_query(Some(query)),
                Err(refusal),
                "{query}"
            );
        }
    }

    #[test]
    fn the_eps_profile_is_taken_with_or_without_its_version() {
        for profile in [
            "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps",
            "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps%7C1.0.0-ballot",
        ] {
            let query = format!("identifier=a%7Cb&profile={profile}");
            assert!(SummaryRequest::from_query(Some(&query)).is_ok(), "{query}");
        }
    }

    #[test]
    fn a_parameters_body_names_the_patient_as_the_query_does() {
        let body = br#"{"resourceType":"Parameters","parameter":[{"name":"identifier","valueString":"urn:oid:2.999.1|synthetic-1"},{"name":"profile","valueCanonical":"http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"}]}"#;
        let read = SummaryRequest::from_parameters(body).unwrap();
        assert_eq!(read.system(), "urn:oid:2.999.1");
        assert_eq!(read.value(), "synthetic-1");
        assert_eq!(
            SummaryRequest::from_parameters(b"{\"resourceType\":\"Patient\"}"),
            Err(Refusal::Body)
        );
    }

    #[test]
    fn no_debug_shows_the_identifier() {
        let read = SummaryRequest::from_query(Some("identifier=a%7Csynthetic-77q")).unwrap();
        assert!(!format!("{read:?}").contains("77q"));
    }
}
