// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log: one record for every access to patient data the gateway
//! intermediates, naming the verified caller (Regulation (EU) 2025/327
//! Annex II 3.2; Federation Tier §13).
//!
//! A federated query, a stored-query execution, a routed read and a routed
//! write each reach a node on behalf of the caller the gateway verified.
//! The handler that served one attaches what it saw to its
//! response: the data subject, the origins, and the model ids of what it
//! delivered, read or wrote. [`record`], the layer inside client
//! authentication, builds the record from those facts, the verified caller
//! and the time, classifies it with the deployment's category map
//! (`ehds_logging`), states how long it is kept (Art 9(2), Annex II 3.4),
//! marks it an emergency access when the verified token declares a purpose
//! of use the deployment names as one (Art 11(5)),
//! and hands it to the sink a binding builds, which for the
//! IHE binding writes a BALP `AuditEvent` to the audit spool. The console
//! reaches the gateway through the same façade, so its queries are recorded
//! with the operator as the caller.
//!
//! An access whose record cannot be stored is refused: the answer is
//! withheld and the caller gets `503 access-unrecorded`, as an IHE
//! transaction whose audit record cannot be stored fails closed. A write
//! cannot be taken back once a member took it, so the place of its record
//! is reserved in the sink before any member is asked: a write whose record
//! has no place is refused with the same `503`, and no member sees it. The
//! reservation is filled with the record of how the write ended, a failed
//! one included. Should that still fail, the member's answer stands, so a
//! client never retries a write that was made, and the loss is raised as the
//! security event `write-unrecorded`. No record content reaches a node,
//! the operator log, a span or a metric label (§5.4, N33): the failure is
//! logged under the gateway's request id with the error chain, which names
//! no value. No specification governs where the record is built: our own
//! design.

pub mod config;
pub(crate) mod patient;
pub(crate) mod routed;
pub(crate) mod version;

use std::collections::BTreeSet;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ehds_logging::classify::{Classification, Evidence};
use ehds_logging::emergency::EmergencyPurposes;
use ehds_logging::label::LabelPolicy;
use ehds_logging::map::CategoryMap;
use ehds_logging::record::{
    AccessRecord, Accessor, Action, Coded, DataSubject, Origin, Outcome, Professional, Purpose,
    RelayedProfessional, RelayedProvider, WrittenVersion,
};
use ehds_logging::retention::RetentionPolicy;
use ehds_logging::sink::{AccessSink, Reservation, SinkError};
use ferrofed_engine::conveyance::caller::{Acting, AssuranceLevel};
use ferrofed_engine::conveyance::relayed::Relayed;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::patient::IdentifierNamespace;
use http::{Method, StatusCode};
use openehr_federation::headers;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::routes::{self, Lookup};
use secrecy::SecretString;

use crate::auth::caller::Caller;
use crate::base_path::BasePath;
use crate::client_address::ClientAddress;
use crate::error::{self, Code};
use crate::metrics::security::Event;
use crate::request_id;
use crate::state::AppState;

/// The access log of one federation: the category map, the retention, the
/// emergency purposes and the sink.
pub struct AccessLog {
    map: CategoryMap,
    retention: RetentionPolicy,
    labels: LabelPolicy,
    emergency: EmergencyPurposes,
    patient_namespaces: Vec<IdentifierNamespace>,
    max_named_ehrs: usize,
    sink: Arc<dyn AccessSink>,
}

impl fmt::Debug for AccessLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessLog")
            .field("digest", &self.map.digest())
            .finish_non_exhaustive()
    }
}

impl AccessLog {
    /// The log that classifies with `map` and stores through `sink`, each
    /// record kept for the three years of Regulation (EU) 2025/327 Art 9(2),
    /// and no access marked an emergency access.
    #[must_use]
    pub fn new(map: CategoryMap, sink: Arc<dyn AccessSink>) -> Self {
        Self {
            map,
            retention: RetentionPolicy::default(),
            labels: LabelPolicy::default(),
            emergency: EmergencyPurposes::default(),
            patient_namespaces: Vec::new(),
            max_named_ehrs: config::AccessLog::default().max_named_ehrs,
            sink,
        }
    }

    /// This log, naming the patient behind an `ehr_id` a request addressed
    /// in each of `namespaces` (Regulation (EU) 2025/327 Art 9(1)).
    #[must_use]
    pub fn with_patient_namespaces(mut self, namespaces: Vec<IdentifierNamespace>) -> Self {
        self.patient_namespaces = namespaces;
        self
    }

    /// This log, the record of a query over many patients' data naming at
    /// most `max` `ehr_id`s from its delivered rows, and stating the query's
    /// scope above it (Regulation (EU) 2025/327 Art 9(1)).
    #[must_use]
    pub fn with_max_named_ehrs(mut self, max: usize) -> Self {
        self.max_named_ehrs = max;
        self
    }

    /// The most `ehr_id`s the record of a query over many patients' data
    /// names.
    #[must_use]
    pub fn max_named_ehrs(&self) -> usize {
        self.max_named_ehrs
    }

    /// This log, each record kept as `retention` says.
    #[must_use]
    pub fn with_retention(mut self, retention: RetentionPolicy) -> Self {
        self.retention = retention;
        self
    }

    /// This log, each record labelled as `labels` says (Regulation (EU)
    /// 2025/327 Annex II 3.4).
    #[must_use]
    pub fn with_labels(mut self, labels: LabelPolicy) -> Self {
        self.labels = labels;
        self
    }

    /// This log, an access whose caller declares one of `emergency` marked
    /// an emergency access (Art 11(5)).
    #[must_use]
    pub fn with_emergency(mut self, emergency: EmergencyPurposes) -> Self {
        self.emergency = emergency;
        self
    }

    /// The category map.
    #[must_use]
    pub fn map(&self) -> &CategoryMap {
        &self.map
    }

    /// The purposes of use that mark an access an emergency access.
    #[must_use]
    pub fn emergency(&self) -> &EmergencyPurposes {
        &self.emergency
    }
}

/// The members whose consent pre-filter denial a routed read set aside for
/// an emergency purpose, attached to its answer for its record.
#[derive(Debug, Clone)]
pub(crate) struct ConsentSetAside(pub(crate) BTreeSet<String>);

/// How the one node a routed request was sent to answered it, as the §11.1
/// status set spells it, attached to its answer for its record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Reached(pub(crate) EndpointStatus);

/// One origin an access asked, as the handler saw it.
#[derive(Debug, Clone)]
pub(crate) struct Asked {
    /// The endpoint.
    pub(crate) endpoint: String,
    /// The node behind it.
    pub(crate) node: Option<String>,
    /// The node's `system_id`.
    pub(crate) system_id: Option<String>,
    /// How it answered, as the §11.1 status set spells it.
    pub(crate) status: String,
    /// What it contributed.
    pub(crate) rows: Option<usize>,
    /// The version a write left there, as its answer named it, or `None`
    /// when the access wrote nothing there.
    pub(crate) version: Option<WrittenVersion>,
    /// Whether it contributed what the access delivered.
    pub(crate) contributed: bool,
    /// What its own answer showed of its data, read before a merge with
    /// other origins' answers, when it contributed.
    pub(crate) evidence: Option<Evidence>,
}

/// What a handler saw of the access it served, attached to its response.
#[derive(Clone)]
pub(crate) struct Accessed {
    /// The log the federation that served the access records to.
    pub(crate) log: Arc<AccessLog>,
    /// What the access did.
    pub(crate) action: Action,
    /// The ITS-REST operation.
    pub(crate) operation: &'static str,
    /// The resource the request addressed.
    pub(crate) resource: Option<String>,
    /// The query text, for a query.
    pub(crate) query: Option<SecretString>,
    /// The stored query executed.
    pub(crate) stored_query: Option<String>,
    /// Whose data.
    pub(crate) subject: DataSubject,
    /// What the access showed of the data.
    pub(crate) evidence: Evidence,
    /// What it delivered, when counted.
    pub(crate) delivered: Option<usize>,
    /// The origins it asked.
    pub(crate) origins: Vec<Asked>,
    /// The members whose consent pre-filter denial was set aside for an
    /// emergency purpose, each asked all the same.
    pub(crate) consent_set_aside: BTreeSet<String>,
}

impl fmt::Debug for Accessed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accessed")
            .field("action", &self.action)
            .field("operation", &self.operation)
            .field("origins", &self.origins.len())
            .finish_non_exhaustive()
    }
}

impl Accessed {
    /// The record of this access by `caller`, from `address`, under the
    /// gateway's `outbound` id, answered with `status`.
    fn record(
        &self,
        caller: &Caller,
        address: Option<IpAddr>,
        outbound: &str,
        status: StatusCode,
    ) -> AccessRecord {
        let categories = self.log.map.classify(&self.evidence);
        let contributing = self
            .origins
            .iter()
            .filter(|asked| asked.contributed)
            .count();
        let origins = self
            .origins
            .iter()
            .map(|asked| Origin {
                endpoint: asked.endpoint.clone(),
                node: asked.node.clone(),
                system_id: asked.system_id.clone(),
                status: asked.status.clone(),
                rows: asked.rows,
                version: asked.version.clone(),
                categories: per_origin(&self.log.map, asked, contributing, &categories),
            })
            .collect();
        let recorded = jiff::Timestamp::now();
        let retention = self.log.retention.retain(
            recorded,
            &categories,
            self.origins.iter().map(|asked| asked.endpoint.as_str()),
        );
        let labels = self.log.labels.label(
            &categories,
            self.origins.iter().map(|asked| asked.endpoint.as_str()),
        );
        let accessor = accessor(caller);
        // NOTE: Regulation (EU) 2025/327 Art 11(5); the mark is read from the purposes the
        // verified token declares alone, never inferred from the query or a node's answer.
        let emergency = self
            .log
            .emergency
            .mark(&accessor.purposes)
            .map(|mark| mark.with_consent_set_aside(self.consent_set_aside.iter().cloned()));
        AccessRecord {
            action: self.action,
            recorded,
            outcome: outcome(status),
            accessor,
            subject: self.subject.clone(),
            categories,
            delivered: self.delivered,
            origins,
            request: ehds_logging::record::Request {
                id: outbound.to_owned(),
                operation: self.operation.to_owned(),
                resource: self.resource.clone(),
                query: self.query.clone(),
                stored_query: self.stored_query.clone(),
                client_address: address,
                correlation: caller.correlation().map(str::to_owned),
            },
            retention,
            labels,
            emergency,
        }
    }
}

/// The categories of `asked`, one of `contributing` origins of what the
/// access delivered, classified with `map`: the access's own `categories`
/// when it alone contributed, those of its own answer when several did, and
/// none when it contributed nothing.
///
/// A merged answer does not say which origin each delivered row came from,
/// so an origin of several is classified by every row it answered with,
/// one a merge then cut by its `LIMIT` or `DISTINCT` included: its set is
/// never short of a category it delivered.
fn per_origin(
    map: &CategoryMap,
    asked: &Asked,
    contributing: usize,
    categories: &Classification,
) -> Option<Classification> {
    if !asked.contributed {
        return None;
    }
    if contributing == 1 {
        return Some(categories.clone());
    }
    asked
        .evidence
        .as_ref()
        .map(|evidence| map.classify(evidence))
}

/// How an access answered with `status` ended: a success, a failure the
/// other party answered or the request was refused with, or one where no
/// answer arrived.
fn outcome(status: StatusCode) -> Outcome {
    if status.is_success() {
        Outcome::Success
    } else if matches!(
        status,
        StatusCode::GATEWAY_TIMEOUT | StatusCode::BAD_GATEWAY | StatusCode::INTERNAL_SERVER_ERROR
    ) {
        Outcome::SeriousFailure
    } else {
        Outcome::MinorFailure
    }
}

/// The accessor `caller` is: the person and the client the verified token
/// names, the provider it acts for, who acts, the professional and the
/// assurance level as authentication verified them, and its purposes of use
/// (Annex II 3.1, 3.2(a), (b)).
///
/// For a caller a national contact point vouched for, the client is the
/// contact point's connector, and the record also names the professional
/// and the provider of another Member State it relays, with their
/// country and issuing authorities, marked as asserted by the contact point
/// (Implementing Regulation (EU) 2026/2099 Art 7, Annex Tables 1 and 2).
fn accessor(caller: &Caller) -> Accessor {
    let requester = caller.requester();
    Accessor {
        issuer: caller.issuer().to_owned(),
        subject: caller.subject().to_owned(),
        client_id: caller.client_id().to_owned(),
        audience: caller.audience().map(str::to_owned),
        provider: requester
            .map(|requester| requester.organisation().to_owned())
            .or_else(|| caller.organisation().map(str::to_owned)),
        // NOTE: Annex II 3.2(b), no specification governs the mapping: our own design; only
        // a caller authentication verified as a natural person is recorded as one.
        acting: if caller.acting() == Acting::Person {
            ehds_logging::record::Acting::Person
        } else {
            ehds_logging::record::Acting::Client
        },
        assurance: caller.assurance().map(|level| match level {
            AssuranceLevel::Low => ehds_logging::record::AssuranceLevel::Low,
            AssuranceLevel::Substantial => ehds_logging::record::AssuranceLevel::Substantial,
            AssuranceLevel::High => ehds_logging::record::AssuranceLevel::High,
        }),
        professional: Professional {
            name: caller.professional().name.clone(),
            identifier: caller.professional().identifier.clone(),
        },
        alt_id: requester.map(|requester| requester.professional().to_owned()),
        purposes: caller
            .purposes()
            .iter()
            .map(|purpose| Purpose {
                system: purpose.system.clone(),
                code: purpose.code.clone(),
            })
            .collect(),
        relayed: caller.relayed().map(relayed),
    }
}

/// What a contact point relays, as the access record names it.
fn relayed(relayed: &Relayed) -> ehds_logging::record::Relayed {
    let professional = &relayed.professional;
    let provider = &relayed.provider;
    ehds_logging::record::Relayed {
        contact_point: relayed.contact_point.clone(),
        country_code: relayed.country_code.clone(),
        professional: RelayedProfessional {
            family_name: professional.family_name.clone(),
            given_name: professional.given_name.clone(),
            identifier: professional.hp_identifier.clone(),
            issuing_authority: professional.issuing_authority_name.clone(),
            roles: professional
                .hp_professional_role
                .iter()
                .map(|role| Coded {
                    system: role.system.clone(),
                    code: role.code.clone(),
                })
                .collect(),
        },
        provider: RelayedProvider {
            identifier: provider.identifier.clone(),
            issuing_authority: provider.issuing_authority_name.clone(),
            name: provider.name.clone(),
            address: provider.address.clone(),
        },
    }
}

/// A handler's statement that the request it answered reached no patient
/// data at a node, such as a query no member was sent: the gate admits its
/// answer with no record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoAccess;

/// The gate every answer passes on its way out.
///
/// It stores the record of the access the answer carries before the answer
/// leaves, and refuses an answer of a patient-data operation that carries
/// data and no access.
#[derive(Debug)]
pub struct Gate {
    state: Arc<AppState>,
    base: BasePath,
}

impl Gate {
    /// The gate over the federation `state` serves, under the base `base`.
    #[must_use]
    pub fn new(state: Arc<AppState>, base: BasePath) -> Self {
        Self { state, base }
    }

    /// Whether the federation `state` serves keeps an access log.
    fn logs(&self) -> bool {
        self.state
            .federation()
            .is_some_and(|federation| federation.access_log().is_some())
    }

    /// What `method` on `path` does with patient data at a node: a query
    /// execution, a request in the EHR area, the creation of an EHR, a
    /// DEMOGRAPHIC request, or the read of an EHR by subject reaches it; a
    /// `POST`, `PUT` or `DELETE` among them but a query, or a document
    /// received on the FHIR face, writes it.
    fn reach(&self, method: &Method, path: &str) -> Reach {
        // NOTE: Regulation (EU) 2025/327 Annex II 3.2: every path under the FHIR face but its
        // capability statement is held to a record, so no variant answers data unrecorded.
        if let Some(face) = self
            .state
            .fhir()
            .and_then(|fhir| crate::fhir::classify(fhir, path))
        {
            return match face {
                crate::fhir::FacePath::Metadata => Reach::None,
                crate::fhir::FacePath::Bundle if *method == Method::POST => Reach::Write,
                _ => Reach::Read,
            };
        }
        let under_base = if self.base.is_root() {
            Some(path)
        } else {
            path.strip_prefix(self.base.as_str())
        };
        let Some(relative) = under_base.and_then(|path| path.strip_prefix("/v1")) else {
            return Reach::None;
        };
        let Lookup::Matched(matched) = routes::lookup(method, relative) else {
            return Reach::None;
        };
        if matched.group == QUERY_GROUP && matched.operation_id.starts_with(QUERY_EXECUTION) {
            return Reach::Read;
        }
        let routed = crate::facade::route::in_ehr_area(&matched)
            || crate::facade::write::creates_ehr(&matched)
            || crate::facade::route::in_demographic_area(&matched)
            || crate::facade::subject::serves(&matched);
        match *method {
            _ if !routed => Reach::None,
            Method::POST | Method::PUT | Method::DELETE => Reach::Write,
            _ => Reach::Read,
        }
    }

    /// The place of the record of a request that `reach` says writes, taken
    /// in the access log's sink before any member is asked; `None` for a
    /// request that writes nothing or a federation that keeps no log.
    async fn reserve(&self, reach: Reach) -> Result<Option<Box<dyn Reservation>>, SinkError> {
        if reach != Reach::Write {
            return Ok(None);
        }
        let Some(log) = self
            .state
            .federation()
            .and_then(|federation| federation.access_log().cloned())
        else {
            return Ok(None);
        };
        log.sink.reserve().await.map(Some)
    }
}

/// What a request does with patient data at a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// It reaches none.
    None,
    /// It reads it.
    Read,
    /// It changes it, which cannot be taken back once a member took it.
    Write,
}

/// The API group of the Query API (ITS-REST).
const QUERY_GROUP: &str = "query";

/// The prefix of every query-execution operation of the Query API.
const QUERY_EXECUTION: &str = "query_execute";

/// Whether `response` carries what a node answered: a success, or an answer
/// that names the endpoint that acted for it (N31).
fn carries_data(response: &Response) -> bool {
    response.status().is_success() || response.headers().contains_key(headers::ENDPOINT)
}

/// Stores the record of the access the answer of `next` carries before the
/// answer leaves.
///
/// The answer is withheld when the record cannot be stored, or when a
/// patient-data answer carries data and no access at all. A write is
/// refused before `next` runs when its record has no place in the sink, and
/// once a member was asked its answer is never withheld.
/// Every answer of the gateway's surface passes here, so no path that
/// returns data can leave without its record.
pub async fn record(State(gate): State<Arc<Gate>>, request: Request, next: Next) -> Response {
    let caller = request.extensions().get::<Caller>().cloned();
    let address = request
        .extensions()
        .get::<ClientAddress>()
        .map(|ClientAddress(address)| *address);
    let logged = request
        .extensions()
        .get::<OutboundId>()
        .map(ToString::to_string)
        .unwrap_or_default();
    let request_id = request_id::of(request.headers())
        .unwrap_or_default()
        .to_owned();
    let reach = gate.reach(request.method(), request.uri().path());
    let reserved = match gate.reserve(reach).await {
        Ok(reserved) => reserved,
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the record of a write has no place in the access log, so no member is asked"
            );
            return error::fixed(Code::AccessUnrecorded, &request_id);
        }
    };
    let mut response = next.run(request).await;
    let accessed = response.extensions_mut().remove::<Accessed>();
    let declared_none = response.extensions_mut().remove::<NoAccess>().is_some();
    let Some(mut accessed) = accessed else {
        if reach != Reach::None && !declared_none && carries_data(&response) && gate.logs() {
            tracing::error!(
                request_id = logged,
                "an answer of a patient-data operation carried no access, so it is withheld"
            );
            return error::fixed(Code::AccessUnrecorded, &request_id);
        }
        return response;
    };
    let Some(caller) = caller else {
        tracing::error!(
            request_id = logged,
            "an access was served with no verified caller, so its answer is withheld"
        );
        return error::fixed(Code::AccessUnrecorded, &request_id);
    };
    if let Some(federation) = gate.state.federation() {
        let namespaces = accessed.log.patient_namespaces.clone();
        patient::name(
            &federation,
            &namespaces,
            &caller.on_behalf(),
            &mut accessed.subject,
        )
        .await;
    }
    let record = accessed.record(&caller, address, &logged, response.status());
    if record.emergency.is_some() {
        tracing::warn!(
            request_id = logged,
            "the access was declared an emergency access by its purpose of use"
        );
    }
    let Some(reserved) = reserved else {
        return match accessed.log.sink.store(record).await {
            Ok(()) => response,
            Err(failure) => {
                tracing::error!(
                    error = %crate::chain(&failure),
                    request_id = logged,
                    "the access could not be recorded, so its answer is withheld"
                );
                error::fixed(Code::AccessUnrecorded, &request_id)
            }
        };
    };
    match reserved.store(record).await {
        Ok(()) => response,
        // NOTE: no specification governs this: our own design; the member took the write, so
        // its answer stands and the lost record is raised to the operator, naming no value.
        Err(failure) => {
            Event::WriteUnrecorded.record();
            tracing::error!(
                target: crate::facade::security::TARGET,
                event = Event::WriteUnrecorded.as_str(),
                error = %crate::chain(&failure),
                request_id = logged,
                "the record of a write a member was asked to make could not be stored in its reserved place; the answer stands"
            );
            response
        }
    }
}
