// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document list of the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1): the `DocumentReference` search, and the `searchset` it answers.
//!
//! The search is the HL7 Europe EU Health Data API 1.0.0-ballot Document
//! Access Provider's `DocumentReference` search-type interaction, with its
//! `patient.identifier` and `type` parameters
//! (`CapabilityStatement-EEHRxF-DocumentAccessProvider`). It names the
//! patient by an identifier with its system, as `$summary` does, so the
//! patient is always the one an identity service confirms (§5.2, N32). Each
//! entry follows the `EehrxfMhdDocumentReference` profile: one for the
//! patient summary the gateway writes on request, retrievable through
//! `Patient/$summary`, and one for each document a member serves itself,
//! read through the gateway at a URL it signs. A member that serves its
//! laboratory, discharge or imaging reports as a Document Access Provider
//! has its own `DocumentReference`s listed as it holds them ([`Match::Passed`]), each
//! attachment URL readdressed to the gateway's ([`readdressed`]).

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

use eehrxf::document::laboratory::LAB_BUNDLE;

use crate::face::FHIR_JSON;
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
pub const DISCHARGE_REPORTS: [&str; 2] = ["18842-5", "100719-4"];

/// The LOINC codes of a medical imaging document, the codes of the
/// `eehrxf-document-type-medical-imaging-vs` value set, in its order.
pub const IMAGING_DOCUMENTS: [&str; 2] = ["85430-7", "18748-4"];

/// The canonical URL of the document `Bundle` profile the gateway's own
/// summary follows.
pub const EPS_BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The system of an identifier that is a URI (FHIR R4 `identifier-registry`).
const URI_SYSTEM: &str = "urn:ietf:rfc:3986";

/// The `_format` values that ask for FHIR JSON.
const JSON_FORMATS: [&str; 3] = ["json", "application/json", FHIR_JSON];

/// The patient a document search names, and whether its `type` admits a
/// patient summary.
#[derive(Clone, PartialEq, Eq)]
pub struct ListRequest {
    system: String,
    value: String,
    summaries: bool,
    laboratory: bool,
    discharge: Vec<&'static str>,
    imaging: Vec<&'static str>,
}

impl std::fmt::Debug for ListRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListRequest")
            .field("summaries", &self.summaries)
            .field("laboratory", &self.laboratory)
            .field("discharge", &self.discharge)
            .field("imaging", &self.imaging)
            .finish_non_exhaustive()
    }
}

/// Why a document search is refused, naming no value it carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ListRefusal {
    /// The search names no `patient.identifier`.
    #[error("the search names the patient by patient.identifier=system|value")]
    NoPatient,
    /// `patient.identifier` is given more than once, or names several
    /// patients.
    #[error("the search names one patient")]
    Several,
    /// The identifier has no system or no value.
    #[error("the identifier names its system and its value as system|value")]
    Unqualified,
    /// The search carries a parameter or a modifier the face does not take.
    #[error("the search takes patient.identifier, type and _format alone")]
    Parameter,
    /// The `_format` asks for something other than FHIR JSON.
    #[error("the face answers application/fhir+json alone")]
    Format,
}

impl ListRequest {
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

    /// Reads the search from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`ListRefusal`] the search is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, ListRefusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        let mut identifier = None;
        let mut summaries = true;
        let mut laboratory = true;
        let mut discharge = DISCHARGE_REPORTS.to_vec();
        let mut imaging = IMAGING_DOCUMENTS.to_vec();
        for (name, value) in pairs {
            match &*name {
                "patient.identifier" => {
                    // NOTE: FHIR R4 search, several values; a comma separates the values a
                    // parameter ORs, so an identifier holding one names several patients.
                    if value.contains(',') || identifier.replace(value.into_owned()).is_some() {
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
                    for codes in [&mut discharge, &mut imaging] {
                        codes.retain(|code| value.split(',').any(|token| admits(token, code)));
                    }
                }
                "_format" => {
                    if !JSON_FORMATS.contains(&&*value) {
                        return Err(ListRefusal::Format);
                    }
                }
                _ => return Err(ListRefusal::Parameter),
            }
        }
        let identifier = identifier.ok_or(ListRefusal::NoPatient)?;
        let (system, value) = identifier.split_once('|').ok_or(ListRefusal::Unqualified)?;
        if system.is_empty() || value.is_empty() {
            return Err(ListRefusal::Unqualified);
        }
        Ok(Self {
            system: system.to_owned(),
            value: value.to_owned(),
            summaries,
            laboratory,
            discharge,
            imaging,
        })
    }
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

/// Returns the entry of a laboratory report the gateway writes on read.
///
/// The entry has the logical id `id` and the unique id `unique`, a
/// `urn:uuid:` URN, and names the report of a composition of the member
/// `custodian` at `url`, a URL the gateway signs relative to the FHIR base.
///
/// The document is an HL7 Europe Laboratory Report 2.0.0 `Bundle-eu-lab`,
/// of the `eehrxf-document-type-laboratory-report-vs` type, named by the
/// identifier its `Bundle.identifier` carries when it is read.
#[must_use]
pub fn laboratory(
    (id, unique): (&str, &str),
    listing: Listing<'_>,
    custodian: &Organisation,
    url: &str,
) -> DocumentReference {
    let attachment = Attachment {
        content_type: Some(FHIR_JSON.into()),
        url: Some(url.into()),
        title: Some("Laboratory report written by the gateway from the member's record".into()),
        ..Attachment::default()
    };
    let format = Coding {
        system: Some(URI_SYSTEM.into()),
        code: Some(LAB_BUNDLE.into()),
        ..Coding::default()
    };
    entry(
        (id, (URI_SYSTEM, unique)),
        (LABORATORY, listing),
        custodian,
        (attachment, Some(format)),
    )
}

/// The document type and the priority category an entry names.
#[derive(Debug, Clone, Copy)]
struct Kind {
    /// The LOINC code of the type.
    code: &'static str,
    /// Its display.
    display: &'static str,
    /// The priority category's code.
    category: &'static str,
    /// Its display.
    category_display: &'static str,
}

/// A patient summary.
const SUMMARY: Kind = Kind {
    code: PATIENT_SUMMARY,
    display: "Patient summary Document",
    category: "Patient-Summaries",
    category_display: "patient summaries",
};

/// A laboratory report.
const LABORATORY: Kind = Kind {
    code: LABORATORY_REPORT,
    display: "Laboratory report",
    category: "Laboratory-Reports",
    category_display: "medical test results, including laboratory and other diagnostic results and related reports",
};

/// The entry with the logical id `id` and the master identifier
/// `system`|`value`, of the content `attachment` in the format `format`.
fn entry(
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
        subject: Some(Reference {
            r#type: Some("Patient".into()),
            identifier: Some(Box::new(Identifier {
                system: Some(listing.system.into()),
                value: Some(listing.value.into()),
                ..Identifier::default()
            })),
            ..Reference::default()
        }),
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

/// The logical reference to `organisation`: its first identifier and its
/// name.
fn organisation(organisation: &Organisation) -> Reference {
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

#[cfg(test)]
mod tests {
    use super::{ListRefusal, ListRequest, Match, attachment_urls, readdressed, searchset};
    use fhir_types::codec::Value;

    #[test]
    fn the_patient_and_the_summary_type_are_read() {
        for query in [
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1",
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=http%3A%2F%2Floinc.org%7C60591-5",
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=60591-5",
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=http%3A%2F%2Floinc.org%7C",
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=x%7Cy,60591-5&_format=json",
        ] {
            let read = ListRequest::from_query(Some(query)).unwrap();
            assert_eq!(read.system(), "urn:oid:2.999.1", "{query}");
            assert_eq!(read.value(), "synthetic-1", "{query}");
            assert!(read.admits_summaries(), "{query}");
        }
    }

    #[test]
    fn another_type_admits_no_summary() {
        for query in [
            "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C11502-2",
            "patient.identifier=a%7Cb&type=%7C60591-5",
            "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C60591-5",
            "patient.identifier=a%7Cb&type=60591-5&type=11502-2",
        ] {
            let read = ListRequest::from_query(Some(query)).unwrap();
            assert!(!read.admits_summaries(), "{query}");
        }
    }

    #[test]
    fn every_other_form_is_refused() {
        for (query, refusal) in [
            ("", ListRefusal::NoPatient),
            ("type=60591-5", ListRefusal::NoPatient),
            ("patient.identifier=synthetic-1", ListRefusal::Unqualified),
            (
                "patient.identifier=%7Csynthetic-1",
                ListRefusal::Unqualified,
            ),
            ("patient.identifier=a%7Cb,a%7Cc", ListRefusal::Several),
            (
                "patient.identifier=a%7Cb&patient.identifier=a%7Cc",
                ListRefusal::Several,
            ),
            (
                "patient.identifier=a%7Cb&status=current",
                ListRefusal::Parameter,
            ),
            (
                "patient.identifier=a%7Cb&type:text=summary",
                ListRefusal::Parameter,
            ),
            ("patient=Patient%2F1", ListRefusal::Parameter),
            ("patient.identifier=a%7Cb&_format=xml", ListRefusal::Format),
        ] {
            assert_eq!(
                ListRequest::from_query(Some(query)),
                Err(refusal),
                "{query}"
            );
        }
    }

    #[test]
    fn the_discharge_codes_every_type_admits_are_read_in_order() {
        for (query, codes) in [
            ("patient.identifier=a%7Cb", &["18842-5", "100719-4"][..]),
            (
                "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C",
                &["18842-5", "100719-4"],
            ),
            (
                "patient.identifier=a%7Cb&type=100719-4,http%3A%2F%2Floinc.org%7C18842-5",
                &["18842-5", "100719-4"],
            ),
            ("patient.identifier=a%7Cb&type=100719-4", &["100719-4"]),
            (
                "patient.identifier=a%7Cb&type=18842-5,100719-4&type=18842-5",
                &["18842-5"],
            ),
            ("patient.identifier=a%7Cb&type=60591-5", &[]),
            (
                "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C18842-5",
                &[],
            ),
            ("patient.identifier=a%7Cb&type=18842-5&type=100719-4", &[]),
        ] {
            let read = ListRequest::from_query(Some(query)).unwrap();
            assert_eq!(read.discharge(), codes, "{query}");
        }
    }

    #[test]
    fn the_imaging_codes_every_type_admits_are_read_in_order() {
        for (query, codes) in [
            ("patient.identifier=a%7Cb", &["85430-7", "18748-4"][..]),
            (
                "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C",
                &["85430-7", "18748-4"],
            ),
            ("patient.identifier=a%7Cb&type=18748-4", &["18748-4"]),
            (
                "patient.identifier=a%7Cb&type=18748-4,http%3A%2F%2Floinc.org%7C85430-7",
                &["85430-7", "18748-4"],
            ),
            ("patient.identifier=a%7Cb&type=18842-5", &[]),
            (
                "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C85430-7",
                &[],
            ),
        ] {
            let read = ListRequest::from_query(Some(query)).unwrap();
            assert_eq!(read.imaging(), codes, "{query}");
        }
    }

    #[test]
    fn no_debug_shows_the_identifier() {
        let read = ListRequest::from_query(Some("patient.identifier=a%7Csynthetic-77q")).unwrap();
        assert!(!format!("{read:?}").contains("77q"));
    }

    /// A member's `DocumentReference`, its members in the order a serialiser
    /// writes them, with two attachments and a decimal of two places.
    const MEMBERS: &str = r#"{"content":[{"attachment":{"contentType":"application/fhir+json","size":2048,"url":"Binary/lab-1"}},{"attachment":{"title":"no url"},"format":{"code":"urn:x"}}],"extension":[{"url":"urn:oid:2.999.9","valueDecimal":1.50}],"id":"doc-1","resourceType":"DocumentReference","status":"current"}"#;

    #[test]
    fn each_attachment_url_is_read_in_order() {
        let document: Value = serde_json::from_str(MEMBERS).unwrap();
        assert_eq!(vec!["Binary/lab-1"], attachment_urls(&document));
    }

    #[test]
    fn a_readdressed_document_changes_its_attachment_url_alone() {
        let document: Value = serde_json::from_str(MEMBERS).unwrap();
        let readdressed =
            readdressed(&document, |url| Some(format!("Binary/token-of-{url}"))).unwrap();
        assert_eq!(
            serde_json::to_string(&readdressed).unwrap(),
            MEMBERS.replace("\"Binary/lab-1\"", "\"Binary/token-of-Binary/lab-1\""),
            "every other element byte for byte, the decimal's text kept"
        );
    }

    #[test]
    fn a_url_with_no_readdress_leaves_no_document() {
        let document: Value = serde_json::from_str(MEMBERS).unwrap();
        assert!(readdressed(&document, |_| None).is_none());
    }

    #[test]
    fn a_passed_document_is_listed_as_the_member_holds_it() {
        let document: Value = serde_json::from_str(MEMBERS).unwrap();
        let bundle = searchset(
            vec![("urn:uuid:1".to_owned(), Match::Passed(document))],
            Vec::new(),
        );
        let text = serde_json::to_string(&bundle).unwrap();
        assert!(text.contains(MEMBERS), "{text}");
        assert!(!format!("{:?}", Match::Passed(Value::Null)).contains("doc-1"));
    }
}
