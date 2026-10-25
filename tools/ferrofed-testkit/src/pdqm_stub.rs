// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Fixed PDQm answers for the offline suites.
//!
//! The offline suites test the gateway's side of PDQm: what it sends, and
//! what it makes of the Supplier's answer, a match, no match, several
//! patients, a deprecated record or an outage. Each test states the answer,
//! and [`supplier`] serves it on a [`Server`]: every ITI-78
//! `POST [base]/Patient/_search` gets a `searchset` of the given entries with
//! its `total` (IHE PDQm 3.2.0 §2:3.78.4.1.3), and every ITI-119
//! `POST [base]/Patient/$match` gets the same entries, each a `certain`
//! match (§2:3.119.4.2.2.4); every entry has a score of 1. The stub matches
//! nothing and decides nothing: patient identity is FerroPIX's. No specification governs
//! the stub: our own design.

use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch};
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::mock::Server;

/// The path of the ITI-78 search under the stub's FHIR base.
pub const SEARCH: &str = "/fhir/Patient/_search";

/// The path of the ITI-119 match under the stub's FHIR base.
pub const MATCH: &str = "/fhir/Patient/$match";

/// The FHIR JSON media type every answer carries.
const FHIR_JSON: &str = "application/fhir+json";

/// The priority of an identifier's answer, ahead of the empty default.
const KNOWN: u8 = 1;

/// The priority of the empty answer every other request gets.
const UNKNOWN: u8 = 10;

/// The canonical URL of the FHIR `match-grade` extension.
const MATCH_GRADE: &str = "http://hl7.org/fhir/StructureDefinition/match-grade";

/// One synthetic Patient an answer carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its identifiers, each a `(system, value)`.
    identifiers: Vec<(String, String)>,
    /// Whether the record is active; a deprecated one is written with
    /// `active` false.
    active: bool,
    /// Its family and given name, for a header.
    name: Option<(String, String)>,
    /// Its birth date, for a header.
    birth_date: Option<String>,
}

impl Entry {
    /// An active Patient carrying `identifiers`, each a `(system, value)`.
    #[must_use]
    pub fn new(identifiers: &[(&str, &str)]) -> Self {
        Self {
            identifiers: identifiers
                .iter()
                .map(|(system, value)| ((*system).to_owned(), (*value).to_owned()))
                .collect(),
            active: true,
            name: None,
            birth_date: None,
        }
    }

    /// This Patient, deprecated: written with `active` false (§2:3.78.4.1.3
    /// Case 6).
    #[must_use]
    pub fn deprecated(mut self) -> Self {
        self.active = false;
        self
    }

    /// This Patient named `family`, `given`, born on `birth_date` when one is
    /// given.
    #[must_use]
    pub fn named(mut self, (family, given): (&str, &str), birth_date: Option<&str>) -> Self {
        self.name = Some((family.to_owned(), given.to_owned()));
        self.birth_date = birth_date.map(str::to_owned);
        self
    }

    /// The Patient resource, under the id `id`.
    fn patient(&self, id: &str) -> Patient {
        Patient {
            id: Some(id.to_owned()),
            identifier: self
                .identifiers
                .iter()
                .map(|(system, value)| Identifier {
                    system: Some(system.as_str().into()),
                    value: Some(value.as_str().into()),
                    ..Identifier::default()
                })
                .collect(),
            active: (!self.active).then(|| false.into()),
            name: self
                .name
                .iter()
                .map(|(family, given)| HumanName {
                    family: Some(family.as_str().into()),
                    given: vec![given.as_str().into()],
                    ..HumanName::default()
                })
                .collect(),
            birth_date: self.birth_date.as_deref().map(Into::into),
            ..Patient::default()
        }
    }
}

/// Starts a stub Supplier that answers every ITI-78 search and every ITI-119
/// match with `entries`.
pub async fn supplier(entries: &[Entry]) -> Server {
    answering(entries, entries).await
}

/// Starts a stub Supplier that answers every ITI-78 search with `search` and
/// every ITI-119 match with `matched`.
///
/// It serves an answer the two transactions give differently, such as several
/// certain matches under `onlyCertainMatches`, which ITI-119 answers with none
/// (§2:3.119.4.1.3 Case 5).
pub async fn answering(search: &[Entry], matched: &[Entry]) -> Server {
    let server = Server::start().await;
    for (route, entries, graded) in [(SEARCH, search, false), (MATCH, matched, true)] {
        let body = searchset(entries, graded);
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body.into_bytes(), FHIR_JSON))
            .mount(&server)
            .await;
    }
    server
}

/// Starts a stub Supplier with one fixed answer per identifier value.
///
/// A search or a match whose body names the value of an `answers` row gets
/// that row's entries, and any other an empty `searchset`, for a test that
/// asks about more than one patient.
pub async fn by_identifier(answers: &[(&str, &[Entry])]) -> Server {
    let server = Server::start().await;
    for (route, graded) in [(SEARCH, false), (MATCH, true)] {
        for (value, entries) in answers {
            Mock::given(method("POST"))
                .and(path(route))
                .and(body_string_contains(*value))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw(searchset(entries, graded).into_bytes(), FHIR_JSON),
                )
                .with_priority(KNOWN)
                .mount(&server)
                .await;
        }
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(searchset(&[], graded).into_bytes(), FHIR_JSON),
            )
            .with_priority(UNKNOWN)
            .mount(&server)
            .await;
    }
    server
}

/// Returns the FHIR base of `server`, with its trailing slash, that a
/// `[pdqm] url` names.
#[must_use]
pub fn base_url(server: &Server) -> String {
    format!("{}/fhir/", server.uri())
}

/// Returns how many ITI-78 searches `server` received.
pub async fn searches(server: &Server) -> usize {
    received(server, SEARCH).await.len()
}

/// Returns how many ITI-119 matches `server` received.
pub async fn matches(server: &Server) -> usize {
    received(server, MATCH).await.len()
}

/// Returns the body of every search `server` received, then every match:
/// synthetic values only, for a test to see what reached the Supplier.
pub async fn bodies(server: &Server) -> Vec<String> {
    let mut found = received(server, SEARCH).await;
    found.extend(received(server, MATCH).await);
    found
}

/// The bodies of the requests `server` received at `route`, in order.
///
/// # Panics
///
/// Never: `wiremock` records every request unless recording is switched
/// off, and [`supplier`] leaves it on.
#[expect(
    clippy::expect_used,
    reason = "a server started by Server::start records its requests, the wiremock default"
)]
async fn received(server: &Server, route: &str) -> Vec<String> {
    server
        .received_requests()
        .await
        .expect("the stub should record its requests")
        .iter()
        .filter(|request| request.url.path() == route)
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect()
}

/// The `searchset` of `entries`, each with a score of 1 and, when `graded`,
/// the `certain` grade.
#[expect(
    clippy::expect_used,
    reason = "a typed Bundle of strings, identifiers and Patients always serialises"
)]
fn searchset(entries: &[Entry], graded: bool) -> String {
    let entry = entries
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let id = format!("pdq-{}", index.saturating_add(1));
            BundleEntry {
                full_url: Some(
                    format!("http://pdq.example.org/fhir/Patient/{id}")
                        .as_str()
                        .into(),
                ),
                resource: Some(Resource::Patient(Box::new(record.patient(&id)))),
                search: Some(BundleEntrySearch {
                    mode: Some("match".into()),
                    score: Some("1".into()),
                    extension: if graded {
                        vec![Extension {
                            url: MATCH_GRADE.to_owned(),
                            value: Some(ExtensionValue::Code("certain".into())),
                            ..Extension::default()
                        }]
                    } else {
                        Vec::new()
                    },
                    ..BundleEntrySearch::default()
                }),
                ..BundleEntry::default()
            }
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&Bundle {
        r#type: "searchset".into(),
        total: Some(
            u32::try_from(entry.len())
                .expect("a test states a handful of entries")
                .into(),
        ),
        entry,
        ..Bundle::default()
    })
    .expect("the searchset should serialise")
}
