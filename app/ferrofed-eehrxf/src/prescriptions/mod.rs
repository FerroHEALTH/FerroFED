// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search of a patient's electronic prescriptions on the FHIR face
//! (Regulation (EU) 2025/327 Art 14(1)(b), Annex II 2.1).
//!
//! `GET {fhir-base}/MedicationRequest?patient.identifier=system|value` is
//! the search-type interaction the HL7 Europe EU Health Data API
//! 1.0.0-ballot `EEHRxF-ResourceAccessProvider` defines for
//! `MedicationRequest`, by the patient. It names the patient by an
//! identifier with its system, as `$summary` and the document list do, so the
//! patient is always the one an identity service confirms (§5.2, N32). Each
//! member that serves prescriptions is asked at its own endpoint, in the HL7
//! Europe Medication Prescription and Dispense 1.0.0 profiles, and each
//! prescription it answers is passed through unchanged with one `Provenance`
//! naming it ([`searchset`]). The member's pages are read by [`page`].
//!
//! The answer is paged by the gateway (FHIR R4 `http.html` §3.1.0.14): a
//! `_count` bounds a page (FHIR R4 search §3.1.1.5.3) and the `next` link
//! carries the position of the following one in [`OFFSET`], a parameter of
//! the gateway's own, so no page state is held between requests. The
//! parameter's name and the order of the matches, member by member in
//! endpoint order and each member's in its own order, are our own design: no
//! specification governs them.
//!
//! The FHIR R4 search and `RESTful` API pages cited here are read from the
//! vendored 4.0.1 copies, `docs/specs/fhir-r4/search.html` and
//! `docs/specs/fhir-r4/http.html`.

pub mod page;
pub mod searchset;

use crate::face::FHIR_JSON;

/// The resource type searched.
pub const RESOURCE: &str = "MedicationRequest";

/// The search parameter that names the patient, chained to its identifier
/// (FHIR R4 search §3.1.1.4.15 Chained parameters).
pub const PATIENT_IDENTIFIER: &str = "patient.identifier";

/// The parameter that bounds a page (FHIR R4 search §3.1.1.5.3).
pub const COUNT: &str = "_count";

/// The gateway's own paging parameter, which a `next` link it wrote sets.
///
/// Its value is the position, among every match, of the first match of the
/// page. FHIR R4 search §3.1.1.6 has a server's own parameter start with a
/// `-`.
pub const OFFSET: &str = "-offset";

/// The `_format` values that ask for FHIR JSON.
const JSON_FORMATS: [&str; 3] = ["json", "application/json", FHIR_JSON];

/// The patient a prescription search names, and the page it asks for.
#[derive(Clone, PartialEq, Eq)]
pub struct SearchRequest {
    system: String,
    value: String,
    count: Option<usize>,
    offset: usize,
}

impl std::fmt::Debug for SearchRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchRequest")
            .field("count", &self.count)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}

/// Why a prescription search is refused, naming no value it carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SearchRefusal {
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
    /// `_count` or `-offset` is no whole number, or is given twice.
    #[error("_count and -offset are each one whole number")]
    Paging,
    /// The search carries a parameter or a modifier the face does not take.
    #[error("the search takes patient.identifier, _count, -offset and _format alone")]
    Parameter,
    /// The `_format` asks for something other than FHIR JSON.
    #[error("the face answers application/fhir+json alone")]
    Format,
}

impl SearchRequest {
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

    /// Returns the most matches a page carries, or `None` when the search
    /// set no `_count`, which answers every match on one page.
    #[must_use]
    pub fn count(&self) -> Option<usize> {
        self.count
    }

    /// Returns the position, among every match, of the page's first match.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Reads the search from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`SearchRefusal`] the search is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, SearchRefusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        let mut identifier = None;
        let (mut count, mut offset) = (None, None);
        for (name, value) in pairs {
            match &*name {
                PATIENT_IDENTIFIER => {
                    // NOTE: FHIR R4 search, several values; a comma separates the values a
                    // parameter ORs, so an identifier holding one names several patients.
                    if value.contains(',') || identifier.replace(value.into_owned()).is_some() {
                        return Err(SearchRefusal::Several);
                    }
                }
                COUNT => once(&mut count, &value)?,
                OFFSET => once(&mut offset, &value)?,
                "_format" => {
                    if !JSON_FORMATS.contains(&&*value) {
                        return Err(SearchRefusal::Format);
                    }
                }
                _ => return Err(SearchRefusal::Parameter),
            }
        }
        let identifier = identifier.ok_or(SearchRefusal::NoPatient)?;
        let (system, value) = identifier
            .split_once('|')
            .ok_or(SearchRefusal::Unqualified)?;
        if system.is_empty() || value.is_empty() {
            return Err(SearchRefusal::Unqualified);
        }
        Ok(Self {
            system: system.to_owned(),
            value: value.to_owned(),
            count,
            offset: offset.unwrap_or_default(),
        })
    }

    /// Returns the query string of the page that starts at `offset`, every
    /// parameter the search used in it, for a `self` or a `next` link (FHIR
    /// R4 search §3.1.1.6).
    #[must_use]
    pub fn page_query(&self, offset: usize) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair(
            PATIENT_IDENTIFIER,
            &format!("{}|{}", self.system, self.value),
        );
        if let Some(count) = self.count {
            query.append_pair(COUNT, &count.to_string());
        }
        if offset > 0 {
            query.append_pair(OFFSET, &offset.to_string());
        }
        query.finish()
    }
}

/// Reads `value` into `slot` as a whole number, once.
fn once(slot: &mut Option<usize>, value: &str) -> Result<(), SearchRefusal> {
    let read = value.parse().map_err(|_unread| SearchRefusal::Paging)?;
    if slot.replace(read).is_some() {
        return Err(SearchRefusal::Paging);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SearchRefusal, SearchRequest};

    #[test]
    fn the_patient_and_the_page_are_read() {
        let read = SearchRequest::from_query(Some(
            "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&_count=2&-offset=4&_format=json",
        ))
        .unwrap();
        assert_eq!(read.system(), "urn:oid:2.999.1");
        assert_eq!(read.value(), "synthetic-1");
        assert_eq!(read.count(), Some(2));
        assert_eq!(read.offset(), 4);
        let plain = SearchRequest::from_query(Some("patient.identifier=urn%3Aoid%3A2.999.1%7Cs-1"))
            .unwrap();
        assert_eq!((plain.count(), plain.offset()), (None, 0));
    }

    #[test]
    fn a_page_query_names_every_parameter_the_search_used() {
        let read = SearchRequest::from_query(Some(
            "patient.identifier=urn%3Aoid%3A2.999.1%7Cs-1&_count=2",
        ))
        .unwrap();
        assert_eq!(
            read.page_query(0),
            "patient.identifier=urn%3Aoid%3A2.999.1%7Cs-1&_count=2"
        );
        assert_eq!(
            read.page_query(2),
            "patient.identifier=urn%3Aoid%3A2.999.1%7Cs-1&_count=2&-offset=2"
        );
    }

    #[test]
    fn every_other_form_is_refused() {
        for (query, refusal) in [
            ("", SearchRefusal::NoPatient),
            ("_count=2", SearchRefusal::NoPatient),
            ("patient.identifier=synthetic-1", SearchRefusal::Unqualified),
            (
                "patient.identifier=%7Csynthetic-1",
                SearchRefusal::Unqualified,
            ),
            ("patient.identifier=a%7Cb,a%7Cc", SearchRefusal::Several),
            (
                "patient.identifier=a%7Cb&patient.identifier=a%7Cc",
                SearchRefusal::Several,
            ),
            ("patient.identifier=a%7Cb&_count=-1", SearchRefusal::Paging),
            ("patient.identifier=a%7Cb&_count=two", SearchRefusal::Paging),
            (
                "patient.identifier=a%7Cb&_count=1&_count=2",
                SearchRefusal::Paging,
            ),
            ("patient.identifier=a%7Cb&-offset=x", SearchRefusal::Paging),
            (
                "patient.identifier=a%7Cb&status=active",
                SearchRefusal::Parameter,
            ),
            ("patient.identifier:missing=true", SearchRefusal::Parameter),
            ("patient=Patient%2F1", SearchRefusal::Parameter),
            (
                "patient.identifier=a%7Cb&_format=xml",
                SearchRefusal::Format,
            ),
        ] {
            assert_eq!(
                SearchRequest::from_query(Some(query)),
                Err(refusal),
                "{query}"
            );
        }
    }

    #[test]
    fn no_debug_shows_the_identifier() {
        let read = SearchRequest::from_query(Some("patient.identifier=a%7Csynthetic-77q")).unwrap();
        assert!(!format!("{read:?}").contains("77q"));
    }
}
