// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The demographics step over PDQm: the master identity of a patient
//! identifier, asked of a Patient Demographics Supplier with ITI-78 or ITI-119
//! (Annex A §A.2 and §A.7).
//!
//! An identifier issued in one of the configured namespaces is sent to the
//! Supplier in that namespace's identifier system: with ITI-78 as an
//! `identifier` search that asks only for identifiers in the master domain
//! (§2:3.78.4.1.2.3), with ITI-119 as the identifier of the input Patient,
//! with `onlyCertainMatches` set (§2:3.119.4.1.2). The identifier the one
//! matched Patient carries in the master domain is the master identity.
//!
//! - No Patient matched, or the one matched carries no master identifier:
//!   [`Identification::NoMatch`] (§2:3.78.4.1.3 Case 3, §2:3.119.4.1.3
//!   Cases 4, 5 and 7).
//! - More than one Patient matched: [`Ambiguity::SeveralPatients`]. ITI-78
//!   counts them in `Bundle.total` (§2:3.78.4.1.3, Case 1), and ITI-119
//!   returns one entry for each (§2:3.119.4.1.3, Case 2).
//! - The matched Patient carries two master identifiers:
//!   [`Ambiguity::SeveralIdentifiers`].
//! - An ITI-119 match not graded `certain`: [`Ambiguity::Uncertain`].
//!
//! A Patient with `active` set to `false` is a deprecated record, never a
//! match: the Supplier may leave it out (§2:3.78.4.1.3 Case 6,
//! §2:3.119.4.1.3 Case 8), so its presence decides nothing.
//!
//! The identifier reaches the Supplier only, inside `ihe_iti`'s redacting
//! types; nothing here logs it, and no error carries it or the master
//! identifier.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::secret::SecretUrl;
use ihe_iti::balp::AuditRecorder;
use ihe_iti::pdqm::PdqmClient;
use ihe_iti::pdqm::error::{InvalidInput, PdqmError};
use ihe_iti::pdqm::input::MatchInput;
use ihe_iti::pdqm::matches::{MatchGrade, MatchResult, MatchedPatient, SearchResult};
use ihe_iti::pdqm::query::PatientQuery;
use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::fhir::{Authentication, ClientError, Tls};
use crate::ihe::audit::balp::audited_as;
use crate::ihe::iua;
use crate::role::behalf::OnBehalfOf;
use crate::role::demographics::{Ambiguity, Demographics, DemographicsError, Identification};
use crate::role::header::{HeaderAnswer, PatientHeader, PersonName, PostalAddress, Telecom};
use crate::role::patient::{IdentifierNamespace, PatientRef, PatientRefError};

/// The PDQm transaction the gateway asks the Supplier with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transaction {
    /// ITI-78, Mobile Patient Demographics Query: a search by identifier.
    #[default]
    #[serde(rename = "iti-78")]
    Search,
    /// ITI-119, Patient Demographics Match, where the deployment declares
    /// it: `$match` with `onlyCertainMatches`.
    #[serde(rename = "iti-119")]
    Match,
}

impl Transaction {
    /// The transaction's IHE name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "iti-78",
            Self::Match => "iti-119",
        }
    }
}

impl fmt::Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The PDQm Supplier as the configuration names it.
#[derive(Debug)]
pub struct PdqmConfig {
    /// The Supplier's FHIR base URL, which `Debug` shows without its
    /// userinfo.
    pub base: SecretUrl,
    /// How the gateway authenticates to it.
    pub auth: Authentication,
    /// The TLS material it is reached with: a client identity for mutual TLS
    /// and trust roots beside the platform's.
    pub tls: Tls,
    /// The transaction the Supplier is asked with.
    pub transaction: Transaction,
    /// The identifier system of the master domain: the identifier the matched
    /// Patient carries in it is the master identity, resolved in the
    /// namespace of the same name.
    pub master: String,
    /// Each client namespace the Supplier is asked about, mapped to the
    /// identifier system the identifier is sent in.
    pub namespaces: BTreeMap<IdentifierNamespace, String>,
}

/// A PDQm demographics step that cannot be built.
///
/// The errors name a namespace or a key, never an identifier value.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PdqmConfigError {
    /// No namespace is taken to the Supplier, so it would never be asked.
    #[error("the PDQm step names no namespace to ask the Supplier about")]
    NoNamespace,
    /// The master domain is empty.
    #[error("the PDQm master domain is empty")]
    EmptyMaster(#[source] PatientRefError),
    /// The master domain is not an absolute URI.
    #[error("the PDQm master domain is not an absolute URI")]
    Master,
    /// A namespace maps to an identifier system that is not an absolute URI.
    #[error("the identifier system namespace {0} maps to is not an absolute URI")]
    Namespace(IdentifierNamespace),
    /// A namespace taken to the Supplier is the master domain itself, whose
    /// identifiers the cross-reference already resolves.
    #[error("namespace {0} is the PDQm master domain, which the cross-reference resolves")]
    MasterNamespace(IdentifierNamespace),
    /// The Supplier's base URL does not parse as a URL.
    #[error("the PDQm Supplier base URL is not a URL")]
    BaseUrl(#[source] url::ParseError),
    /// The Supplier's base URL is not an `http(s)` URL without a query.
    #[error("the PDQm Supplier base URL is not an http(s) URL without a query or a fragment")]
    Base(#[source] InvalidInput),
    /// The HTTP client could not be built: a credential that forms no
    /// `Authorization` value, or a client the platform refuses.
    #[error("the HTTP client for the PDQm Supplier could not be built")]
    Client(#[source] ClientError),
}

/// Why the Supplier's exchange gave no answer the step can use.
///
/// It carries the client's error, which names no value, a URL or the
/// Supplier's free text.
#[derive(Debug, Error)]
#[error("the PDQm Supplier could not identify the patient")]
struct SupplierFailed(#[source] PdqmError);

/// A matched Patient the Supplier counted but did not return.
#[derive(Debug, Error)]
#[error("the PDQm Supplier counted a match it did not return")]
struct Unreturned;

/// A patient identifier in a namespace the step does not take to the
/// Supplier, which the core never hands it.
#[derive(Debug, Error)]
#[error("the PDQm step does not take this namespace to the Supplier")]
struct Unhandled;

/// The [`Demographics`] step over one PDQm Supplier.
pub struct PdqmDemographics {
    client: PdqmClient,
    transaction: Transaction,
    master: String,
    master_namespace: IdentifierNamespace,
    namespaces: BTreeMap<IdentifierNamespace, String>,
}

impl PdqmDemographics {
    /// Builds the step `config` describes.
    ///
    /// # Errors
    /// A [`PdqmConfigError`] for no namespace, a master domain or a mapped
    /// identifier system that is no absolute URI, a namespace that is the
    /// master domain, a base URL the client refuses, credentials no header
    /// can carry, or an HTTP client that cannot be built.
    pub fn from_config(config: PdqmConfig) -> Result<Self, PdqmConfigError> {
        if config.namespaces.is_empty() {
            return Err(PdqmConfigError::NoNamespace);
        }
        let master_namespace = IdentifierNamespace::new(config.master.as_str())
            .map_err(PdqmConfigError::EmptyMaster)?;
        if !absolute(&config.master) {
            return Err(PdqmConfigError::Master);
        }
        for (namespace, system) in &config.namespaces {
            if !absolute(system) {
                return Err(PdqmConfigError::Namespace(namespace.clone()));
            }
            // NOTE: Annex A §A.2: the step finds the master identity, so an
            // identifier already in the master domain goes to resolution as it is.
            if *namespace == master_namespace || *system == config.master {
                return Err(PdqmConfigError::MasterNamespace(namespace.clone()));
            }
        }
        let (http, authorizer) =
            iua::client(&config.auth, &config.tls).map_err(PdqmConfigError::Client)?;
        let base = Url::parse(config.base.expose()).map_err(PdqmConfigError::BaseUrl)?;
        let mut client = PdqmClient::new(base, http).map_err(PdqmConfigError::Base)?;
        if let Some(authorizer) = authorizer {
            client = client.with_authorizer(authorizer);
        }
        Ok(Self {
            client,
            transaction: config.transaction,
            master: config.master,
            master_namespace,
            namespaces: config.namespaces,
        })
    }

    /// This step, every ITI-78 or ITI-119 exchange of which is recorded
    /// through `recorder` as the PDQm Query or Match Consumer audit record
    /// (§2:3.78.5.1, §2:3.119.5.1.1).
    ///
    /// An exchange whose record the recorder refuses fails with
    /// [`DemographicsError::AuditFailed`].
    #[must_use]
    pub fn audited(mut self, recorder: &Arc<dyn AuditRecorder>) -> Self {
        self.client = self.client.audited(Arc::clone(recorder));
        self
    }

    /// The transaction the Supplier is asked with.
    #[must_use]
    pub fn transaction(&self) -> Transaction {
        self.transaction
    }

    /// The namespace a master identifier is resolved in.
    #[must_use]
    pub fn master_namespace(&self) -> &IdentifierNamespace {
        &self.master_namespace
    }

    /// The one active Patient of `patients`, the Patients the Supplier
    /// matched, every one of them returned, graded `certain` when `certain`
    /// asks for it.
    fn one(
        patients: &[MatchedPatient],
        certain: bool,
    ) -> Result<Option<&MatchedPatient>, Ambiguity> {
        let active: Vec<&MatchedPatient> = patients
            .iter()
            .filter(|matched| {
                matched
                    .patient()
                    .active
                    .as_ref()
                    .and_then(|active| active.value)
                    != Some(false)
            })
            .collect();
        let matched = match active.as_slice() {
            [] => return Ok(None),
            [matched] => *matched,
            _ => return Err(Ambiguity::SeveralPatients),
        };
        if certain && matched.grade() != Some(MatchGrade::Certain) {
            return Err(Ambiguity::Uncertain);
        }
        Ok(Some(matched))
    }

    /// The master identity `patients` name, the Patients the Supplier
    /// matched, every one of them returned.
    fn read(
        &self,
        patients: &[MatchedPatient],
        certain: bool,
    ) -> Result<Option<SecretString>, Ambiguity> {
        let Some(matched) = Self::one(patients, certain)? else {
            return Ok(None);
        };
        let values: BTreeSet<&str> = matched
            .patient()
            .identifier
            .iter()
            .filter(|identifier| {
                identifier
                    .system
                    .as_ref()
                    .and_then(|system| system.value.as_deref())
                    == Some(self.master.as_str())
            })
            .filter_map(|identifier| identifier.value.as_ref()?.value.as_deref())
            .filter(|value| !value.is_empty())
            .collect();
        let mut values = values.into_iter();
        match (values.next(), values.next()) {
            (None, _) => Ok(None),
            (Some(value), None) => Ok(Some(SecretString::from(value))),
            (Some(_), Some(_)) => Err(Ambiguity::SeveralIdentifiers),
        }
    }

    /// The identification an ITI-78 first page `page` gives.
    fn searched(&self, page: &SearchResult) -> Identification {
        match counted(page) {
            Ok(patients) => self.identified(self.read(patients, false)),
            Err(Uncounted::Several) => Identification::Ambiguous(Ambiguity::SeveralPatients),
            Err(Uncounted::Unreturned) => {
                Identification::Unavailable(DemographicsError::Backend(Box::new(Unreturned)))
            }
        }
    }

    /// The identification an ITI-119 answer `found` gives.
    fn matched(&self, found: &MatchResult) -> Identification {
        self.identified(self.read(found.patients(), true))
    }

    /// The identifier system the Supplier is asked about an identifier of
    /// `namespace` in, for a header: the system the namespace is mapped to,
    /// the master domain, or the namespace itself when it is an absolute
    /// URI, as a FHIR identifier system is.
    fn system_of<'a>(&'a self, namespace: &'a IdentifierNamespace) -> Option<&'a str> {
        if let Some(system) = self.namespaces.get(namespace) {
            return Some(system);
        }
        if *namespace == self.master_namespace {
            return Some(&self.master);
        }
        absolute(namespace.as_str()).then_some(namespace.as_str())
    }

    fn identified(&self, read: Result<Option<SecretString>, Ambiguity>) -> Identification {
        match read {
            Ok(None) => Identification::NoMatch,
            Ok(Some(value)) => match PatientRef::new(self.master_namespace.clone(), value) {
                Ok(master) => Identification::Identified(master),
                Err(_empty) => Identification::NoMatch,
            },
            Err(ambiguity) => Identification::Ambiguous(ambiguity),
        }
    }
}

impl fmt::Debug for PdqmDemographics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdqmDemographics")
            .field("client", &self.client)
            .field("transaction", &self.transaction)
            .field("master", &self.master)
            .field("namespaces", &self.namespaces.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Demographics for PdqmDemographics {
    fn handles(&self, namespace: &IdentifierNamespace) -> bool {
        self.namespaces.contains_key(namespace)
    }

    async fn identify(
        &self,
        patient: &PatientRef,
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Identification {
        let Some(system) = self.namespaces.get(patient.namespace()) else {
            return Identification::Unavailable(DemographicsError::Backend(Box::new(Unhandled)));
        };
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Identification::Unavailable(DemographicsError::DeadlineExceeded);
        }
        let value = SecretString::from(patient.value());
        let audited = audited_as(on_behalf);
        match self.transaction {
            Transaction::Search => {
                let query = PatientQuery::new()
                    .identifier(Some(system), &value)
                    .and_then(|query| query.identifier_domains(&[self.master.as_str()]));
                let query = match query {
                    Ok(query) => query,
                    Err(refused) => return refused_input(refused),
                };
                match self.client.search(&query, &audited, timeout).await {
                    Ok(page) => self.searched(&page),
                    Err(error) => unavailable(error),
                }
            }
            Transaction::Match => {
                let input = match MatchInput::new(system, &value) {
                    Ok(input) => input.only_certain_matches(true),
                    Err(refused) => return refused_input(refused),
                };
                match self.client.match_patient(&input, &audited, timeout).await {
                    Ok(found) => self.matched(&found),
                    Err(error) => unavailable(error),
                }
            }
        }
    }

    async fn header(
        &self,
        patient: &PatientRef,
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> HeaderAnswer {
        let Some(system) = self.system_of(patient.namespace()) else {
            return HeaderAnswer::NotHandled;
        };
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return HeaderAnswer::Unavailable(DemographicsError::DeadlineExceeded);
        }
        let value = SecretString::from(patient.value());
        let audited = audited_as(on_behalf);
        match self.transaction {
            Transaction::Search => {
                let query = match PatientQuery::new().identifier(Some(system), &value) {
                    Ok(query) => query,
                    Err(refused) => {
                        return HeaderAnswer::Unavailable(DemographicsError::Backend(Box::new(
                            refused,
                        )));
                    }
                };
                match self.client.search(&query, &audited, timeout).await {
                    Ok(page) => match counted(&page) {
                        Ok(patients) => headed(Self::one(patients, false)),
                        Err(Uncounted::Several) => {
                            HeaderAnswer::Ambiguous(Ambiguity::SeveralPatients)
                        }
                        Err(Uncounted::Unreturned) => HeaderAnswer::Unavailable(
                            DemographicsError::Backend(Box::new(Unreturned)),
                        ),
                    },
                    Err(error) => HeaderAnswer::Unavailable(failure(error)),
                }
            }
            Transaction::Match => {
                let input = match MatchInput::new(system, &value) {
                    Ok(input) => input.only_certain_matches(true),
                    Err(refused) => {
                        return HeaderAnswer::Unavailable(DemographicsError::Backend(Box::new(
                            refused,
                        )));
                    }
                };
                match self.client.match_patient(&input, &audited, timeout).await {
                    Ok(found) => headed(Self::one(found.patients(), true)),
                    Err(error) => HeaderAnswer::Unavailable(failure(error)),
                }
            }
        }
    }
}

/// Why an ITI-78 first page names no set of Patients to choose from.
enum Uncounted {
    /// It counts several matches.
    Several,
    /// It counts one match it does not return.
    Unreturned,
}

/// The Patients an ITI-78 first page `page` holds, when it holds every
/// match it counts.
fn counted(page: &SearchResult) -> Result<&[MatchedPatient], Uncounted> {
    let Ok(total) = usize::try_from(page.total()) else {
        return Err(Uncounted::Several);
    };
    if total == 0 {
        return Ok(&[]);
    }
    // NOTE: §2:3.78.4.1.3 Case 1, `total` counts every match; one the page
    // does not hold may be active, so several counted are several matched.
    if page.patients().len() < total {
        return Err(if total > 1 {
            Uncounted::Several
        } else {
            Uncounted::Unreturned
        });
    }
    Ok(page.patients())
}

/// The header answer the one Patient `read` from the Supplier's matches
/// gives.
fn headed(read: Result<Option<&MatchedPatient>, Ambiguity>) -> HeaderAnswer {
    match read {
        Ok(Some(matched)) => HeaderAnswer::Found(Box::new(header_of(matched))),
        Ok(None) => HeaderAnswer::NoMatch,
        Err(ambiguity) => HeaderAnswer::Ambiguous(ambiguity),
    }
}

/// The value of a FHIR primitive `$element`, an `Option` of one, when it
/// has a non-empty one.
macro_rules! value {
    ($element:expr) => {
        $element
            .as_ref()
            .and_then(|element| element.value.clone())
            .filter(|value| !value.is_empty())
    };
}

/// The non-empty values of the FHIR primitives `$elements`, in order.
macro_rules! values {
    ($elements:expr) => {
        $elements
            .iter()
            .filter_map(|element| element.value.clone())
            .filter(|value| !value.is_empty())
            .collect()
    };
}

/// What `matched`, a PDQm Patient, holds of the summary header: its names,
/// birth date, gender, addresses and telecoms, each as the Supplier gives
/// it, and nothing else.
fn header_of(matched: &MatchedPatient) -> PatientHeader {
    let patient = matched.patient();
    PatientHeader {
        names: patient
            .name
            .iter()
            .map(|name| PersonName {
                purpose: value!(name.r#use),
                text: value!(name.text),
                family: value!(name.family),
                given: values!(name.given),
                prefix: values!(name.prefix),
                suffix: values!(name.suffix),
            })
            .collect(),
        birth_date: value!(patient.birth_date),
        gender: value!(patient.gender),
        addresses: patient
            .address
            .iter()
            .map(|address| PostalAddress {
                purpose: value!(address.r#use),
                text: value!(address.text),
                lines: values!(address.line),
                city: value!(address.city),
                district: value!(address.district),
                state: value!(address.state),
                postal_code: value!(address.postal_code),
                country: value!(address.country),
            })
            .collect(),
        telecoms: patient
            .telecom
            .iter()
            .map(|telecom| Telecom {
                system: value!(telecom.system),
                value: value!(telecom.value),
                purpose: value!(telecom.r#use),
            })
            .collect(),
    }
}

/// The identification of an identifier the client refuses before sending:
/// one the configuration checked, so only an empty value reaches here.
fn refused_input(refused: InvalidInput) -> Identification {
    Identification::Unavailable(DemographicsError::Backend(Box::new(refused)))
}

/// The identification of an exchange that failed with `error`.
fn unavailable(error: PdqmError) -> Identification {
    Identification::Unavailable(failure(error))
}

/// The demographics failure of an exchange that failed with `error`.
fn failure(error: PdqmError) -> DemographicsError {
    match error {
        PdqmError::Timeout => DemographicsError::DeadlineExceeded,
        PdqmError::Audit(_) => DemographicsError::AuditFailed(Box::new(SupplierFailed(error))),
        PdqmError::Rejected { status, .. } => DemographicsError::Answered {
            status,
            source: Box::new(SupplierFailed(error)),
        },
        PdqmError::DomainNotRecognized => DemographicsError::Answered {
            status: http::StatusCode::NOT_FOUND,
            source: Box::new(SupplierFailed(error)),
        },
        other => DemographicsError::Backend(Box::new(SupplierFailed(other))),
    }
}

/// Whether `text` is an absolute URI.
fn absolute(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_whitespace) && Url::parse(text).is_ok()
}
