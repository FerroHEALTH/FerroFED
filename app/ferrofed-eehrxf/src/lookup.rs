// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient lookup of the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1): the `Patient` search by identifier, and the `searchset` it answers.
//!
//! Xt-EHR D5.1 §3.4.6 `api-producer-patient` has the producer "offer a
//! patient lookup API sufficient for a consumer to unambiguously identify the
//! patient EHR in the producer's system given patient identification data
//! (demographics or other forms of patient identity) as search parameters".
//! The search takes the patient's identifier with its system alone, the
//! FHIR R4 `Patient` search parameter `identifier`, so the patient is always
//! the one an identity service confirms (§5.2, N32), and a lookup by
//! demographics is refused (§5.4.3). The one match is the `Patient` the
//! identity binding's demographics service holds
//! ([`crate::summary::header_patient`]).

use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch};
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;

use crate::face::FHIR_JSON;

/// The canonical URL of the FHIR R4 `Patient` search parameter
/// `identifier`.
pub const IDENTIFIER_PARAMETER: &str = "http://hl7.org/fhir/SearchParameter/Patient-identifier";

/// The `_format` values that ask for FHIR JSON.
const JSON_FORMATS: [&str; 3] = ["json", "application/json", FHIR_JSON];

/// The patient a lookup names.
#[derive(Clone, PartialEq, Eq)]
pub struct LookupRequest {
    system: String,
    value: String,
}

impl std::fmt::Debug for LookupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookupRequest").finish_non_exhaustive()
    }
}

/// Why a lookup is refused, naming no value it carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LookupRefusal {
    /// The search names no `identifier`.
    #[error(
        "the Patient search names the patient by identifier=system|value, the one search parameter it takes"
    )]
    NoIdentifier,
    /// `identifier` is given more than once, or names several patients.
    #[error("the Patient search names one patient")]
    Several,
    /// The identifier has no system or no value.
    #[error("the identifier names its system and its value as system|value")]
    Unqualified,
    /// The search carries a parameter or a modifier the face does not take,
    /// such as a demographic one.
    #[error(
        "the Patient search takes identifier=system|value and _format alone; a lookup by demographics is not served"
    )]
    Parameter,
    /// The `_format` asks for something other than FHIR JSON.
    #[error("the face answers application/fhir+json alone")]
    Format,
}

impl LookupRequest {
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

    /// Reads the lookup from the query string `query` of a `GET`.
    ///
    /// # Errors
    ///
    /// The [`LookupRefusal`] the lookup is answered with.
    pub fn from_query(query: Option<&str>) -> Result<Self, LookupRefusal> {
        let pairs = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes());
        let mut identifier = None;
        for (name, value) in pairs {
            match &*name {
                "identifier" => {
                    // NOTE: FHIR R4 search, several values; a comma separates the values a
                    // parameter ORs, so an identifier holding one names several patients.
                    if value.contains(',') || identifier.replace(value.into_owned()).is_some() {
                        return Err(LookupRefusal::Several);
                    }
                }
                "_format" => {
                    if !JSON_FORMATS.contains(&&*value) {
                        return Err(LookupRefusal::Format);
                    }
                }
                _ => return Err(LookupRefusal::Parameter),
            }
        }
        let identifier = identifier.ok_or(LookupRefusal::NoIdentifier)?;
        let (system, value) = identifier
            .split_once('|')
            .ok_or(LookupRefusal::Unqualified)?;
        if system.is_empty() || value.is_empty() {
            return Err(LookupRefusal::Unqualified);
        }
        Ok(Self {
            system: system.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// Returns the `searchset` of the `matches`, each under its full URL, and
/// the `outcomes` that say what the lookup misses.
#[must_use]
pub fn searchset(
    matches: Vec<(String, Patient)>,
    outcomes: Vec<(String, OperationOutcome)>,
) -> Bundle {
    let total = u32::try_from(matches.len()).ok();
    let mode = |mode: &str| {
        Some(BundleEntrySearch {
            mode: Some(mode.into()),
            ..BundleEntrySearch::default()
        })
    };
    let found = matches.into_iter().map(|(url, patient)| BundleEntry {
        full_url: Some(url.into()),
        resource: Some(Resource::Patient(Box::new(patient))),
        search: mode("match"),
        ..BundleEntry::default()
    });
    // NOTE: FHIR R4 Bundle `search.mode`; an `outcome` entry informs about the processing
    // of the search, here each member the lookup could not resolve the patient at.
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

/// Returns the `OperationOutcome` that names a member whose Query API
/// endpoint is `endpoint`, at which the patient could not be resolved
/// (§11.1 `not-resolved`).
#[must_use]
pub fn unresolved(endpoint: &str) -> OperationOutcome {
    OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: "incomplete".into(),
            diagnostics: Some(
                format!(
                    "the lookup is incomplete: the patient could not be resolved at {endpoint} (not-resolved) (§11.3)"
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
    use super::{LookupRefusal, LookupRequest};

    #[test]
    fn an_identifier_with_its_system_names_the_patient() {
        for query in [
            "identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1",
            "identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&_format=json",
            "_format=application%2Ffhir%2Bjson&identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1",
        ] {
            let read = LookupRequest::from_query(Some(query)).unwrap();
            assert_eq!(read.system(), "urn:oid:2.999.1", "{query}");
            assert_eq!(read.value(), "synthetic-1", "{query}");
        }
    }

    #[test]
    fn every_other_form_is_refused() {
        for (query, refusal) in [
            ("", LookupRefusal::NoIdentifier),
            ("_format=json", LookupRefusal::NoIdentifier),
            ("family=Synthetic", LookupRefusal::Parameter),
            (
                "family=Synthetic&birthdate=1970-01-01",
                LookupRefusal::Parameter,
            ),
            ("name=Synthetic&gender=female", LookupRefusal::Parameter),
            (
                "identifier=a%7Cb&family=Synthetic",
                LookupRefusal::Parameter,
            ),
            ("identifier:of-type=a%7Cb%7Cc", LookupRefusal::Parameter),
            ("_id=synthetic-1", LookupRefusal::Parameter),
            ("identifier=synthetic-1", LookupRefusal::Unqualified),
            ("identifier=%7Csynthetic-1", LookupRefusal::Unqualified),
            ("identifier=urn:x%7C", LookupRefusal::Unqualified),
            ("identifier=a%7Cb&identifier=a%7Cc", LookupRefusal::Several),
            ("identifier=a%7Cb,a%7Cc", LookupRefusal::Several),
            ("identifier=a%7Cb&_format=xml", LookupRefusal::Format),
        ] {
            assert_eq!(
                LookupRequest::from_query(Some(query)),
                Err(refusal),
                "{query}"
            );
        }
    }

    #[test]
    fn a_refusal_names_the_supported_parameter_and_no_value() {
        for refusal in [LookupRefusal::NoIdentifier, LookupRefusal::Parameter] {
            assert!(
                refusal.to_string().contains("identifier=system|value"),
                "{refusal}"
            );
        }
        let refused = LookupRequest::from_query(Some("family=SYNTHETIC-9z")).unwrap_err();
        assert!(!refused.to_string().contains("SYNTHETIC-9z"));
    }

    #[test]
    fn no_debug_shows_the_identifier() {
        let read = LookupRequest::from_query(Some("identifier=a%7Csynthetic-77q")).unwrap();
        assert!(!format!("{read:?}").contains("77q"));
    }

    #[test]
    fn the_searchset_counts_its_matches_and_not_its_outcomes() {
        let bundle = super::searchset(
            vec![(
                String::from("urn:uuid:1"),
                fhir_types::r4::patient::Patient::default(),
            )],
            vec![(String::from("urn:uuid:2"), super::unresolved("node-b-pub"))],
        );
        let json = serde_json::to_value(&bundle).unwrap();
        assert_eq!(json["type"], "searchset");
        assert_eq!(json["total"], 1);
        assert_eq!(json["entry"][0]["search"]["mode"], "match");
        assert_eq!(json["entry"][0]["resource"]["resourceType"], "Patient");
        assert_eq!(json["entry"][1]["search"]["mode"], "outcome");
        assert!(
            json["entry"][1]["resource"]["issue"][0]["diagnostics"]
                .as_str()
                .unwrap()
                .contains("node-b-pub (not-resolved)")
        );
    }
}
