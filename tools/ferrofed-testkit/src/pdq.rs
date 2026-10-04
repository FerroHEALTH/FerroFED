// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness PDQm Supplier: a test device that answers ITI-78 searches by
//! identifier and ITI-119 matches from synthetic `Patient`s a test adds
//! (Federation Tier with AQL §16.1; IHE PDQm 3.2.0).
//!
//! **This is a test device, not a PDQm implementation.** It plays the Patient
//! Demographics Supplier the gateway's demographics step asks, so a test can
//! read real ITI-78 and ITI-119 answers without a container (no specification
//! governs this: our own design). It matches on identifiers alone, which is
//! all the gateway sends, and decides nothing a Supplier decides on
//! demographics. Every identifier it holds is in the `urn:oid:2.999` example
//! arc: [`PdqSupplier::add`] refuses any other system.
//!
//! - **ITI-78:** `POST [base]/Patient/_search` with a form body. Each
//!   `identifier=<system>|<value>` must be carried by a match; each
//!   `identifier=<system>|,…` names the domains whose identifiers are
//!   returned (§2:3.78.4.1.2.3). The answer is a `searchset` with `total`
//!   (§2:3.78.4.1.3 Cases 1 to 3), and a `404` `not-found` warning when a
//!   named domain is one no Patient carries (Case 4). A deprecated Patient is
//!   returned with `active` false (Case 6).
//! - **ITI-119:** `POST [base]/Patient/$match` with a `Parameters` body. A
//!   Patient carrying any input identifier is a `certain` match with a score
//!   of 1 (§2:3.119.4.2.2.4); with `onlyCertainMatches` set, several matches
//!   are the zero result set of Case 5, and `count` bounds the answer
//!   (Case 6).
//!
//! The device's `Debug` shows counts only, and nothing here logs a value.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::Bytes;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch};
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameterValue};
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;
use http::header::{CONTENT_TYPE, HeaderValue};
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The FHIR base path the device serves under.
const BASE: &str = "/fhir";

/// The FHIR JSON media type (ITI TF-2 Appendix Z.6).
const FHIR_JSON: &str = "application/fhir+json";

/// The example arc every identifier system the device holds lies under.
pub const EXAMPLE_ARC: &str = "urn:oid:2.999";

/// The canonical URL of the FHIR `match-grade` extension.
const MATCH_GRADE: &str = "http://hl7.org/fhir/StructureDefinition/match-grade";

/// The harness PDQm Supplier could not start or take a Patient.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PdqError {
    /// No loopback port could be bound or read back.
    #[error("the PDQm Supplier could not bind a loopback port")]
    Bind(#[source] std::io::Error),
    /// A Patient carries no identifier, or one with an empty value.
    #[error("a synthetic Patient carries at least one identifier, each with a value")]
    Empty,
    /// An identifier system is outside the `urn:oid:2.999` example arc.
    #[error("an identifier system is outside the urn:oid:2.999 example arc")]
    OutsideExampleArc,
}

/// One identifier: its system and its value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DomainId {
    system: String,
    value: String,
}

/// One synthetic Patient the device holds.
#[derive(Debug, Clone)]
struct Record {
    id: String,
    identifiers: BTreeSet<DomainId>,
    active: bool,
}

impl Record {
    /// The Patient resource, with only the identifiers in `domains`, or all
    /// of them when `domains` is empty.
    fn patient(&self, domains: &BTreeSet<String>) -> Patient {
        Patient {
            id: Some(self.id.clone()),
            identifier: self
                .identifiers
                .iter()
                .filter(|id| domains.is_empty() || domains.contains(&id.system))
                .map(|id| Identifier {
                    system: Some(id.system.as_str().into()),
                    value: Some(id.value.as_str().into()),
                    ..Identifier::default()
                })
                .collect(),
            active: (!self.active).then(|| false.into()),
            ..Patient::default()
        }
    }
}

/// What the device holds and what it was asked.
#[derive(Debug, Default)]
struct State {
    patients: BTreeMap<String, Record>,
    next: u64,
    searches: usize,
    matches: usize,
    bodies: Vec<String>,
}

impl State {
    fn domains(&self) -> BTreeSet<&str> {
        self.patients
            .values()
            .flat_map(|record| record.identifiers.iter().map(|id| id.system.as_str()))
            .collect()
    }
}

/// The state the accept loop shares.
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The harness PDQm Supplier on a loopback port, stopped when it is dropped.
pub struct PdqSupplier {
    origin: String,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl fmt::Debug for PdqSupplier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.shared.lock();
        f.debug_struct("PdqSupplier")
            .field("origin", &self.origin)
            .field("patients", &state.patients.len())
            .field("searches", &state.searches)
            .field("matches", &state.matches)
            .finish_non_exhaustive()
    }
}

impl PdqSupplier {
    /// Starts the device on a free loopback port, holding no Patient.
    ///
    /// # Errors
    ///
    /// Returns [`PdqError::Bind`] when no loopback port can be bound.
    pub async fn start() -> Result<Self, PdqError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(PdqError::Bind)?;
        let address = listener.local_addr().map_err(PdqError::Bind)?;
        let shared = Arc::new(Shared::default());
        let task = tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        Ok(Self {
            origin: format!("http://{address}"),
            shared,
            task,
        })
    }

    /// Adds a synthetic Patient carrying `identifiers`, each a
    /// `(system, value)`, deprecated when `active` is `false`, and returns its
    /// resource id.
    ///
    /// # Errors
    ///
    /// Returns [`PdqError::Empty`] for no identifier or an empty value, and
    /// [`PdqError::OutsideExampleArc`] for a system outside `urn:oid:2.999`.
    pub fn add(&self, identifiers: &[(&str, &str)], active: bool) -> Result<String, PdqError> {
        if identifiers.is_empty() || identifiers.iter().any(|(_, value)| value.is_empty()) {
            return Err(PdqError::Empty);
        }
        if identifiers
            .iter()
            .any(|(system, _)| !in_example_arc(system))
        {
            return Err(PdqError::OutsideExampleArc);
        }
        let mut state = self.shared.lock();
        state.next = state.next.saturating_add(1);
        let id = format!("pdq-{}", state.next);
        state.patients.insert(
            id.clone(),
            Record {
                id: id.clone(),
                identifiers: identifiers
                    .iter()
                    .map(|(system, value)| DomainId {
                        system: (*system).to_owned(),
                        value: (*value).to_owned(),
                    })
                    .collect(),
                active,
            },
        );
        Ok(id)
    }

    /// Returns the FHIR base URL a PDQm client is pointed at, with its
    /// trailing slash: `http://127.0.0.1:<port>/fhir/`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("{}{BASE}/", self.origin)
    }

    /// Returns the origin the device listens on, with no path.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns how many ITI-78 searches the device has received.
    #[must_use]
    pub fn searches(&self) -> usize {
        self.shared.lock().searches
    }

    /// Returns how many ITI-119 matches the device has received.
    #[must_use]
    pub fn matches(&self) -> usize {
        self.shared.lock().matches
    }

    /// Returns the body of every request the device has received, in order:
    /// synthetic values only, for a test to see what reached the Supplier.
    #[must_use]
    pub fn bodies(&self) -> Vec<String> {
        self.shared.lock().bodies.clone()
    }
}

impl Drop for PdqSupplier {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Whether `system` lies under the `urn:oid:2.999` example arc.
fn in_example_arc(system: &str) -> bool {
    system == EXAMPLE_ARC
        || system
            .strip_prefix(EXAMPLE_ARC)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Accepts connections until the device is dropped.
async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let connection_shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let service =
                service_fn(move |request| handle(Arc::clone(&connection_shared), request));
            // A connection ends in an error whenever a client hangs up, which a
            // test does on purpose; the device has nothing to report about it.
            match http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                Ok(()) | Err(_) => {}
            }
        });
    }
}

/// Routes one request to its transaction.
async fn handle(
    shared: Arc<Shared>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let (parts, body) = request.into_parts();
    let Ok(body) = body
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
    else {
        return Ok(outcome(
            StatusCode::BAD_REQUEST,
            "error",
            "structure",
            "the body could not be read",
        ));
    };
    shared
        .lock()
        .bodies
        .push(String::from_utf8_lossy(&body).into_owned());
    let answer = match (&parts.method, parts.uri.path()) {
        (&Method::POST, "/fhir/Patient/_search") => search(&shared, &body),
        (&Method::POST, "/fhir/Patient/$match" | "/fhir/Patient/%24match") => {
            matching(&shared, &body)
        }
        _ => outcome(
            StatusCode::NOT_FOUND,
            "error",
            "not-supported",
            "the device serves ITI-78 searches and ITI-119 matches only",
        ),
    };
    Ok(answer)
}

/// ITI-78: the Patients carrying every identifier the search names, with the
/// identifiers of the domains it asks for (§2:3.78.4.1.3).
fn search(shared: &Shared, body: &[u8]) -> Response<Full<Bytes>> {
    let mut state = shared.lock();
    state.searches = state.searches.saturating_add(1);
    let mut criteria = Vec::new();
    let mut domains = BTreeSet::new();
    for (name, value) in url::form_urlencoded::parse(body) {
        if name != "identifier" {
            return outcome(
                StatusCode::BAD_REQUEST,
                "error",
                "not-supported",
                "the device searches by identifier only",
            );
        }
        if value.ends_with('|') {
            domains.extend(
                value
                    .split(',')
                    .filter_map(|domain| domain.strip_suffix('|'))
                    .map(str::to_owned),
            );
            continue;
        }
        let Some((system, value)) = value.split_once('|') else {
            return outcome(
                StatusCode::BAD_REQUEST,
                "error",
                "invalid",
                "the device searches by <system>|<value>",
            );
        };
        criteria.push(DomainId {
            system: system.to_owned(),
            value: value.to_owned(),
        });
    }
    let known = state.domains();
    if domains
        .iter()
        .any(|domain| !known.contains(domain.as_str()))
    {
        // NOTE: §2:3.78.4.1.3 Case 4, the preferred answer for a domain the
        // Supplier is not an authority for.
        return outcome(
            StatusCode::NOT_FOUND,
            "warning",
            "not-found",
            "targetSystem not found",
        );
    }
    let found: Vec<&Record> = state
        .patients
        .values()
        .filter(|record| {
            criteria
                .iter()
                .all(|criterion| record.identifiers.contains(criterion))
        })
        .collect();
    let total = u32::try_from(found.len()).unwrap_or(u32::MAX);
    let entry = found
        .iter()
        .map(|record| entry(record, &domains, None))
        .collect();
    resource(
        StatusCode::OK,
        &Bundle {
            r#type: "searchset".into(),
            total: Some(total.into()),
            entry,
            ..Bundle::default()
        },
    )
}

/// ITI-119: the Patients carrying an identifier of the input Patient, each a
/// certain match (§2:3.119.4.1.3).
fn matching(shared: &Shared, body: &[u8]) -> Response<Full<Bytes>> {
    let mut state = shared.lock();
    state.matches = state.matches.saturating_add(1);
    let parameters = match decode(body) {
        Ok(parameters) => parameters,
        Err(reason) => return outcome(StatusCode::BAD_REQUEST, "error", "structure", reason),
    };
    let mut input = None;
    let mut certain_only = false;
    let mut count = None;
    for parameter in &parameters.parameter {
        match (
            parameter.name.value.as_deref(),
            &parameter.resource,
            &parameter.value,
        ) {
            (Some("resource"), Some(Resource::Patient(patient)), None) => input = Some(patient),
            (Some("onlyCertainMatches"), None, Some(ParametersParameterValue::Boolean(flag))) => {
                certain_only = flag.value == Some(true);
            }
            (Some("count"), None, Some(ParametersParameterValue::Integer(limit))) => {
                count = limit.value.and_then(|limit| usize::try_from(limit).ok());
            }
            _ => {
                return outcome(
                    StatusCode::BAD_REQUEST,
                    "error",
                    "invalid",
                    "a parameter the PDQm Match Input Parameters profile does not admit",
                );
            }
        }
    }
    let Some(input) = input else {
        return outcome(
            StatusCode::BAD_REQUEST,
            "error",
            "required",
            "the resource parameter is 1..1",
        );
    };
    let asked: BTreeSet<DomainId> = input
        .identifier
        .iter()
        .filter_map(|identifier| {
            Some(DomainId {
                system: identifier.system.as_ref()?.value.clone()?,
                value: identifier.value.as_ref()?.value.clone()?,
            })
        })
        .collect();
    let mut found: Vec<&Record> = state
        .patients
        .values()
        .filter(|record| !record.identifiers.is_disjoint(&asked))
        .collect();
    // NOTE: §2:3.119.4.1.3 Case 5, several matches the device cannot confirm
    // are one person are the zero result set under onlyCertainMatches.
    if certain_only && found.len() > 1 {
        found.clear();
    }
    if let Some(count) = count {
        found.truncate(count);
    }
    let entry = found
        .iter()
        .map(|record| entry(record, &BTreeSet::new(), Some("certain")))
        .collect();
    resource(
        StatusCode::OK,
        &Bundle {
            r#type: "searchset".into(),
            entry,
            ..Bundle::default()
        },
    )
}

/// The Bundle entry of `record`, with the identifiers of `domains`, graded
/// `grade` when one is given.
fn entry(record: &Record, domains: &BTreeSet<String>, grade: Option<&str>) -> BundleEntry {
    BundleEntry {
        full_url: Some(format!("http://pdq.example.org/fhir/Patient/{}", record.id).into()),
        resource: Some(Resource::Patient(Box::new(record.patient(domains)))),
        search: Some(BundleEntrySearch {
            mode: Some("match".into()),
            score: Some("1".into()),
            extension: grade
                .map(|grade| Extension {
                    url: MATCH_GRADE.to_owned(),
                    value: Some(ExtensionValue::Code(grade.into())),
                    ..Extension::default()
                })
                .into_iter()
                .collect(),
            ..BundleEntrySearch::default()
        }),
        ..BundleEntry::default()
    }
}

/// Decodes a FHIR JSON `Parameters`.
fn decode(body: &[u8]) -> Result<Parameters, &'static str> {
    let value: Value = serde_json::from_slice(body).map_err(|_unquoted| "the body is not JSON")?;
    let root = Path::root("Parameters");
    let object = expect_object(&value, &root).map_err(|_unquoted| "the body is not an object")?;
    if object.get("resourceType").and_then(Value::as_str) != Some("Parameters") {
        return Err("the body is not a Parameters");
    }
    Parameters::from_json(object, &mut Path::root("Parameters"))
        .map_err(|_unquoted| "the body does not decode as an R4 Parameters")
}

/// An `OperationOutcome` with one issue of `severity` and `code`.
fn outcome(status: StatusCode, severity: &str, code: &str, text: &str) -> Response<Full<Bytes>> {
    resource(
        status,
        &OperationOutcome {
            issue: vec![OperationOutcomeIssue {
                severity: severity.into(),
                code: code.into(),
                diagnostics: Some(text.into()),
                ..OperationOutcomeIssue::default()
            }],
            ..OperationOutcome::default()
        },
    )
}

/// `body` serialised as FHIR JSON under `status`.
fn resource(status: StatusCode, body: &impl serde::Serialize) -> Response<Full<Bytes>> {
    let mut answer = match serde_json::to_vec(body) {
        Ok(bytes) => Response::new(Full::new(Bytes::from(bytes))),
        Err(_unencodable) => {
            let mut empty = Response::new(Full::new(Bytes::new()));
            *empty.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return empty;
        }
    };
    *answer.status_mut() = status;
    answer
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
    answer
}

#[cfg(test)]
mod tests {
    use super::in_example_arc;

    #[test]
    fn only_the_example_arc_is_admitted() {
        assert!(in_example_arc("urn:oid:2.999"));
        assert!(in_example_arc("urn:oid:2.999.1.7"));
        assert!(!in_example_arc("urn:oid:2.9990"));
        assert!(!in_example_arc("urn:oid:1.2.3"));
    }
}
