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
//! (`ehds_logging`), and hands it to the sink a binding builds, which for the
//! IHE binding writes a BALP `AuditEvent` to the audit spool. The console
//! reaches the gateway through the same façade, so its queries are recorded
//! with the operator as the caller.
//!
//! An access whose record cannot be stored is refused: the answer is
//! withheld and the caller gets `503 access-unrecorded`, as an IHE
//! transaction whose audit record cannot be stored fails closed. A write the
//! node took before the record failed stays at the node, and the answer says
//! the gateway does not know its outcome. No record content reaches a node,
//! the operator log, a span or a metric label (§5.4, N33): the failure is
//! logged under the gateway's request id with the error chain, which names
//! no value. No specification governs where the record is built: our own
//! design.

pub mod config;
pub(crate) mod routed;

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ehds_logging::classify::{Classification, Evidence};
use ehds_logging::map::CategoryMap;
use ehds_logging::record::{AccessRecord, Accessor, Action, DataSubject, Origin, Outcome, Purpose};
use ehds_logging::sink::AccessSink;
use ferrofed_engine::outbound_id::OutboundId;
use http::{Method, StatusCode};
use openehr_federation::headers;
use openehr_its::rest::routes::{self, Lookup};
use secrecy::SecretString;

use crate::auth::caller::Caller;
use crate::base_path::BasePath;
use crate::client_address::ClientAddress;
use crate::error::{self, Code};
use crate::request_id;
use crate::state::AppState;

/// The access log of one federation: the category map and the sink.
pub struct AccessLog {
    map: CategoryMap,
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
    /// The log that classifies with `map` and stores through `sink`.
    #[must_use]
    pub fn new(map: CategoryMap, sink: Arc<dyn AccessSink>) -> Self {
        Self { map, sink }
    }

    /// The category map.
    #[must_use]
    pub fn map(&self) -> &CategoryMap {
        &self.map
    }
}

/// One origin an access asked, as the handler saw it.
#[derive(Debug, Clone)]
pub(crate) struct Asked {
    /// The endpoint.
    pub(crate) endpoint: String,
    /// The node behind it.
    pub(crate) node: Option<String>,
    /// The node's `system_id`.
    pub(crate) system_id: Option<String>,
    /// How it answered.
    pub(crate) status: String,
    /// What it contributed.
    pub(crate) rows: Option<usize>,
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
                categories: per_origin(&self.log.map, asked, contributing, &categories),
            })
            .collect();
        AccessRecord {
            action: self.action,
            recorded: jiff::Timestamp::now(),
            outcome: outcome(status),
            accessor: accessor(caller),
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
            },
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
/// names, the provider it acts for, and its purposes of use (Annex II
/// 3.2(a), (b)).
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
        professional: requester.map(|requester| requester.professional().to_owned()),
        purposes: caller
            .purposes()
            .iter()
            .map(|purpose| Purpose {
                system: purpose.system.clone(),
                code: purpose.code.clone(),
            })
            .collect(),
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

    /// Whether `method` on `path` is an operation that reaches patient data
    /// at a node: a query execution, a request in the EHR area, the creation
    /// of an EHR, a DEMOGRAPHIC request, or the read of an EHR by subject.
    fn reaches_patient_data(&self, method: &Method, path: &str) -> bool {
        let under_base = if self.base.is_root() {
            Some(path)
        } else {
            path.strip_prefix(self.base.as_str())
        };
        let Some(relative) = under_base.and_then(|path| path.strip_prefix("/v1")) else {
            return false;
        };
        let Lookup::Matched(matched) = routes::lookup(method, relative) else {
            return false;
        };
        (matched.group == QUERY_GROUP && matched.operation_id.starts_with(QUERY_EXECUTION))
            || crate::facade::route::in_ehr_area(&matched)
            || crate::facade::write::creates_ehr(&matched)
            || crate::facade::route::in_demographic_area(&matched)
            || crate::facade::subject::serves(&matched)
    }
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
/// patient-data answer carries data and no access at all.
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
    let reaches = gate.reaches_patient_data(request.method(), request.uri().path());
    let mut response = next.run(request).await;
    let accessed = response.extensions_mut().remove::<Accessed>();
    let declared_none = response.extensions_mut().remove::<NoAccess>().is_some();
    let Some(accessed) = accessed else {
        if reaches && !declared_none && carries_data(&response) && gate.logs() {
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
    let record = accessed.record(&caller, address, &logged, response.status());
    match accessed.log.sink.store(record).await {
        Ok(()) => response,
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = logged,
                "the access could not be recorded, so its answer is withheld"
            );
            error::fixed(Code::AccessUnrecorded, &request_id)
        }
    }
}
