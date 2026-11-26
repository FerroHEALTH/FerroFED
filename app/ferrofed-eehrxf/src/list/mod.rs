// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document list of the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1): the `DocumentReference` search, and the `searchset` it answers.
//!
//! The search is the HL7 Europe EU Health Data API 1.0.0-ballot Document
//! Access Provider's `DocumentReference` search-type interaction, with its
//! `patient.identifier` and `type` parameters
//! (`CapabilityStatement-EEHRxF-DocumentAccessProvider`), and the further
//! parameters of the MyHealth@EU national contact point API the gateway
//! evaluates on every entry ([`criteria`]). It names the
//! patient by an identifier with its system, as `$summary` does, so the
//! patient is always the one an identity service confirms (§5.2, N32). Each
//! entry follows the `EehrxfMhdDocumentReference` profile: one for the
//! patient summary the gateway writes on request, retrievable through
//! `Patient/$summary`, and one for each document a member serves itself,
//! read through the gateway at a URL it signs. A member that serves its
//! laboratory, discharge or imaging reports as a Document Access Provider
//! has its own `DocumentReference`s listed as it holds them ([`Match::Passed`]), each
//! attachment URL readdressed to the gateway's ([`readdressed`]).

pub mod criteria;
pub mod written;

use fhir_types::codec::Value;
use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::{DocumentReference, DocumentReferenceContent};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::{Resource, UnknownResource};

use crate::face::FHIR_JSON;
use crate::list::criteria::{Criteria, CriterionError};
use crate::summary::Organisation;

/// The canonical URL of the profile every entry follows.
pub const DOCUMENT_REFERENCE_PROFILE: &str =
    "http://hl7.eu/fhir/health-data-api/StructureDefinition/EehrxfMhdDocumentReference";

/// The canonical URL of the `CodeSystem` of the priority categories.
pub const CATEGORY_SYSTEM: &str =
    "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs";

/// The LOINC system.
pub const LOINC: &str = "http://loinc.org";

/// The LOINC code of a patient summary document, the one code of the
/// `eehrxf-document-type-patient-summary-vs` value set.
pub const PATIENT_SUMMARY: &str = "60591-5";

/// The LOINC code of a laboratory report, the one code of the
/// `eehrxf-document-type-laboratory-report-vs` value set.
pub const LABORATORY_REPORT: &str = "11502-2";

/// The LOINC codes of a discharge report, the codes of the
/// `eehrxf-document-type-discharge-report-vs` value set, in its order.
pub const DISCHARGE_REPORTS: [&str; 2] = [DISCHARGE_SUMMARY, "100719-4"];

/// The LOINC code of a discharge summary, the first code of the
/// `eehrxf-document-type-discharge-report-vs` value set.
pub const DISCHARGE_SUMMARY: &str = "18842-5";

/// The LOINC code a discharge report the gateway writes is listed under.
///
/// It is the hospital discharge summary `composition-eu-hdr` fixes, and the
/// one code of MyHealth@EU `eHDSIHospitalDischargeReportType`.
pub const HOSPITAL_DISCHARGE_SUMMARY: &str = "34105-7";

/// The LOINC codes of a medical imaging document, the codes of the
/// `eehrxf-document-type-medical-imaging-vs` value set, in its order.
pub const IMAGING_DOCUMENTS: [&str; 2] = [IMAGING_REPORT, IMAGING_STUDY];

/// The LOINC code of a diagnostic imaging report, the first code of the
/// `eehrxf-document-type-medical-imaging-vs` value set.
pub const IMAGING_REPORT: &str = "85430-7";

/// The LOINC code an imaging report the gateway writes is listed under.
///
/// It is the diagnostic imaging study, the second code of the
/// `eehrxf-document-type-medical-imaging-vs` value set and the one code of
/// MyHealth@EU `eHDSIMedicalImagingReportType`.
pub const IMAGING_STUDY: &str = "18748-4";

/// The LOINC code a vaccination history is listed under, `11369-6`
/// "History of Immunization note", which `composition-euvac-myhealtheu`
/// fixes as its type and MyHealth@EU `eHDSIDocumentCode` holds.
pub const IMMUNIZATION_HISTORY: &str = eehrxf::document::vaccination::IMMUNIZATION_HISTORY;

/// The canonical URL of the document `Bundle` profile the gateway's own
/// summary follows.
pub const EPS_BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The system of an identifier that is a URI (FHIR R4 `identifier-registry`).
pub(super) const URI_SYSTEM: &str = "urn:ietf:rfc:3986";

/// The `_format` values that ask for FHIR JSON.
const JSON_FORMATS: [&str; 3] = ["json", "application/json", FHIR_JSON];

/// The patient a document search names, and whether its `type` admits a
/// patient summary.
#[derive(Clone, PartialEq, Eq)]
pub struct ListRequest {
    subject: Subject,
    summaries: bool,
    laboratory: bool,
    hospital_discharge: bool,
    vaccination: bool,
    discharge: Vec<&'static str>,
    imaging: Vec<&'static str>,
    criteria: Criteria,
}

impl std::fmt::Debug for ListRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListRequest")
            .field("summaries", &self.summaries)
            .field("laboratory", &self.laboratory)
            .field("hospital_discharge", &self.hospital_discharge)
            .field("vaccination", &self.vaccination)
            .field("discharge", &self.discharge)
            .field("imaging", &self.imaging)
            .field("criteria", &self.criteria)
            .finish_non_exhaustive()
    }
}

/// How a document search names its patient.
#[derive(Clone, PartialEq, Eq)]
pub enum Subject {
    /// `patient.identifier`: by an identifier with its system.
    Identifier {
        /// The identifier system.
        system: String,
        /// The identifier value.
        value: String,
    },
    /// `patient`: by the logical id of a `Patient` the face answered, as
    /// the reference names it, `[id]`, `Patient/[id]` or an absolute URL
    /// (FHIR R4 search §3.1.1.4.11).
    Patient(String),
    /// `_id` alone, or a read by id: by the logical id of a document the
    /// face listed, which stands for its patient.
    Document(String),
}

impl std::fmt::Debug for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identifier { .. } => f.write_str("Identifier"),
            Self::Patient(_) => f.write_str("Patient"),
            Self::Document(_) => f.write_str("Document"),
        }
    }
}

/// Why a document search is refused, naming no value it carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ListRefusal {
    /// The search names neither `patient.identifier` nor `patient`, nor
    /// one document by `_id`.
    #[error(
        "the search names the patient by patient.identifier=system|value or patient=Patient/id, \
         or one document by _id"
    )]
    NoPatient,
    /// The patient is named more than once, or several patients are.
    #[error("the search names one patient")]
    Several,
    /// The identifier has no system or no value.
    #[error("the identifier names its system and its value as system|value")]
    Unqualified,
    /// The search carries a parameter or a modifier the face does not take.
    #[error(
        "the search takes patient.identifier, patient, _id, type, category, contenttype, status, modality, \
         bodysite, creation, date, period and _format alone, with no modifier"
    )]
    Parameter,
    /// A value is outside its parameter's grammar.
    #[error(transparent)]
    Value(#[from] CriterionError),
    /// The `_format` asks for something other than FHIR JSON.
    #[error("the face answers application/fhir+json alone")]
    Format,
}

impl ListRequest {
    /// Returns how the search names its patient.
    #[must_use]
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// Returns whether every `type` the search gives admits a patient
    /// summary, the one document type the face lists.
    #[must_use]
    pub fn admits_summaries(&self) -> bool {
        self.summaries
    }

    /// Returns whether every `type` the search gives admits a laboratory
    /// report.
    #[must_use]
    pub fn admits_laboratory(&self) -> bool {
        self.laboratory
    }

    /// Returns whether every `type` the search gives admits a hospital
    /// discharge summary ([`HOSPITAL_DISCHARGE_SUMMARY`]).
    #[must_use]
    pub fn admits_hospital_discharge(&self) -> bool {
        self.hospital_discharge
    }

    /// Returns whether every `type` the search gives admits a vaccination
    /// history ([`IMMUNIZATION_HISTORY`]).
    #[must_use]
    pub fn admits_vaccination(&self) -> bool {
        self.vaccination
    }

    /// Returns the codes of [`DISCHARGE_REPORTS`] every `type` the search
    /// gives admits, in their order; none when it admits no discharge
    /// report.
    #[must_use]
    pub fn discharge(&self) -> &[&'static str] {
        &self.discharge
    }

    /// Returns the codes of [`IMAGING_DOCUMENTS`] every `type` the search
    /// gives admits, in their order; none when it admits no imaging
    /// document.
    #[must_use]
    pub fn imaging(&self) -> &[&'static str] {
        &self.imaging
    }

    /// Returns whether `document`, a listed `DocumentReference` as JSON,
    /// meets every national contact point API parameter the search gives,
    /// the instant `now`
    /// deciding an `ap` date.
    #[must_use]
    pub fn selects(&self, document: &Value, now: jiff::Timestamp) -> bool {
        self.criteria.selects(document, now)
    }

    /// Reads the search from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`ListRefusal`] the search is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, ListRefusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        let mut subject = None;
        let mut summaries = true;
        let mut laboratory = true;
        let mut hospital_discharge = true;
        let mut vaccination = true;
        let mut discharge = DISCHARGE_REPORTS.to_vec();
        let mut imaging = IMAGING_DOCUMENTS.to_vec();
        let mut criteria = Criteria::default();
        let mut ids = Vec::new();
        for (name, value) in pairs {
            match &*name {
                // NOTE: FHIR R4 search, several values; a comma separates the values a parameter
                // ORs, so a value holding one names several patients.
                "patient.identifier" | "patient" => {
                    if value.contains(',') {
                        return Err(ListRefusal::Several);
                    }
                    let named = if name == "patient" {
                        Subject::Patient(value.into_owned())
                    } else {
                        identified(&value)?
                    };
                    if subject.replace(named).is_some() {
                        return Err(ListRefusal::Several);
                    }
                }
                // NOTE: FHIR R4 search, several values; repeated parameters are ANDed and the
                // values of one ORed, so every repetition must admit the summary type.
                "type" => {
                    summaries &= value.split(',').any(|token| admits(token, PATIENT_SUMMARY));
                    laboratory &= value
                        .split(',')
                        .any(|token| admits(token, LABORATORY_REPORT));
                    hospital_discharge &= value
                        .split(',')
                        .any(|token| admits(token, HOSPITAL_DISCHARGE_SUMMARY));
                    vaccination &= value
                        .split(',')
                        .any(|token| admits(token, IMMUNIZATION_HISTORY));
                    for codes in [&mut discharge, &mut imaging] {
                        codes.retain(|code| value.split(',').any(|token| admits(token, code)));
                    }
                }
                "_format" => {
                    if !JSON_FORMATS.contains(&&*value) {
                        return Err(ListRefusal::Format);
                    }
                }
                other => {
                    if other == "_id" {
                        ids.push(value.clone().into_owned());
                    }
                    if !criteria.read(other, &value)? {
                        return Err(ListRefusal::Parameter);
                    }
                }
            }
        }
        // NOTE: no specification governs this: our own design; one `_id` of one value names
        // the document's patient through the record of the id, as a read by id does.
        let subject = match (subject, ids.as_slice()) {
            (Some(subject), _) => subject,
            (None, [id]) if !id.contains(',') => Subject::Document(id.clone()),
            (None, _) => return Err(ListRefusal::NoPatient),
        };
        Ok(Self {
            subject,
            summaries,
            laboratory,
            hospital_discharge,
            vaccination,
            discharge,
            imaging,
            criteria,
        })
    }
}

impl ListRequest {
    /// The search a read of the document `id` runs: every type the face
    /// lists, the document's patient named by the id, and the entry the id
    /// names selected.
    ///
    /// # Errors
    ///
    /// [`ListRefusal::Value`] when `id` is no `_id` value.
    pub fn read(id: &str) -> Result<Self, ListRefusal> {
        let mut criteria = Criteria::default();
        criteria.read("_id", &fhir_search::escape::escape(id))?;
        Ok(Self {
            subject: Subject::Document(id.to_owned()),
            summaries: true,
            laboratory: true,
            hospital_discharge: true,
            vaccination: true,
            discharge: DISCHARGE_REPORTS.to_vec(),
            imaging: IMAGING_DOCUMENTS.to_vec(),
            criteria,
        })
    }
}

/// The patient `identifier`, a `patient.identifier` value, names.
fn identified(identifier: &str) -> Result<Subject, ListRefusal> {
    let (system, value) = identifier.split_once('|').ok_or(ListRefusal::Unqualified)?;
    if system.is_empty() || value.is_empty() {
        return Err(ListRefusal::Unqualified);
    }
    Ok(Subject::Identifier {
        system: system.to_owned(),
        value: value.to_owned(),
    })
}

/// Whether the token `token` matches the document type `kind`, a LOINC
/// code: the code alone, the code in the LOINC system, or any code of the
/// LOINC system (FHIR R4 search, the token type).
fn admits(token: &str, kind: &str) -> bool {
    match token.split_once('|') {
        None => token == kind,
        Some((system, code)) => system == LOINC && (code.is_empty() || code == kind),
    }
}

/// What every entry of a list names: the patient, as the search named it,
/// and when the entries were written, a FHIR `instant`.
#[derive(Debug, Clone, Copy)]
pub struct Listing<'a> {
    /// The patient's identifier system.
    pub system: &'a str,
    /// The patient's identifier value.
    pub value: &'a str,
    /// The patient's logical id on the face.
    pub patient: &'a str,
    /// When the list is written.
    pub date: &'a str,
}

/// A member's document as the list fetched it, named by a URL the gateway
/// serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The media type the member answered with.
    pub content_type: String,
    /// The URL the gateway serves the document at, relative to the FHIR
    /// base.
    pub url: String,
    /// The SHA-1 digest of the member's bytes, base64.
    pub hash: String,
    /// The number of the member's bytes.
    pub size: u32,
}

/// Returns the entry, with the logical id `id` and the unique id
/// `unique`, a `urn:uuid:` URN, for the patient summary the gateway writes
/// on request as the operator `custodian`.
///
/// The document is named by the `Patient/$summary` request that writes it,
/// relative to the FHIR base, the patient's own identifier in its query.
#[must_use]
pub fn on_demand(
    (id, unique): (&str, &str),
    listing: Listing<'_>,
    custodian: &Organisation,
) -> DocumentReference {
    let identifier = format!("{}|{}", listing.system, listing.value);
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("identifier", &identifier)
        .finish();
    let attachment = Attachment {
        content_type: Some(FHIR_JSON.into()),
        url: Some(format!("Patient/$summary?{query}").into()),
        title: Some("Patient summary written on request".into()),
        ..Attachment::default()
    };
    let format = Coding {
        system: Some(URI_SYSTEM.into()),
        code: Some(EPS_BUNDLE.into()),
        ..Coding::default()
    };
    entry(
        (id, (URI_SYSTEM, unique)),
        (SUMMARY, listing),
        custodian,
        (attachment, Some(format)),
    )
}

/// Returns the entry, with the logical id `id`, of the document a member
/// `custodian` serves itself, as `fetched` holds it.
///
/// The document's own `Bundle.identifier`, `identifier`, which FHIR R4
/// `bdl-9` requires of every document, names it.
#[must_use]
pub fn native(
    (id, identifier): (&str, (&str, &str)),
    listing: Listing<'_>,
    custodian: &Organisation,
    fetched: Fetched,
) -> DocumentReference {
    // NOTE: `EehrxfMhdDocumentReference` 1.0.0-ballot takes no `attachment.data` and requires
    // `attachment.url`, so the bytes are read at a URL of the gateway's, never of the member's.
    let attachment = Attachment {
        content_type: Some(fetched.content_type.into()),
        url: Some(fetched.url.into()),
        hash: Some(fetched.hash.into()),
        size: Some(fetched.size.into()),
        title: Some("Patient summary served by the member".into()),
        ..Attachment::default()
    };
    entry(
        (id, identifier),
        (SUMMARY, listing),
        custodian,
        (attachment, None),
    )
}

/// The document type and the priority category an entry names.
#[derive(Debug, Clone, Copy)]
pub(super) struct Kind {
    /// The LOINC code of the type.
    pub(super) code: &'static str,
    /// Its display.
    pub(super) display: &'static str,
    /// The priority category's code.
    pub(super) category: &'static str,
    /// Its display.
    pub(super) category_display: &'static str,
}

/// A patient summary.
const SUMMARY: Kind = Kind {
    code: PATIENT_SUMMARY,
    display: "Patient summary Document",
    category: "Patient-Summaries",
    category_display: "patient summaries",
};

/// The entry with the logical id `id` and the master identifier
/// `system`|`value`, of the content `attachment` in the format `format`.
pub(super) fn entry(
    (id, (system, value)): (&str, (&str, &str)),
    (kind, listing): (Kind, Listing<'_>),
    custodian: &Organisation,
    (attachment, format): (Attachment, Option<Coding>),
) -> DocumentReference {
    DocumentReference {
        id: Some(id.to_owned()),
        meta: Some(Meta {
            profile: vec![DOCUMENT_REFERENCE_PROFILE.into()],
            ..Meta::default()
        }),
        master_identifier: Some(Identifier {
            system: Some(system.into()),
            value: Some(value.into()),
            ..Identifier::default()
        }),
        status: "current".into(),
        r#type: Some(CodeableConcept {
            coding: vec![Coding {
                system: Some(LOINC.into()),
                code: Some(kind.code.into()),
                display: Some(kind.display.into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        }),
        category: vec![CodeableConcept {
            coding: vec![Coding {
                system: Some(CATEGORY_SYSTEM.into()),
                code: Some(kind.category.into()),
                display: Some(kind.category_display.into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        }],
        subject: Some(subject(listing)),
        date: Some(listing.date.into()),
        custodian: Some(organisation(custodian)),
        content: vec![DocumentReferenceContent {
            attachment,
            format,
            ..DocumentReferenceContent::default()
        }],
        ..DocumentReference::default()
    }
}

/// The reference to the patient of `listing`: its id on the face and the
/// identifier the search named it by.
pub(super) fn subject(listing: Listing<'_>) -> Reference {
    Reference {
        reference: Some(format!("Patient/{}", listing.patient).into()),
        r#type: Some("Patient".into()),
        identifier: Some(Box::new(Identifier {
            system: Some(listing.system.into()),
            value: Some(listing.value.into()),
            ..Identifier::default()
        })),
        ..Reference::default()
    }
}

/// The logical reference to `organisation`: its first identifier and its
/// name.
pub(super) fn organisation(organisation: &Organisation) -> Reference {
    Reference {
        r#type: Some("Organization".into()),
        identifier: organisation.identifiers.first().map(|(system, value)| {
            Box::new(Identifier {
                system: Some(system.as_str().into()),
                value: Some(value.as_str().into()),
                ..Identifier::default()
            })
        }),
        display: Some(
            organisation
                .name
                .as_deref()
                .unwrap_or(&organisation.id)
                .into(),
        ),
        ..Reference::default()
    }
}

/// Returns the `OperationOutcome` that says the id a search or a read named
/// is no id the gateway holds a record of, so the caller repeats the
/// search that gave it.
#[must_use]
pub fn unknown_id() -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "not-found".into(),
            diagnostics: Some(
                "the id is unknown or has expired; repeat the Patient lookup or the document \
                 search and use the id it answers"
                    .into(),
            ),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

/// Returns the `OperationOutcome` that names a member whose own endpoint,
/// `endpoint`, gave no answer, with its §11.1 status `status`.
#[must_use]
pub fn silent(endpoint: &str, status: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "incomplete".into(),
            diagnostics: Some(
                format!("the list is incomplete: no answer from {endpoint} ({status}) (§11.3)")
                    .into(),
            ),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

/// Returns the `OperationOutcome` that names a member whose own endpoint,
/// `endpoint`, answered with no document the list names.
///
/// Its answer is no FHIR document `Bundle` that carries an identifier with
/// a system and a value (FHIR R4 `bdl-9`), so the list leaves it out and
/// names the member with the §11.1 status `node-error`.
#[must_use]
pub fn unusable(endpoint: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "incomplete".into(),
            diagnostics: Some(
                format!("the list is incomplete: {} (§11.3)", unidentified(endpoint)).into(),
            ),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

/// Returns the sentence that says the member whose own endpoint is
/// `endpoint` answered with no document the face lists or serves.
///
/// The answer is no FHIR document `Bundle` with an identifier that has a
/// system and a value (FHIR R4 `bdl-9`), which makes the member one in
/// `node-error` (§11.1).
#[must_use]
pub fn unidentified(endpoint: &str) -> String {
    format!(
        "{endpoint} (node-error) answered no FHIR document Bundle with an identifier \
         that has a system and a value (bdl-9)"
    )
}

/// One match of the list.
#[derive(Clone)]
pub enum Match {
    /// An entry the gateway writes.
    Written(Box<DocumentReference>),
    /// A member's own `DocumentReference`, its JSON value as the member
    /// holds it but for its attachment URLs ([`readdressed`]).
    Passed(Value),
}

impl std::fmt::Debug for Match {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Written(_) => f.write_str("Written"),
            Self::Passed(_) => f.write_str("Passed"),
        }
    }
}

/// Returns the JSON form of the `DocumentReference` `listed` names, which
/// the search parameters are evaluated on.
///
/// # Errors
///
/// The `serde_json` error when a written `DocumentReference` does not
/// serialise.
pub fn json(listed: &Match) -> Result<Value, serde_json::Error> {
    match listed {
        Match::Written(reference) => {
            serde_json::to_value(reference.as_ref()).map(Value::from_serde_json)
        }
        Match::Passed(body) => Ok(body.clone()),
    }
}

/// Returns the logical id of the `DocumentReference` `listed` names.
#[must_use]
pub fn id(listed: &Match) -> Option<&str> {
    match listed {
        Match::Written(reference) => reference.id.as_deref(),
        Match::Passed(body) => body.get("id").and_then(Value::as_str),
    }
}

/// Returns `document`, a member's `DocumentReference`, with the logical id
/// `id` in place of the member's own.
#[must_use]
pub fn renamed(mut document: Value, id: &str) -> Value {
    if let Value::Object(resource) = &mut document {
        resource.insert("id".to_owned(), Value::String(id.to_owned()));
    }
    document
}

/// Returns the `searchset` of the `matches`, each under its full URL, and
/// the `outcomes` that say what the list misses.
#[must_use]
pub fn searchset(
    matches: Vec<(String, Match)>,
    outcomes: Vec<(String, OperationOutcome)>,
) -> Bundle {
    let matches = matches
        .into_iter()
        .map(|(url, listed)| {
            let resource = match listed {
                Match::Written(reference) => Resource::DocumentReference(reference),
                Match::Passed(body) => Resource::Unknown(UnknownResource {
                    resource_type: RESOURCE.to_owned(),
                    body,
                }),
            };
            (url, resource)
        })
        .collect();
    crate::face::searchset(matches, outcomes)
}

/// The resource type the list names documents by.
pub const RESOURCE: &str = "DocumentReference";

/// Returns each `content.attachment.url` of the `DocumentReference`
/// `document`, in its order.
#[must_use]
pub fn attachment_urls(document: &Value) -> Vec<&str> {
    document
        .get("content")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|content| content.get("attachment")?.get("url")?.as_str())
        .collect()
}

/// Returns `document`, a member's `DocumentReference`, with each
/// `content.attachment.url` replaced by the URL `to` gives for it, the one
/// change made to it, or `None` when `to` gives none for one of them.
pub fn readdressed(document: &Value, mut to: impl FnMut(&str) -> Option<String>) -> Option<Value> {
    let mut readdressed = document.clone();
    let Value::Object(resource) = &mut readdressed else {
        return Some(readdressed);
    };
    let Some(Value::Array(contents)) = resource.get_mut("content") else {
        return Some(readdressed);
    };
    for content in contents {
        let Value::Object(content) = content else {
            continue;
        };
        let Some(Value::Object(attachment)) = content.get_mut("attachment") else {
            continue;
        };
        if let Some(Value::String(url)) = attachment.get_mut("url") {
            *url = to(url)?;
        }
    }
    Some(readdressed)
}

/// Returns the `OperationOutcome` that names a member whose own endpoint
/// answered documents the list cannot pass through.
///
/// The member's own endpoint is `endpoint`, the documents are named
/// `documents`, such as laboratory reports, and `reason` says why; the
/// member has the §11.1 status `node-error`.
#[must_use]
pub fn unread(endpoint: &str, documents: &str, reason: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "incomplete".into(),
            diagnostics: Some(
                format!(
                    "the list is incomplete: {endpoint} (node-error) answered {documents} \
                     the list cannot pass through: {reason} (§11.3)"
                )
                .into(),
            ),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

/// Returns the `OperationOutcome` that names a document the gateway would
/// write from a member's record and cannot, so the list names it
/// unavailable in its place.
///
/// The member's Query API endpoint is `endpoint`, the document is named
/// `document`, such as a vaccination history, and `reason` says why, naming
/// no value the record carries. The member answered, so its §11.1 status is
/// unchanged.
#[must_use]
pub fn unavailable(endpoint: &str, document: &str, reason: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "incomplete".into(),
            diagnostics: Some(
                format!("a {document} of {endpoint} is unavailable: {reason}").into(),
            ),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }
}

#[cfg(test)]
mod tests;
