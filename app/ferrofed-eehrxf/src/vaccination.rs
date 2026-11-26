// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored query of the vaccination history, and the document the
//! gateway writes from one member's immunisation list.
//!
//! MyHealth@EU Requirements Catalogue v10.0.0 §1.24 asks the contact point of
//! the country of affiliation to make a patient's vaccination history
//! available. The document is MyHealth@EU Vaccination 10.0.0-preview's
//! `bundle-euvac-myhealtheu`, whose `composition-euvac-myhealtheu` is legally
//! authenticated (`Composition.attester:legalAuthenticator` 1..*, mode
//! `legal`, with a `time` and a `party`). The legal authenticator is the
//! person who attested the member's record: openEHR RM Common
//! `ORIGINAL_VERSION.attestations`, each an `ATTESTATION` whose `committer`
//! attested the version at `time_committed`. The composer is the record's
//! author and no authenticator, so a composition no person attested gives
//! no document.
//!
//! One document is written per composition that holds the immunisations
//! section ([`SECTION`]), per member: that composition is the member's
//! persistent immunisation list, and a history merged across members would
//! have no one legal authenticator. The stored query, held read-only under
//! the reserved namespace at [`VERSION`] (§12.7, N44), selects each such
//! composition with the attestations of its version
//! ([`section_query::attested`]).

use async_trait::async_trait;
use eehrxf::document::Document;
use eehrxf::document::authored::MYHEALTHEU_PRACTITIONER;
use eehrxf::document::language::DocumentLanguage;
use eehrxf::document::metadata::{Attestation, DocumentMetadata};
use eehrxf::document::vaccination::{IMMUNIZATION_HISTORY, VACCINATION_HISTORY_FORM};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::EUVAC;
use eehrxf::terminology::Terminology;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_registry::definition::{StoredDefinition, query_name};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::practitioner::Practitioner;
use fhir_types::r4::resource::Resource;
use openehr_its::rest::stored_query::{QualifiedQueryName, QueryNameError};
use openehr_query::printer;
use serde_json::Value;

use crate::list::LOINC;
use crate::prescriptions::mapped::Template;
use crate::report::{self, Author, Report, ServedError, Writes};
use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query;

/// The query's name in the reserved namespace.
pub const SLUG: &str = "vaccination-history";

/// The `SECTION` archetype of the immunisations a composition holds, the
/// section `eehrxf::mappings::EUVAC` enters.
pub const SECTION: &str = "openEHR-EHR-SECTION.immunisation_list.v0";

/// Returns the query's qualified name,
/// `eu.ferrofed.eehrxf::vaccination-history`.
///
/// # Errors
///
/// The [`QueryNameError`] of [`QualifiedQueryName::new`], which the name
/// does not meet.
pub fn name() -> Result<QualifiedQueryName, QueryNameError> {
    query_name(&format!("{NAMESPACE}::{SLUG}"))
}

/// Returns the query as AQL text, [`section_query::attested`] of
/// [`SECTION`] printed from its syntax tree.
#[must_use]
pub fn aql() -> String {
    printer::to_aql(&section_query::attested(SECTION), None)
}

/// Returns the query as the registry holds it: under its reserved name, at
/// [`VERSION`], saved at [`SAVED`].
///
/// # Errors
///
/// The [`QueryNameError`] of [`name`].
pub fn definition() -> Result<StoredDefinition, QueryNameError> {
    Ok(StoredDefinition::new(name()?, VERSION, aql(), SAVED))
}

/// A person who attested a member's composition, the document's legal
/// authenticator.
#[derive(Clone, PartialEq, Eq)]
pub struct Attester {
    name: String,
    identifiers: Vec<String>,
    time: String,
}

impl std::fmt::Debug for Attester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attester").finish_non_exhaustive()
    }
}

/// Why a member's composition names no person who attested it; no
/// variant's text names a value the record carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unattested {
    /// The version carries no attestation, or one that is still pending.
    #[error("no attestation of the record's version is complete")]
    None,
    /// Every complete attestation is by a committer the gateway cannot
    /// describe as a person: no `PARTY_IDENTIFIED` with a name, or one with
    /// no `time_committed`.
    #[error("no complete attestation of the record's version names a person and a time")]
    Impersonal,
}

/// The persons who attested a version, read from `attestations`, the
/// `ORIGINAL_VERSION.attestations` of its canonical JSON: one `ATTESTATION`
/// or a list of them.
///
/// An attestation counts when it says it is not pending (`is_pending`,
/// mandatory) and its
/// `committer` is a `PARTY_IDENTIFIED` with a name (openEHR RM Common
/// `ATTESTATION`, `AUDIT_DETAILS`); a `PARTY_SELF` is the record's subject.
///
/// # Errors
///
/// [`Unattested::None`] when no attestation is complete, and
/// [`Unattested::Impersonal`] when none of those names a person and a
/// time.
pub fn attesters(attestations: &Value) -> Result<Vec<Attester>, Unattested> {
    let listed = match attestations {
        Value::Array(listed) => listed.iter().collect(),
        Value::Object(_) => vec![attestations],
        _ => Vec::new(),
    };
    let complete: Vec<&Value> = listed
        .into_iter()
        .filter(|attestation| {
            attestation.get("_type").and_then(Value::as_str) == Some("ATTESTATION")
        })
        .filter(|attestation| attestation.get("is_pending").and_then(Value::as_bool) == Some(false))
        .collect();
    if complete.is_empty() {
        return Err(Unattested::None);
    }
    let persons: Vec<Attester> = complete.into_iter().filter_map(person).collect();
    if persons.is_empty() {
        return Err(Unattested::Impersonal);
    }
    Ok(persons)
}

/// Why a member's `ORIGINAL_VERSION` gives no vaccination history; no
/// variant's text names a value the record carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unversioned {
    /// The answer is no JSON.
    #[error("the version is no canonical JSON")]
    Json(#[source] serde_json::Error),
    /// The answer is no `ORIGINAL_VERSION` holding a composition.
    #[error("the version holds no composition")]
    NoData,
    /// No person attested the version.
    #[error(transparent)]
    Unattested(#[from] Unattested),
}

/// The composition, as canonical JSON, and the persons who attested it,
/// read from `version`, an `ORIGINAL_VERSION` in canonical JSON (openEHR RM
/// Common `ORIGINAL_VERSION.data` and `.attestations`).
///
/// # Errors
///
/// The [`Unversioned`] that says why `version` gives none.
pub fn original_version(version: &str) -> Result<(String, Vec<Attester>), Unversioned> {
    let version: Value = serde_json::from_str(version).map_err(Unversioned::Json)?;
    if version.get("_type").and_then(Value::as_str) != Some("ORIGINAL_VERSION") {
        return Err(Unversioned::NoData);
    }
    let composition = version
        .get("data")
        .filter(|data| data.get("_type").and_then(Value::as_str) == Some("COMPOSITION"))
        .ok_or(Unversioned::NoData)?;
    let attested = attesters(version.get("attestations").unwrap_or(&Value::Null))?;
    Ok((composition.to_string(), attested))
}

/// The person `attestation` names, when its committer is a named
/// `PARTY_IDENTIFIED` and it carries a `time_committed`.
fn person(attestation: &Value) -> Option<Attester> {
    // NOTE: openEHR RM Common AUDIT_DETAILS.committer is a PARTY_PROXY; an attestation by no
    // named PARTY_IDENTIFIED is legitimately no person's, so it names no legal authenticator.
    let committer = attestation
        .get("committer")
        .filter(|party| party.get("_type").and_then(Value::as_str) == Some("PARTY_IDENTIFIED"))?;
    let name = committer
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())?;
    let time = attestation
        .pointer("/time_committed/value")
        .and_then(Value::as_str)?;
    let identifiers = committer
        .get("identifiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|identifier| identifier.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    Some(Attester {
        name: name.to_owned(),
        identifiers,
        time: time.to_owned(),
    })
}

/// Why the patient cannot be the subject of a vaccination history; no
/// variant's text names a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unnamed {
    /// The identity binding holds no name of the patient.
    #[error("the identity binding holds no name of the patient, which MyHealth@EU Core requires")]
    Name,
    /// The identity binding holds no birth date of the patient.
    #[error(
        "the identity binding holds no birth date of the patient, which MyHealth@EU Core requires"
    )]
    BirthDate,
}

/// Whether `header` names the patient as the vaccination history's subject
/// must be named.
///
/// # Errors
///
/// The [`Unnamed`] of the element the header lacks.
pub fn named(header: &PatientHeader) -> Result<(), Unnamed> {
    // NOTE: MyHealth@EU Core 1.0.1 patient-myhealtheu-core: Patient.name 1..* and
    // Patient.birthDate 1..1, which a data-absent-reason does not meet for a written document.
    if !header.named() {
        return Err(Unnamed::Name);
    }
    if header.birth_date.is_none() {
        return Err(Unnamed::BirthDate);
    }
    Ok(())
}

/// The shipped vaccination mapping compiled against a deployment's patient
/// summary template, which writes a member's immunisation list as a
/// MyHealth@EU `bundle-euvac-myhealtheu` document.
#[derive(Debug)]
pub struct Served {
    mapping: Mapping,
}

impl Served {
    /// Compiles the shipped vaccination mapping (`eehrxf::mappings::EUVAC`)
    /// against `template`.
    ///
    /// # Errors
    ///
    /// [`ServedError::Template`] when the template does not read and
    /// [`ServedError::Compile`] when the mapping does not compile against
    /// it.
    pub fn compile(template: Template<'_>) -> Result<Self, ServedError> {
        Ok(Self {
            mapping: report::compiled(template, &EUVAC)?,
        })
    }
}

#[async_trait]
impl Writes for Served {
    /// Writes `composition`, a composition holding the immunisations
    /// section in canonical JSON, as the `bundle-euvac-myhealtheu` document
    /// `report` names, legally authenticated by each person `report` names
    /// as an attester of its version.
    ///
    /// The document's `Composition` is authored by the gateway's `Device`,
    /// whose owner is the operator's `Organization`, its custodian the
    /// member's `Organization`, its subject the `Patient` the identifier
    /// names, and it is written in the record's language-region tag once
    /// `terminology` answers that eHDSILanguage holds it. Its immunisations
    /// section lists each vaccination the mapping writes, and one
    /// `Provenance` names the member as their source.
    ///
    /// # Errors
    ///
    /// [`ServedError::Composition`] when the composition is no JSON,
    /// [`ServedError::Unattested`] when `report` names no attester,
    /// [`ServedError::Unnamed`] when the patient has no name or birth date,
    /// [`ServedError::Language`] when its language is not held,
    /// [`ServedError::Run`] when it does not map, and
    /// [`ServedError::Document`] when the document cannot be assembled.
    async fn document(
        &self,
        composition: &str,
        report: Report<'_>,
        terminology: &Terminology,
    ) -> Result<Bundle, ServedError> {
        let record: Value = serde_json::from_str(composition).map_err(ServedError::Composition)?;
        if report.attesters.is_empty() {
            return Err(ServedError::Unattested(Unattested::None));
        }
        named(report.header).map_err(ServedError::Unnamed)?;
        let mut document = Document::of(
            &VACCINATION_HISTORY_FORM,
            report.base,
            report.identifier.to_owned(),
            report.timestamp.to_owned(),
            String::from("Vaccination history"),
        );
        let language = DocumentLanguage::of_composition(&record).map_err(ServedError::Language)?;
        if let Some(value_set) = VACCINATION_HISTORY_FORM.language.binding {
            document
                .with_held_language(
                    language
                        .held(terminology, value_set)
                        .await
                        .map_err(ServedError::Language)?,
                )
                .map_err(ServedError::Document)?;
        }
        let mapped = report::authored(
            &mut document,
            report,
            Author::Gateway(
                "Software that writes the vaccination history from the member's record automatically",
            ),
            (&self.mapping, composition),
        )?;
        let withheld = report::withheld(mapped.bundle());
        let sections = document
            .with_mapped_composition(mapped.bundle())
            .map_err(ServedError::Document)?;
        let entries: Vec<String> = sections
            .iter()
            .flat_map(|section| section.entries.iter().cloned())
            .collect();
        report::sourced(&mut document, report, &entries).map_err(ServedError::Document)?;
        for section in sections {
            let paragraphs = std::iter::once(String::from(
                "The immunisations as the member records them, written by the gateway.",
            ))
            .chain(withheld.clone())
            .collect();
            document
                .with_section(section.into_section(paragraphs))
                .map_err(ServedError::Document)?;
        }
        let mut metadata = DocumentMetadata::from_composition(&record, history(), None)
            .map_err(ServedError::Document)?;
        for (index, attester) in report.attesters.iter().enumerate() {
            let party = document
                .add(&[Resource::Practitioner(Box::new(practitioner(
                    index, attester,
                )))])
                .map_err(ServedError::Document)?
                .into_iter()
                .next()
                .ok_or(ServedError::Unattested(Unattested::Impersonal))?;
            metadata =
                metadata.with_legal_authentication(Attestation::new(party, attester.time.clone()));
        }
        document.with_metadata(metadata);
        document.into_bundle().map_err(ServedError::Document)
    }
}

/// The document category of a vaccination history, LOINC
/// [`IMMUNIZATION_HISTORY`], the type `composition-euvac-myhealtheu` fixes.
fn history() -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding {
            system: Some(LOINC.into()),
            code: Some(IMMUNIZATION_HISTORY.into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }
}

/// The `Practitioner` of the attester at `index`, claiming the MyHealth@EU
/// Core profile `composition-euvac-myhealtheu` admits as the legal
/// authenticator's party, each `DV_IDENTIFIER.id` as an identifier value.
fn practitioner(index: usize, attester: &Attester) -> Practitioner {
    Practitioner {
        id: Some(format!("legal-authenticator-{index}")),
        meta: Some(Meta {
            profile: vec![MYHEALTHEU_PRACTITIONER.into()],
            ..Meta::default()
        }),
        identifier: attester
            .identifiers
            .iter()
            .map(|value| Identifier {
                value: Some(value.as_str().into()),
                ..Identifier::default()
            })
            .collect(),
        name: vec![HumanName {
            text: Some(attester.name.as_str().into()),
            ..HumanName::default()
        }],
        ..Practitioner::default()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Unattested, attesters};

    fn attestation(committer: serde_json::Value, pending: bool) -> serde_json::Value {
        json!({
            "_type": "ATTESTATION",
            "system_id": "synthetic.example.org",
            "time_committed": { "_type": "DV_DATE_TIME", "value": "2026-10-01T09:00:00Z" },
            "change_type": { "_type": "DV_CODED_TEXT", "value": "creation", "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "249" } },
            "committer": committer,
            "is_pending": pending,
            "reason": { "_type": "DV_TEXT", "value": "signed" }
        })
    }

    fn person() -> serde_json::Value {
        json!({ "_type": "PARTY_IDENTIFIED", "name": "Synthetic Attester" })
    }

    #[test]
    fn a_complete_attestation_by_a_named_person_names_its_attester() {
        let found = attesters(&json!([attestation(person(), false)])).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn no_attestation_or_only_a_pending_one_is_none() {
        assert_eq!(attesters(&json!([])), Err(Unattested::None));
        assert_eq!(attesters(&serde_json::Value::Null), Err(Unattested::None));
        assert_eq!(
            attesters(&json!([attestation(person(), true)])),
            Err(Unattested::None)
        );
    }

    #[test]
    fn an_attestation_by_the_subject_or_a_nameless_party_is_impersonal() {
        assert_eq!(
            attesters(&attestation(json!({ "_type": "PARTY_SELF" }), false)),
            Err(Unattested::Impersonal)
        );
        assert_eq!(
            attesters(&json!([attestation(
                json!({ "_type": "PARTY_IDENTIFIED" }),
                false
            )])),
            Err(Unattested::Impersonal)
        );
    }
}
