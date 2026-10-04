// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness PIX Manager: a test device that answers ITI-83 from what an
//! ITI-104 feed delivered (Federation Tier with AQL §16.1; IHE PIXm 3.1.0).
//!
//! **This is a test device, not a PIXm implementation.** It plays the Patient
//! Identifier Cross-reference Manager the e2e suites resolve through, so a
//! test can seed real cross-references and read real ITI-83 answers without
//! a container (no specification governs this: our own design). It holds the
//! wire of both transactions to the vendored PIXm 3.1.0 artefacts under
//! `docs/specs/ihe-pixm/`, and it decides nothing a Manager decides on
//! demographics: it cross-references the identifiers one fed `Patient` carries
//! together, and the identifiers of every `Patient` that shares one of them.
//! A FerroPIX instance replaces it in a differential run once one exists.
//!
//! - **ITI-104, Patient Identity Feed FHIR:** `PUT [base]/Patient?identifier=
//!   <system>|<value>`, a conditional update (`CapabilityStatement-IHE.PIXm
//!   .Manager`). The `Patient` is held to the `IHE.PIXm.Patient` profile's
//!   minimums: at least one `identifier`, each with a `system` and a `value`;
//!   at least one `name` carrying a `family`, `given` or `text`
//!   (`iti-pdqm-patname`); and `active` present when `link` is
//!   (`iti-pdqm-linkstatus`). No match creates the `Patient` (`201`), one match
//!   replaces it (`200`), several are a `412`. `DELETE [base]/Patient?identifier=
//!   …` is the Remove Patient Option's conditional delete (`204`).
//! - **ITI-83, Get Corresponding Identifiers:** `GET [base]/Patient/$ihe-pix?
//!   sourceIdentifier=<system>|<value>[&targetSystem=<system>]*`, or the same
//!   inputs as `valueString`s of a `Parameters` body posted to the operation
//!   (FHIR R4 Operations §3.2.0.1), answered as
//!   ITI TF-2 §3.83.4.2.2 lays out: a `Parameters` of `targetId` and
//!   `targetIdentifier` (Case 1), a `404` `not-found` when the source is unknown
//!   (Case 2), a `400` `code-invalid` when its domain is unknown (Case 3), a
//!   `403` `code-invalid` when a target domain is unknown (Case 4), and a `200`
//!   with an empty `Bundle` when the source names only a deprecated `Patient`
//!   (`active` false, §3.83.4.2.2.5).
//!
//! Faults and wire capture come from the harness proxy: a
//! [`CapturingProxy`](crate::proxy::CapturingProxy) in front of
//! [`PixManager::origin`] refuses, delays or fails the Manager's answers as it
//! does a node's. The identifiers the device holds are synthetic, as every
//! harness fixture is, and nothing here logs one; its `Debug` shows counts
//! only.

use bytes::Bytes;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use http::header::{CONTENT_TYPE, HeaderValue, LOCATION};
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The FHIR base path the device serves under.
const BASE: &str = "/fhir";

/// The FHIR JSON media type (ITI TF-2 Appendix Z.6).
const FHIR_JSON: &str = "application/fhir+json";

/// The harness PIX Manager could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PixError {
    /// No loopback port could be bound or read back.
    #[error("the PIX Manager could not bind a loopback port")]
    Bind(#[source] std::io::Error),
}

/// One identifier in one identity domain: the assigning authority's system
/// and the value it issued.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DomainId {
    /// The assigning authority (`Identifier.system`).
    system: String,
    /// The identifier value (`Identifier.value`).
    value: String,
}

impl DomainId {
    /// Parses the `<system>|<value>` token of a FHIR identifier search
    /// parameter, or `None` when either half is empty.
    fn token(text: &str) -> Option<Self> {
        let (system, value) = text.split_once('|')?;
        (!system.is_empty() && !value.is_empty()).then(|| Self {
            system: system.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// One fed `Patient`.
#[derive(Debug, Clone)]
struct Record {
    /// The resource id the device assigned when the feed created it.
    id: String,
    /// The identifiers the feed delivered.
    identifiers: BTreeSet<DomainId>,
    /// Whether the `Patient` is active; a deprecated one is `false`.
    active: bool,
}

/// What the device holds and what it was asked.
#[derive(Debug, Default)]
struct State {
    /// The fed `Patient`s, by resource id.
    patients: BTreeMap<String, Record>,
    /// The number the next created `Patient`'s id is built from.
    next: u64,
    /// How many ITI-83 requests arrived.
    queries: usize,
    /// How many ITI-104 updates were accepted.
    feeds: usize,
}

impl State {
    /// Every identity domain a fed `Patient` names, deprecated ones included.
    fn domains(&self) -> BTreeSet<&str> {
        self.patients
            .values()
            .flat_map(|record| record.identifiers.iter().map(|id| id.system.as_str()))
            .collect()
    }

    /// The ids of the `Patient`s that carry `identifier`.
    fn carrying(&self, identifier: &DomainId) -> Vec<String> {
        self.patients
            .values()
            .filter(|record| record.identifiers.contains(identifier))
            .map(|record| record.id.clone())
            .collect()
    }

    /// The active `Patient`s cross-referenced with `source`: those carrying
    /// it and, transitively, those sharing an identifier with one of them.
    fn linked(&self, source: &DomainId) -> Vec<&Record> {
        let mut found: BTreeSet<&str> = BTreeSet::new();
        let mut frontier: BTreeSet<&DomainId> = BTreeSet::from([source]);
        while let Some(identifier) = frontier.pop_first() {
            for record in self.patients.values() {
                if record.active
                    && record.identifiers.contains(identifier)
                    && found.insert(record.id.as_str())
                {
                    frontier.extend(record.identifiers.iter());
                }
            }
        }
        found
            .into_iter()
            .filter_map(|id| self.patients.get(id))
            .collect()
    }
}

/// The state the accept loop shares.
#[derive(Default)]
struct Shared {
    /// The device's holdings and counters.
    state: Mutex<State>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The harness PIX Manager on a loopback port, stopped when it is dropped.
pub struct PixManager {
    /// The origin the device listens on, with no path.
    origin: String,
    /// The state the accept loop shares.
    shared: Arc<Shared>,
    /// The accept loop, aborted on drop.
    task: JoinHandle<()>,
}

impl fmt::Debug for PixManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.shared.lock();
        f.debug_struct("PixManager")
            .field("origin", &self.origin)
            .field("patients", &state.patients.len())
            .field("queries", &state.queries)
            .field("feeds", &state.feeds)
            .finish_non_exhaustive()
    }
}

impl PixManager {
    /// Starts the device on a free loopback port, holding no `Patient`.
    ///
    /// # Errors
    ///
    /// Returns [`PixError::Bind`] when no loopback port can be bound.
    pub async fn start() -> Result<Self, PixError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(PixError::Bind)?;
        let address = listener.local_addr().map_err(PixError::Bind)?;
        let shared = Arc::new(Shared::default());
        let task = tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        Ok(Self {
            origin: format!("http://{address}"),
            shared,
            task,
        })
    }

    /// Returns the FHIR base URL a PIXm client and an ITI-104 source are
    /// pointed at, with its trailing slash: `http://127.0.0.1:<port>/fhir/`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("{}{BASE}/", self.origin)
    }

    /// Returns the origin the device listens on, with no path.
    ///
    /// A [`CapturingProxy`](crate::proxy::CapturingProxy) started on this
    /// origin journals every request to the device and injects its faults; a
    /// client then uses the proxy's origin followed by `/fhir/` as its base.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns how many ITI-83 requests the device has received.
    #[must_use]
    pub fn queries(&self) -> usize {
        self.shared.lock().queries
    }

    /// Returns how many ITI-104 updates the device has accepted.
    #[must_use]
    pub fn feeds(&self) -> usize {
        self.shared.lock().feeds
    }

    /// Returns how many `Patient`s the device holds, deprecated ones included.
    #[must_use]
    pub fn patients(&self) -> usize {
        self.shared.lock().patients.len()
    }
}

impl Drop for PixManager {
    fn drop(&mut self) {
        self.task.abort();
    }
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
            "structure",
            "the body could not be read",
        ));
    };
    let path = parts.uri.path();
    let query = query_pairs(parts.uri.query());
    let answer = match (&parts.method, path) {
        (&Method::GET, "/fhir/Patient/$ihe-pix" | "/fhir/Patient/%24ihe-pix") => {
            cross_reference(&shared, &query)
        }
        (&Method::POST, "/fhir/Patient/$ihe-pix" | "/fhir/Patient/%24ihe-pix") => {
            match posted(parts.headers.get(CONTENT_TYPE), &body) {
                Ok(inputs) => cross_reference(&shared, &inputs),
                Err(refusal) => refusal.answer(),
            }
        }
        (&Method::PUT, "/fhir/Patient") => {
            feed(&shared, &query, parts.headers.get(CONTENT_TYPE), &body)
        }
        (&Method::DELETE, "/fhir/Patient") => remove(&shared, &query),
        _ => outcome(
            StatusCode::NOT_FOUND,
            "not-supported",
            "the device serves ITI-83 and ITI-104 only",
        ),
    };
    Ok(answer)
}

/// The decoded `name=value` pairs of a query string, in order.
fn query_pairs(query: Option<&str>) -> Vec<(String, String)> {
    query.map_or_else(Vec::new, |query| {
        url::form_urlencoded::parse(query.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect()
    })
}

/// The values of the query parameter `name`, in order.
fn values<'a>(query: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    query
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .collect()
}

/// Why a conditional interaction's condition is refused: the issue type and
/// the text of the `400` answering it.
#[derive(Debug, Clone, Copy)]
struct BadCondition(&'static str, &'static str);

impl BadCondition {
    fn answer(self) -> Response<Full<Bytes>> {
        outcome(StatusCode::BAD_REQUEST, self.0, self.1)
    }
}

/// The one `identifier=<system>|<value>` a conditional interaction names.
fn condition(query: &[(String, String)]) -> Result<DomainId, BadCondition> {
    match values(query, "identifier").as_slice() {
        [token] => DomainId::token(token).ok_or(BadCondition(
            "invalid",
            "the identifier condition is not <system>|<value>",
        )),
        [] => Err(BadCondition(
            "required",
            "a conditional interaction names one identifier",
        )),
        _ => Err(BadCondition(
            "invalid",
            "a conditional interaction names one identifier, not several",
        )),
    }
}

/// The input parameters of a posted `$ihe-pix`, read from its `Parameters`
/// body as the name and value pairs a `GET` carries: each a `valueString`
/// named by the `OperationDefinition` (FHIR R4 Operations §3.2.0.1; the PIXm
/// Query Parameters In profile). The request URL is not read.
fn posted(media: Option<&HeaderValue>, body: &[u8]) -> Result<Vec<(String, String)>, BadCondition> {
    const NOT_PARAMETERS: BadCondition = BadCondition("structure", "the body is not a Parameters");
    if !fhir_json(media) {
        return Err(BadCondition("not-supported", "the body is not FHIR JSON"));
    }
    let value: Value = serde_json::from_slice(body).map_err(|_unquoted| NOT_PARAMETERS)?;
    let object =
        expect_object(&value, &Path::root("Parameters")).map_err(|_unquoted| NOT_PARAMETERS)?;
    if object.get("resourceType").and_then(Value::as_str) != Some("Parameters") {
        return Err(NOT_PARAMETERS);
    }
    let parameters = Parameters::from_json(object, &mut Path::root("Parameters"))
        .map_err(|_unquoted| NOT_PARAMETERS)?;
    parameters
        .parameter
        .iter()
        .map(
            |parameter| match (parameter.name.value.as_deref(), &parameter.value) {
                (
                    Some(name @ ("sourceIdentifier" | "targetSystem" | "_format")),
                    Some(ParametersParameterValue::String(text)),
                ) => text
                    .value
                    .clone()
                    .map(|text| (name.to_owned(), text))
                    .ok_or(NOT_PARAMETERS),
                _ => Err(BadCondition(
                    "invalid",
                    "a $ihe-pix input is a sourceIdentifier, targetSystem or _format valueString",
                )),
            },
        )
        .collect()
}

/// ITI-83: the identifiers cross-referenced with the source in the asked
/// domains (ITI TF-2 §3.83.4.2.2).
fn cross_reference(shared: &Shared, query: &[(String, String)]) -> Response<Full<Bytes>> {
    let mut state = shared.lock();
    state.queries = state.queries.saturating_add(1);
    let source = match values(query, "sourceIdentifier").as_slice() {
        [token] => match DomainId::token(token) {
            Some(source) => source,
            None => {
                return outcome(
                    StatusCode::BAD_REQUEST,
                    "invalid",
                    "sourceIdentifier is not <system>|<value>",
                );
            }
        },
        [] => {
            return outcome(
                StatusCode::BAD_REQUEST,
                "required",
                "sourceIdentifier is required",
            );
        }
        _ => {
            return outcome(
                StatusCode::BAD_REQUEST,
                "invalid",
                "sourceIdentifier is 1..1 ($ihe-pix)",
            );
        }
    };
    let targets: BTreeSet<&str> = values(query, "targetSystem").into_iter().collect();
    let domains = state.domains();
    if !domains.contains(source.system.as_str()) {
        // ITI TF-2 §3.83.4.2.2.3, Case 3.
        return outcome(
            StatusCode::BAD_REQUEST,
            "code-invalid",
            "sourceIdentifier Assigning Authority not found",
        );
    }
    if targets.iter().any(|target| !domains.contains(target)) {
        // ITI TF-2 §3.83.4.2.2.4, Case 4.
        return outcome(
            StatusCode::FORBIDDEN,
            "code-invalid",
            "targetSystem not found",
        );
    }
    let linked = state.linked(&source);
    if linked.is_empty() {
        if state.carrying(&source).is_empty() {
            // ITI TF-2 §3.83.4.2.2.2, Case 2.
            return outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "sourceIdentifier Patient Identifier not found",
            );
        }
        // ITI TF-2 §3.83.4.2.2.5: the source names a deprecated Patient only.
        return resource(StatusCode::OK, &empty_bundle());
    }
    let mut parameter = Vec::new();
    for record in linked {
        let mut wanted = record
            .identifiers
            .iter()
            .filter(|id| **id != source)
            .filter(|id| targets.is_empty() || targets.contains(id.system.as_str()))
            .peekable();
        if wanted.peek().is_none() {
            continue;
        }
        parameter.push(ParametersParameter {
            name: "targetId".into(),
            value: Some(ParametersParameterValue::Reference(Box::new(Reference {
                reference: Some(format!("Patient/{}", record.id).as_str().into()),
                ..Reference::default()
            }))),
            ..ParametersParameter::default()
        });
        for id in wanted {
            parameter.push(ParametersParameter {
                name: "targetIdentifier".into(),
                value: Some(ParametersParameterValue::Identifier(Box::new(Identifier {
                    system: Some(id.system.as_str().into()),
                    value: Some(id.value.as_str().into()),
                    ..Identifier::default()
                }))),
                ..ParametersParameter::default()
            });
        }
    }
    resource(
        StatusCode::OK,
        &Parameters {
            parameter,
            ..Parameters::default()
        },
    )
}

/// ITI-104: the conditional update that creates or replaces one `Patient`.
fn feed(
    shared: &Shared,
    query: &[(String, String)],
    media: Option<&HeaderValue>,
    body: &[u8],
) -> Response<Full<Bytes>> {
    let condition = match condition(query) {
        Ok(condition) => condition,
        Err(refusal) => return refusal.answer(),
    };
    if !fhir_json(media) {
        return outcome(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a fed Patient is FHIR JSON",
        );
    }
    let patient = match decode(body) {
        Ok(patient) => patient,
        Err(reason) => return outcome(StatusCode::BAD_REQUEST, "structure", reason),
    };
    let identifiers = match conformant(&patient) {
        Ok(identifiers) => identifiers,
        Err(reason) => return outcome(StatusCode::UNPROCESSABLE_ENTITY, "invariant", reason),
    };
    if !identifiers.contains(&condition) {
        return outcome(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the Patient does not carry the identifier its condition names",
        );
    }
    let active = patient
        .active
        .as_ref()
        .and_then(|active| active.value)
        .unwrap_or(true);
    let mut state = shared.lock();
    let matches = state.carrying(&condition);
    let (id, created) = match matches.as_slice() {
        [] => {
            state.next = state.next.saturating_add(1);
            (format!("pix-{}", state.next), true)
        }
        [one] => (one.clone(), false),
        _ => {
            return outcome(
                StatusCode::PRECONDITION_FAILED,
                "multiple-matches",
                "the condition matches more than one Patient",
            );
        }
    };
    state.patients.insert(
        id.clone(),
        Record {
            id: id.clone(),
            identifiers,
            active,
        },
    );
    state.feeds = state.feeds.saturating_add(1);
    let stored = Patient {
        id: Some(id.clone()),
        ..patient
    };
    let mut answer = resource(
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        &stored,
    );
    if let Ok(location) = HeaderValue::from_str(&format!("{BASE}/Patient/{id}")) {
        answer.headers_mut().insert(LOCATION, location);
    }
    answer
}

/// ITI-104, Remove Patient Option: the conditional delete of one `Patient`.
fn remove(shared: &Shared, query: &[(String, String)]) -> Response<Full<Bytes>> {
    let condition = match condition(query) {
        Ok(condition) => condition,
        Err(refusal) => return refusal.answer(),
    };
    let mut state = shared.lock();
    match state.carrying(&condition).as_slice() {
        [] => empty(StatusCode::NO_CONTENT),
        [one] => {
            let one = one.clone();
            state.patients.remove(&one);
            empty(StatusCode::NO_CONTENT)
        }
        _ => outcome(
            StatusCode::PRECONDITION_FAILED,
            "multiple-matches",
            "the condition matches more than one Patient (conditionalDelete single)",
        ),
    }
}

/// Decodes a FHIR JSON `Patient`.
fn decode(body: &[u8]) -> Result<Patient, &'static str> {
    let value: Value = serde_json::from_slice(body).map_err(|_unquoted| "the body is not JSON")?;
    let root = Path::root("Patient");
    let object = expect_object(&value, &root).map_err(|_unquoted| "the body is not an object")?;
    if object.get("resourceType").and_then(Value::as_str) != Some("Patient") {
        return Err("the body is not a Patient");
    }
    Patient::from_json(object, &mut Path::root("Patient"))
        .map_err(|_unquoted| "the body does not decode as an R4 Patient")
}

/// The identifiers of a `Patient` that holds to the `IHE.PIXm.Patient`
/// profile's minimums, or the profile rule it breaks.
fn conformant(patient: &Patient) -> Result<BTreeSet<DomainId>, &'static str> {
    if patient.identifier.is_empty() {
        return Err("IHE.PIXm.Patient: Patient.identifier is 1..*");
    }
    let mut identifiers = BTreeSet::new();
    for identifier in &patient.identifier {
        let system = identifier
            .system
            .as_ref()
            .and_then(|system| system.value.clone())
            .filter(|system| !system.is_empty())
            .ok_or("IHE.PIXm.Patient: Patient.identifier.system is 1..1")?;
        let value = identifier
            .value
            .as_ref()
            .and_then(|value| value.value.clone())
            .filter(|value| !value.is_empty())
            .ok_or("IHE.PIXm.Patient: Patient.identifier.value is 1..1")?;
        identifiers.insert(DomainId { system, value });
    }
    if patient.name.is_empty() {
        return Err("IHE.PIXm.Patient: Patient.name is 1..*");
    }
    let named = patient
        .name
        .iter()
        .all(|name| name.family.is_some() || !name.given.is_empty() || name.text.is_some());
    if !named {
        return Err("iti-pdqm-patname: a name carries a family, a given or a text");
    }
    if !patient.link.is_empty() && patient.active.is_none() {
        return Err("iti-pdqm-linkstatus: Patient.active is present when Patient.link is");
    }
    Ok(identifiers)
}

/// Whether `media` names FHIR JSON or plain JSON (FHIR R4 accepts both).
fn fhir_json(media: Option<&HeaderValue>) -> bool {
    media
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|essence| {
            essence.eq_ignore_ascii_case(FHIR_JSON)
                || essence.eq_ignore_ascii_case("application/json")
        })
}

/// The `200` answer of a source that names a deprecated `Patient` only: a
/// search-set `Bundle` with no entry (ITI TF-2 §3.83.4.2.2.5).
fn empty_bundle() -> Bundle {
    Bundle {
        r#type: "searchset".into(),
        ..Bundle::default()
    }
}

/// An `OperationOutcome` with one error issue of `code`.
fn outcome(status: StatusCode, code: &str, text: &str) -> Response<Full<Bytes>> {
    resource(
        status,
        &OperationOutcome {
            issue: vec![OperationOutcomeIssue {
                severity: "error".into(),
                code: code.into(),
                details: Some(CodeableConcept {
                    text: Some(text.into()),
                    ..CodeableConcept::default()
                }),
                ..OperationOutcomeIssue::default()
            }],
            ..OperationOutcome::default()
        },
    )
}

/// `body` serialised as FHIR JSON under `status`.
fn resource(status: StatusCode, body: &impl serde::Serialize) -> Response<Full<Bytes>> {
    match serde_json::to_vec(body) {
        Ok(bytes) => {
            let mut answer = Response::new(Full::new(Bytes::from(bytes)));
            *answer.status_mut() = status;
            answer
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
            answer
        }
        Err(_unencodable) => empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// An answer with no body.
fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    let mut answer = Response::new(Full::new(Bytes::new()));
    *answer.status_mut() = status;
    answer
}
