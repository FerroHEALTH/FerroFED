// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record: what the logging component records "on every access
//! event or group of events" (Regulation (EU) 2025/327 Annex II 3.2).
//!
//! Annex II 3.2 asks for (a) the healthcare provider or other individuals
//! that accessed the data, (b) the specific natural person or persons, (c)
//! the categories of data accessed, (d) the time and date, and (e) the
//! origin or origins of the data. An [`AccessRecord`] carries each, and
//! beside them the data subject whose data were accessed and the purpose of
//! use the access was made for, which Art 9(2) asks the patient be told.
//!
//! A record names the patient and the person who accessed their data, so
//! `Debug` shows neither, nor any id the classification read: a record
//! leaves through an [`AccessSink`](crate::sink::AccessSink) alone.

use std::fmt;
use std::net::IpAddr;

use jiff::Timestamp;
use secrecy::SecretString;

use crate::classify::Classification;
use crate::retention::Retention;

/// What the access did to the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// A query over the data, an ad hoc or a stored one.
    Query,
    /// A read of one resource.
    Read,
    /// The creation of a resource.
    Create,
    /// The update of a resource.
    Update,
    /// The deletion of a resource.
    Delete,
}

/// How the access ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// The data were delivered or written as asked.
    Success,
    /// The access was answered with a failure, or with less than asked.
    MinorFailure,
    /// The access failed on the side that records it.
    SeriousFailure,
}

/// One purpose of use the accessor declared: a code and the system that
/// defines it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Purpose {
    /// The code system, when one is named.
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

/// Who acts behind the token an access was authenticated by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Acting {
    /// The natural person the token's `sub` names.
    Person,
    /// A client application, which the recording system admits to personal
    /// data only for the professional the token names.
    Client,
}

impl Acting {
    /// The code a record writes the acting mode as: `person` or `client`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Client => "client",
        }
    }
}

/// An assurance level of an electronic identification means, as Regulation
/// (EU) No 910/2014 Art 8(2) names them, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssuranceLevel {
    /// Level low, Art 8(2)(a).
    Low,
    /// Level substantial, Art 8(2)(b).
    Substantial,
    /// Level high, Art 8(2)(c).
    High,
}

impl AssuranceLevel {
    /// The code a record writes the level as: `low`, `substantial` or
    /// `high`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Substantial => "substantial",
            Self::High => "high",
        }
    }
}

/// The professional's identification, as the token the access was
/// authenticated by states it.
///
/// `Debug` shows which members are present and none of their values.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Professional {
    /// The professional's name.
    pub name: Option<String>,
    /// The identifier their national authority issued them as a health
    /// professional.
    pub identifier: Option<String>,
}

impl fmt::Debug for Professional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Professional")
            .field("name", &self.name.is_some())
            .field("identifier", &self.identifier.is_some())
            .finish()
    }
}

/// A coded value, such as a professional role: a code and the system that
/// defines it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Coded {
    /// The code system, when one is named.
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

/// What a national contact point for digital health relays with a request.
///
/// The health professional and the healthcare provider of another Member
/// State, as the contact point asserts them (Implementing Regulation (EU)
/// 2026/2099 Art 7 and its Annex Tables 1 and 2). The recording system verified the contact point, never the professional:
/// Art 6(1) and (2) give their identification and authentication to the
/// entity the Member State of the professional lists. `Debug` shows the
/// country and none of the other values.
#[derive(Clone, PartialEq, Eq)]
pub struct Relayed {
    /// The contact point that asserted them, by the issuer of its token.
    pub contact_point: String,
    /// `country_code`: the ISO 3166-1 alpha-2 code of the Member State that
    /// issued the professional's identification data.
    pub country_code: String,
    /// The health professional (Annex Table 1).
    pub professional: RelayedProfessional,
    /// The healthcare provider (Annex Table 2).
    pub provider: RelayedProvider,
}

impl fmt::Debug for Relayed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Relayed")
            .field("country_code", &self.country_code)
            .finish_non_exhaustive()
    }
}

/// The health professional a contact point relays (2026/2099 Annex
/// Table 1).
#[derive(Clone, PartialEq, Eq)]
pub struct RelayedProfessional {
    /// `family_name`.
    pub family_name: String,
    /// `given_name`.
    pub given_name: String,
    /// `hp_identifier`, unique among the identifiers the Member State where
    /// the professional is registered issues.
    pub identifier: String,
    /// `issuing_authority_name`: the agency that issued `hp_identifier`.
    pub issuing_authority: String,
    /// `hp_professional_role`, as the codes the contact point states.
    pub roles: Vec<Coded>,
}

impl fmt::Debug for RelayedProfessional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayedProfessional")
            .field("roles", &self.roles.len())
            .finish_non_exhaustive()
    }
}

/// The healthcare provider a contact point relays (2026/2099 Annex
/// Table 2), where the professional provides the treatment.
#[derive(Clone, PartialEq, Eq)]
pub struct RelayedProvider {
    /// `healthcare_provider_identifier`, unique in the Member State that
    /// issued it.
    pub identifier: String,
    /// `issuing_authority_name`: the agency that issued the identifier.
    pub issuing_authority: String,
    /// `healthcare_provider_name`.
    pub name: String,
    /// `healthcare_provider_address`: the official registered full address.
    pub address: String,
}

impl fmt::Debug for RelayedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayedProvider").finish_non_exhaustive()
    }
}

/// Who accessed the data (Annex II 3.2(a), (b)): the person, as the token
/// the access was authenticated by names them, the client they used, and
/// the provider they act for.
///
/// Each value is the token's, as the recording system verified it: the
/// record infers none, so the assurance level is absent unless the
/// authentication established one.
#[derive(Clone, PartialEq, Eq)]
pub struct Accessor {
    /// The issuer that vouched for the person (`iss`).
    pub issuer: String,
    /// The person (`sub`).
    pub subject: String,
    /// The client application they used (`client_id`).
    pub client_id: String,
    /// The audience the token named the recording system by.
    pub audience: Option<String>,
    /// The healthcare provider or other organisation the person acts for.
    pub provider: Option<String>,
    /// Who acts behind the token: the person, or a client application.
    pub acting: Acting,
    /// The assurance level the authentication reached, when the recording
    /// system established one from the token.
    pub assurance: Option<AssuranceLevel>,
    /// The professional the token names (3.2(b)), who acts in person or
    /// whom a client acts for.
    pub professional: Professional,
    /// Another identification of the person the token states beside its
    /// `sub`, such as a professional number a claim of the deployment's
    /// naming carries.
    pub alt_id: Option<String>,
    /// Every purpose of use the token declares.
    pub purposes: Vec<Purpose>,
    /// The professional and the provider of another Member State a national
    /// contact point relays, marked as asserted by it, when the client is
    /// one (Implementing Regulation (EU) 2026/2099 Art 7).
    pub relayed: Option<Relayed>,
}

impl fmt::Debug for Accessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accessor")
            .field("provider", &self.provider.is_some())
            .field("acting", &self.acting)
            .field("assurance", &self.assurance)
            .field("professional", &self.professional)
            .field("alt_id", &self.alt_id.is_some())
            .field("purposes", &self.purposes)
            .field("relayed", &self.relayed)
            .finish_non_exhaustive()
    }
}

/// The patient an access concerned, by the identifier the request named and
/// the namespace that issued it; `Debug` shows the namespace alone.
#[derive(Debug, Clone)]
pub struct PatientIdentifier {
    /// The issuing namespace.
    pub namespace: String,
    /// The identifier.
    pub value: SecretString,
}

/// Whether the patient behind an `ehr_id` is named, so a search by the
/// patient's identifier finds the access (Art 9(1)).
///
/// `Debug` shows the outcome and how many identifiers were found, never one.
#[derive(Clone)]
#[non_exhaustive]
pub enum PatientLookup {
    /// The request named the patient, so the record names them already.
    RequestNamed,
    /// The identity service holds these identifiers for the patient.
    Found(Vec<PatientIdentifier>),
    /// The identity service holds no identifier for the patient in a
    /// namespace asked.
    NotFound,
    /// The identity service could not be asked, or did not answer.
    Unavailable,
    /// The recording system names no namespace to ask in.
    NotConfigured,
    /// The recording system has no identity service that names a patient
    /// by an `ehr_id`.
    Unsupported,
}

impl PatientLookup {
    /// The outcome as a record writes it: `request-named`, `found`,
    /// `not-found`, `unavailable`, `not-configured` or `unsupported`.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::RequestNamed => "request-named",
            Self::Found(_) => "found",
            Self::NotFound => "not-found",
            Self::Unavailable => "unavailable",
            Self::NotConfigured => "not-configured",
            Self::Unsupported => "unsupported",
        }
    }
}

impl fmt::Debug for PatientLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(found) => write!(f, "Found({})", found.len()),
            other => f.write_str(other.code()),
        }
    }
}

/// One `ehr_id` an access reached, at the endpoint that holds it, with
/// whether the patient it belongs to is named.
#[derive(Clone)]
pub struct EhrAt {
    /// The endpoint.
    pub endpoint: String,
    /// The `ehr_id` at that endpoint.
    pub ehr_id: String,
    /// Whether the patient behind the `ehr_id` is named.
    pub patient: PatientLookup,
}

impl fmt::Debug for EhrAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EhrAt")
            .field("endpoint", &self.endpoint)
            .field("patient", &self.patient)
            .finish_non_exhaustive()
    }
}

/// Whose data an access concerned: the patient the request named, and each
/// `ehr_id` the access reached.
///
/// An access to many patients' data, such as a population query, names none.
#[derive(Clone, Default)]
pub struct DataSubject {
    /// The patient, when the request named one.
    pub patient: Option<PatientIdentifier>,
    /// The `ehr_id`s the access reached.
    pub ehrs: Vec<EhrAt>,
}

impl fmt::Debug for DataSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataSubject")
            .field("patient", &self.patient.is_some())
            .field("ehrs", &self.ehrs.len())
            .finish()
    }
}

/// One origin of the data (Annex II 3.2(e)): an endpoint the access asked,
/// with how it answered.
#[derive(Clone, PartialEq, Eq)]
pub struct Origin {
    /// The endpoint, as the registry names it.
    pub endpoint: String,
    /// The node behind it.
    pub node: Option<String>,
    /// The node's openEHR `system_id`.
    pub system_id: Option<String>,
    /// How the endpoint answered, such as `active` or `node-error`.
    pub status: String,
    /// The rows or objects it contributed, when counted.
    pub rows: Option<usize>,
    /// The categories of what it contributed, when told apart from the
    /// other origins'.
    pub categories: Option<Classification>,
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Origin")
            .field("endpoint", &self.endpoint)
            .field("node", &self.node)
            .field("status", &self.status)
            .field("rows", &self.rows)
            .field("categories", &self.categories)
            .finish_non_exhaustive()
    }
}

/// What the access asked for.
#[derive(Clone)]
pub struct Request {
    /// The id the recording system knows the request by, which its own log
    /// names too.
    pub id: String,
    /// The operation, such as an ITS-REST `operationId`.
    pub operation: String,
    /// The resource the request addressed, when it addressed one.
    pub resource: Option<String>,
    /// The query text, for a query.
    pub query: Option<SecretString>,
    /// The name of the stored query, for a stored-query execution.
    pub stored_query: Option<String>,
    /// The address the request came from, when known.
    pub client_address: Option<IpAddr>,
    /// The identifier the client correlates the request by in its own
    /// log, such as a national contact point's exchange log, when it sent
    /// one.
    pub correlation: Option<String>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("id", &self.id)
            .field("operation", &self.operation)
            .field("query", &self.query.is_some())
            .field("correlation", &self.correlation.is_some())
            .finish_non_exhaustive()
    }
}

/// One access event (Annex II 3.2).
#[derive(Clone)]
pub struct AccessRecord {
    /// What the access did.
    pub action: Action,
    /// When it ended (3.2(d)).
    pub recorded: Timestamp,
    /// How it ended.
    pub outcome: Outcome,
    /// Who accessed the data (3.2(a), (b)), with their purposes of use.
    pub accessor: Accessor,
    /// Whose data.
    pub subject: DataSubject,
    /// The categories of the data accessed (3.2(c)).
    pub categories: Classification,
    /// The rows or objects delivered to the accessor, when counted.
    pub delivered: Option<usize>,
    /// The origins of the data (3.2(e)), in the order the access asked them.
    pub origins: Vec<Origin>,
    /// What the access asked for.
    pub request: Request,
    /// How long the record is kept (Art 9(2), Annex II 3.4).
    pub retention: Retention,
}

impl fmt::Debug for AccessRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessRecord")
            .field("action", &self.action)
            .field("recorded", &self.recorded)
            .field("outcome", &self.outcome)
            .field("accessor", &self.accessor)
            .field("subject", &self.subject)
            .field("categories", &self.categories)
            .field("delivered", &self.delivered)
            .field("origins", &self.origins)
            .field("request", &self.request)
            .field("retention", &self.retention)
            .finish()
    }
}
