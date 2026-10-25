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
//! face keeps no `Patient` resource (the lookup of [`crate::lookup`] writes
//! one from the identity binding for each answer), and a request that names
//! the patient by anything other than an identifier with its system, such as
//! demographics, is refused, so the patient is always the one an identity
//! service has confirmed (§5.2, N32).

use std::fmt;

use eehrxf::category::Category;
use fhir_types::r4::capability_statement::{
    CapabilityStatement, CapabilityStatementRest, CapabilityStatementRestResource,
    CapabilityStatementRestResourceInteraction, CapabilityStatementRestResourceOperation,
    CapabilityStatementRestResourceSearchParam, CapabilityStatementSoftware,
};
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameterValue};

use crate::receive::Receivers;
use crate::serving::Serving;

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
/// product `product` at `version`, receiving documents as `receivers`
/// declares and serving the categories `serving` declares.
///
/// It describes a server that serves `$summary` and the lookup by
/// identifier on `Patient`, the
/// `DocumentReference` search of the document list ([`crate::list`]), the
/// `Binary` read of a member's document, the `create` of a received
/// document on `Bundle` ([`bundle`]) only when a member receives a category,
/// and, only for a category a member is declared for, `MedicationRequest` and `MedicationDispense`
/// ([`Serving::resources`]), in FHIR JSON.
#[must_use]
pub fn capability(
    product: &str,
    version: &str,
    date: &str,
    (receivers, serving): (&Receivers, &Serving),
) -> CapabilityStatement {
    let mut resource = vec![patient(), documents(), binary()];
    // NOTE: FHIR R4 `capabilitystatement.html`: the statement describes what the server
    // supports, so `Bundle` create is declared only when a member receives a category.
    if !receivers.is_empty() {
        resource.push(bundle(receivers));
    }
    resource.extend(serving.resources());
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
            resource,
            ..CapabilityStatementRest::default()
        }],
        ..CapabilityStatement::default()
    }
}

/// The `Patient` resource of the capability statement: the `$summary`
/// operation, and the search-type interaction of the lookup by identifier
/// ([`crate::lookup`]), its one search parameter `identifier`.
fn patient() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: "Patient".into(),
        interaction: vec![CapabilityStatementRestResourceInteraction {
            code: "search-type".into(),
            documentation: Some(
                "The patient lookup by identifier: one Patient, as the identity binding holds it, when a member holds the patient, and an empty searchset otherwise; a search by demographics is refused.".into(),
            ),
            ..CapabilityStatementRestResourceInteraction::default()
        }],
        search_param: vec![CapabilityStatementRestResourceSearchParam {
            name: "identifier".into(),
            definition: Some(crate::lookup::IDENTIFIER_PARAMETER.into()),
            r#type: "token".into(),
            documentation: Some(
                "The patient, as system|value; required, one patient alone, and the only parameter taken.".into(),
            ),
            ..CapabilityStatementRestResourceSearchParam::default()
        }],
        operation: vec![CapabilityStatementRestResourceOperation {
            name: "summary".into(),
            definition: SUMMARY_OPERATION.into(),
            ..CapabilityStatementRestResourceOperation::default()
        }],
        ..CapabilityStatementRestResource::default()
    }
}

/// The `Binary` resource of the capability statement: the read of a
/// member's document at the URL the document list names.
fn binary() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: "Binary".into(),
        interaction: vec![CapabilityStatementRestResourceInteraction {
            code: "read".into(),
            documentation: Some(
                "A member's own document, at the attachment URL the document list names; the URL is valid for a short time and for the caller it was listed for.".into(),
            ),
            ..CapabilityStatementRestResourceInteraction::default()
        }],
        ..CapabilityStatementRestResource::default()
    }
}

/// The `Bundle` resource of the capability statement: the `create` of a
/// document in the exchange format, at the end-point FHIR R4 documents
/// §3.3.4 names.
///
/// Its documentation names, by their code in HL7 Europe's
/// `EEHRxFDocumentPriorityCategoryCS`, the Art 14(1) categories the
/// deployment declares a receiving member for, and every other category
/// this build carries, which no member receives; its supported profiles are
/// the `Bundle` profiles of the received categories. It names no member.
#[must_use]
pub fn bundle(receivers: &Receivers) -> CapabilityStatementRestResource {
    let received: Vec<Category> = receivers.members().map(|(category, _)| category).collect();
    let codes = |categories: &mut dyn Iterator<Item = Category>| {
        let codes: Vec<&str> = categories.map(Category::code).collect();
        if codes.is_empty() {
            String::from("none")
        } else {
            codes.join(", ")
        }
    };
    let unreceived = codes(&mut receivers.unreceived());
    let documentation = format!(
        "Receives a document in the European electronic health record exchange format and writes it to the one member the deployment declares for its Art 14(1) category. Categories received: {}. Categories with no receiving member: {unreceived}; a document of one of them, or of any other category, is refused with an OperationOutcome that names its category.",
        codes(&mut received.iter().copied()),
    );
    CapabilityStatementRestResource {
        r#type: "Bundle".into(),
        supported_profile: received
            .iter()
            .filter_map(|category| category.profiles())
            .map(|profiles| profiles.bundle.into())
            .collect(),
        documentation: Some(documentation.into()),
        interaction: vec![CapabilityStatementRestResourceInteraction {
            code: "create".into(),
            documentation: Some(
                "POST of a document Bundle as application/fhir+json; the answer is an OperationOutcome naming the member that holds it.".into(),
            ),
            ..CapabilityStatementRestResourceInteraction::default()
        }],
        ..CapabilityStatementRestResource::default()
    }
}

/// The `DocumentReference` resource of the capability statement: the
/// search-type interaction with the two parameters the list takes, after
/// the HL7 Europe EU Health Data API Document Access Provider.
fn documents() -> CapabilityStatementRestResource {
    let parameter = |name: &str, definition: Option<&str>, documentation: &str| {
        CapabilityStatementRestResourceSearchParam {
            name: name.into(),
            definition: definition.map(Into::into),
            r#type: "token".into(),
            documentation: Some(documentation.into()),
            ..CapabilityStatementRestResourceSearchParam::default()
        }
    };
    CapabilityStatementRestResource {
        r#type: "DocumentReference".into(),
        supported_profile: vec![crate::list::DOCUMENT_REFERENCE_PROFILE.into()],
        interaction: vec![CapabilityStatementRestResourceInteraction {
            code: "search-type".into(),
            ..CapabilityStatementRestResourceInteraction::default()
        }],
        search_param: vec![
            parameter(
                "patient.identifier",
                None,
                "The patient, as system|value; required, and one patient alone.",
            ),
            parameter(
                "type",
                Some("http://hl7.org/fhir/SearchParameter/clinical-type"),
                "The document type; the face lists patient summaries, LOINC 60591-5.",
            ),
        ],
        ..CapabilityStatementRestResource::default()
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

    #[test]
    fn a_deployment_that_receives_nothing_names_every_category_unreceived() {
        let resource = super::bundle(&crate::receive::Receivers::default());
        assert_eq!(
            resource.r#type.value.as_deref().unwrap_or_default(),
            "Bundle"
        );
        let interactions: Vec<&str> = resource
            .interaction
            .iter()
            .map(|interaction| interaction.code.value.as_deref().unwrap_or_default())
            .collect();
        assert_eq!(interactions, ["create"]);
        assert_eq!(
            resource.supported_profile,
            Vec::<fhir_types::r4::primitives::Canonical>::new()
        );
        let documentation = resource.documentation.unwrap().value.unwrap();
        assert!(
            documentation.contains(
                "Categories received: none. Categories with no receiving member: Patient-Summaries"
            ),
            "{documentation:?}"
        );
    }

    #[test]
    fn a_category_with_no_document_profile_is_never_named_unreceived() {
        let receivers = crate::receive::Receivers::default();
        let unreceived: Vec<&str> = receivers
            .unreceived()
            .map(eehrxf::category::Category::code)
            .collect();
        assert_eq!(unreceived, ["Patient-Summaries"]);
        let documentation = super::bundle(&receivers)
            .documentation
            .unwrap()
            .value
            .unwrap();
        assert!(
            documentation.contains("Categories with no receiving member: Patient-Summaries;"),
            "{documentation:?}"
        );
        for code in ["Electronic-Prescriptions", "Electronic-Dispensations"] {
            assert!(!documentation.contains(code), "{documentation:?}");
        }
    }
}
