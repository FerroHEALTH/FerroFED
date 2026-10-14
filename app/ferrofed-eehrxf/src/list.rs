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
//! read through the gateway at a URL it signs.

use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::{DocumentReference, DocumentReferenceContent};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;

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
}

impl std::fmt::Debug for ListRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListRequest")
            .field("summaries", &self.summaries)
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

    /// Reads the search from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`ListRefusal`] the search is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, ListRefusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        let mut identifier = None;
        let mut summaries = true;
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
                "type" => summaries &= value.split(',').any(admits_summary),
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
        })
    }
}

/// Whether the token `token` matches the patient summary type: its code
/// alone, the code in the LOINC system, or any code of the LOINC system
/// (FHIR R4 search, the token type).
fn admits_summary(token: &str) -> bool {
    match token.split_once('|') {
        None => token == PATIENT_SUMMARY,
        Some((system, code)) => system == LOINC && (code.is_empty() || code == PATIENT_SUMMARY),
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
        (id, Some((URI_SYSTEM, unique))),
        listing,
        custodian,
        (attachment, Some(format)),
    )
}

/// Returns the entry, with the logical id `id`, of the document a member
/// `custodian` serves itself, as `fetched` holds it, whose own
/// identifier `identifier` names it when the document gives one.
#[must_use]
pub fn native(
    (id, identifier): (&str, Option<(&str, &str)>),
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
    entry((id, identifier), listing, custodian, (attachment, None))
}

/// The entry with the logical id `id` and the master identifier
/// `identifier`, of the content `attachment` in the format `format`.
fn entry(
    (id, identifier): (&str, Option<(&str, &str)>),
    listing: Listing<'_>,
    custodian: &Organisation,
    (attachment, format): (Attachment, Option<Coding>),
) -> DocumentReference {
    DocumentReference {
        id: Some(id.to_owned()),
        meta: Some(Meta {
            profile: vec![DOCUMENT_REFERENCE_PROFILE.into()],
            ..Meta::default()
        }),
        master_identifier: identifier.map(|(system, value)| Identifier {
            system: Some(system.into()),
            value: Some(value.into()),
            ..Identifier::default()
        }),
        status: "current".into(),
        r#type: Some(CodeableConcept {
            coding: vec![Coding {
                system: Some(LOINC.into()),
                code: Some(PATIENT_SUMMARY.into()),
                display: Some("Patient summary Document".into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        }),
        category: vec![CodeableConcept {
            coding: vec![Coding {
                system: Some(CATEGORY_SYSTEM.into()),
                code: Some("Patient-Summaries".into()),
                display: Some("patient summaries".into()),
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

/// Returns the `searchset` of the `matches`, each under its full URL, and
/// the `outcomes` that say what the list misses.
#[must_use]
pub fn searchset(
    matches: Vec<(String, DocumentReference)>,
    outcomes: Vec<(String, OperationOutcome)>,
) -> Bundle {
    let total = u32::try_from(matches.len()).ok();
    let mode = |mode: &str| {
        Some(BundleEntrySearch {
            mode: Some(mode.into()),
            ..BundleEntrySearch::default()
        })
    };
    let found = matches.into_iter().map(|(url, reference)| BundleEntry {
        full_url: Some(url.into()),
        resource: Some(Resource::DocumentReference(Box::new(reference))),
        search: mode("match"),
        ..BundleEntry::default()
    });
    // NOTE: FHIR R4 Bundle `search.mode`; an `outcome` entry informs about the processing
    // of the search, here each member the list misses.
    let missed = outcomes.into_iter().map(|(url, outcome)| BundleEntry {
        full_url: Some(url.into()),
        resource: Some(Resource::OperationOutcome(Box::new(outcome))),
        search: mode("outcome"),
        ..BundleEntry::default()
    });
    Bundle {
        r#type: "searchset".into(),
        total: total.map(Into::into),
        entry: found.chain(missed).collect(),
        ..Bundle::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{ListRefusal, ListRequest};

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
    fn no_debug_shows_the_identifier() {
        let read = ListRequest::from_query(Some("patient.identifier=a%7Csynthetic-77q")).unwrap();
        assert!(!format!("{read:?}").contains("77q"));
    }
}
