// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness Patient Identity Registry: a test device that takes ITI-94
//! subscriptions and sends ITI-93 messages to them (Federation Tier with AQL
//! §16.1, track 8; IHE PMIR 1.6.0).
//!
//! **This is a test device, not a PMIR implementation.** It plays the Patient
//! Identity Registry the identity feed subscribes at, so a test can drive a
//! merge through the gateway's real route without a container (no
//! specification governs this: our own design). It holds the wire to the
//! vendored PMIR 1.6.0 artefacts under `docs/specs/ihe-pmir/`:
//!
//! - **ITI-94, Subscribe to Patient Updates:** `POST [base]/Subscription`
//!   with a `Subscription` that holds to the request profile (`status`
//!   `requested`, a `message` channel with an `endpoint` and a `payload`), and
//!   no `channel.header`, since no credential for the feed travels in the
//!   subscription (§2:3.94.5), is answered `201` with its `Location`; any
//!   other is `400`. `GET` of the location answers the `Subscription` with the
//!   status [`PatientIdentityRegistry::set_status`] gives it, `active` until
//!   then, and `DELETE` removes it (`204`); an unknown id is `404`.
//! - **ITI-93, Mobile Patient Identity Feed:** [`PatientIdentityRegistry::send`]
//!   posts a message to the channel endpoint of every subscription it holds,
//!   with the bearer token the test agreed with the gateway out of band
//!   (§2:3.93.5), and returns each answer. [`merge_message`] writes a message
//!   whose history holds one merge (§2:3.93.4.1.2.4).
//!
//! The identifiers it carries are synthetic, inside the `urn:oid:2.999`
//! example arc, and nothing here logs one; its `Debug` shows counts only.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntryRequest, BundleEntryResponse};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::message_header::{
    MessageHeader, MessageHeaderDestination, MessageHeaderEvent, MessageHeaderSource,
};
use fhir_types::r4::patient::{Patient, PatientLink};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhir_types::r4::subscription::Subscription;
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, LOCATION};
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

/// The harness Registry could not start or send.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// No loopback port could be bound or read back.
    #[error("the Patient Identity Registry could not bind a loopback port")]
    Bind(#[source] std::io::Error),
    /// A message could not be sent to a subscription's channel.
    #[error("the ITI-93 message could not be sent to a subscription's channel")]
    Send(#[source] reqwest::Error),
    /// A message could not be written.
    #[error("the ITI-93 message could not be written as FHIR JSON")]
    Write(#[source] serde_json::Error),
}

/// One subscription the device holds, as the subscriber wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The `criteria`.
    pub criteria: String,
    /// The `channel.endpoint` the feed is sent to.
    pub endpoint: String,
    /// The `channel.payload`.
    pub payload: String,
    /// The `status` a read answers with.
    pub status: String,
}

/// What the device holds.
#[derive(Debug, Default)]
struct State {
    /// The subscriptions, by id.
    subscriptions: BTreeMap<String, Held>,
    /// The number the next subscription's id is built from.
    next: u64,
    /// The status every create is answered with instead of `201`, when set.
    refusing: Option<StatusCode>,
    /// How many subscriptions were deleted.
    deleted: usize,
    /// How many creates arrived, refused ones included.
    creates: usize,
    /// How long a create waits, after the subscription is held, before it
    /// answers.
    create_delay: Option<Duration>,
    /// The `Location` a create answers with.
    location: LocationMode,
    /// Whether a search by `url` is refused with `400`.
    search_refused: bool,
}

/// The `Location` the device answers a create with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocationMode {
    /// `Subscription/<id>/_history/1`, under the device's base.
    #[default]
    Relative,
    /// No `Location` at all.
    Missing,
    /// A `Subscription` under another base.
    Elsewhere,
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

/// The harness Patient Identity Registry on a loopback port, stopped when it
/// is dropped.
pub struct PatientIdentityRegistry {
    origin: String,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl fmt::Debug for PatientIdentityRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.shared.lock();
        f.debug_struct("PatientIdentityRegistry")
            .field("origin", &self.origin)
            .field("subscriptions", &state.subscriptions.len())
            .field("deleted", &state.deleted)
            .finish_non_exhaustive()
    }
}

impl PatientIdentityRegistry {
    /// Starts the device on a free loopback port, holding no subscription.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::Bind`] when no loopback port can be bound.
    pub async fn start() -> Result<Self, RegistryError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(RegistryError::Bind)?;
        let address = listener.local_addr().map_err(RegistryError::Bind)?;
        let shared = Arc::new(Shared::default());
        let task = tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        Ok(Self {
            origin: format!("http://{address}"),
            shared,
            task,
        })
    }

    /// Returns the FHIR base URL a subscriber is pointed at, with its trailing
    /// slash: `http://127.0.0.1:<port>/fhir/`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("{}{BASE}/", self.origin)
    }

    /// Returns the subscriptions the device holds, in id order.
    #[must_use]
    pub fn subscriptions(&self) -> Vec<Held> {
        self.shared.lock().subscriptions.values().cloned().collect()
    }

    /// Returns how many subscriptions were deleted.
    #[must_use]
    pub fn deleted(&self) -> usize {
        self.shared.lock().deleted
    }

    /// Answers every later create with `status` and an `OperationOutcome`,
    /// or with `201` again when `None`.
    pub fn refuse_subscriptions(&self, status: Option<StatusCode>) {
        self.shared.lock().refusing = status;
    }

    /// Gives every subscription the device holds `status`, which a read then
    /// answers with (§2:3.94.4.1.3).
    pub fn set_status(&self, status: &str) {
        for held in self.shared.lock().subscriptions.values_mut() {
            status.clone_into(&mut held.status);
        }
    }

    /// Forgets every subscription, as a Registry that lost them would.
    pub fn forget_subscriptions(&self) {
        self.shared.lock().subscriptions.clear();
    }

    /// Returns how many creates arrived, refused ones included.
    #[must_use]
    pub fn creates(&self) -> usize {
        self.shared.lock().creates
    }

    /// Holds every later subscription at once and answers its create only
    /// after `delay`, as a Registry that answers late would; `None` answers
    /// at once again.
    pub fn delay_creates(&self, delay: Option<Duration>) {
        self.shared.lock().create_delay = delay;
    }

    /// Answers every later create with the `Location` `mode` names.
    pub fn locate_creates(&self, mode: LocationMode) {
        self.shared.lock().location = mode;
    }

    /// Refuses every later search by `url` with `400`, as a Registry that
    /// does not support the parameter would, when `refused`.
    pub fn refuse_search(&self, refused: bool) {
        self.shared.lock().search_refused = refused;
    }

    /// Sends the ITI-93 message `body` to the channel endpoint of every
    /// subscription, with `token` as its bearer token when given, and
    /// returns each answer's status and body, in subscription order.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::Send`] when a channel cannot be reached.
    pub async fn send(
        &self,
        body: &str,
        token: Option<&str>,
    ) -> Result<Vec<(StatusCode, String)>, RegistryError> {
        let endpoints: Vec<String> = self
            .subscriptions()
            .into_iter()
            .map(|held| held.endpoint)
            .collect();
        let client = reqwest::Client::new();
        let mut answers = Vec::with_capacity(endpoints.len());
        for endpoint in endpoints {
            let mut request = client
                .post(endpoint)
                .header(CONTENT_TYPE, FHIR_JSON)
                .body(body.to_owned());
            if let Some(token) = token {
                request = request.header(AUTHORIZATION, format!("Bearer {token}"));
            }
            let response = request.send().await.map_err(RegistryError::Send)?;
            let status = response.status();
            let text = response.text().await.map_err(RegistryError::Send)?;
            answers.push((status, text));
        }
        Ok(answers)
    }
}

impl Drop for PatientIdentityRegistry {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// An ITI-93 message whose history holds one merge: the Patient `subsumed`,
/// carrying `identifiers` as `(system, value)`, replaced by `surviving`
/// (§2:3.93.4.1.2.4, the merged Patient profile).
///
/// # Errors
///
/// Returns [`RegistryError::Write`] when the message cannot be written.
pub fn merge_message(
    subsumed: &str,
    identifiers: &[(&str, &str)],
    surviving: &str,
) -> Result<String, RegistryError> {
    let patient = Patient {
        id: Some(subsumed.to_owned()),
        identifier: identifiers
            .iter()
            .map(|(system, value)| Identifier {
                system: Some((*system).into()),
                value: Some((*value).into()),
                ..Identifier::default()
            })
            .collect(),
        active: Some(false.into()),
        link: vec![PatientLink {
            other: Reference {
                reference: Some(format!("Patient/{surviving}").into()),
                ..Reference::default()
            },
            r#type: "replaced-by".into(),
            ..PatientLink::default()
        }],
        ..Patient::default()
    };
    let history = Bundle {
        id: Some(String::from("history-1")),
        r#type: "history".into(),
        entry: vec![BundleEntry {
            full_url: Some(format!("http://pmir.example.test{BASE}/Patient/{subsumed}").into()),
            resource: Some(Resource::Patient(Box::new(patient))),
            request: Some(BundleEntryRequest {
                method: "PUT".into(),
                url: format!("Patient/{subsumed}").into(),
                ..BundleEntryRequest::default()
            }),
            response: Some(BundleEntryResponse {
                status: "200".into(),
                ..BundleEntryResponse::default()
            }),
            ..BundleEntry::default()
        }],
        ..Bundle::default()
    };
    let header = MessageHeader {
        id: Some(String::from("message-1")),
        meta: None,
        implicit_rules: None,
        language: None,
        text: None,
        contained: Vec::new(),
        extension: Vec::new(),
        modifier_extension: Vec::new(),
        event: MessageHeaderEvent::Uri("urn:ihe:iti:pmir:2019:patient-feed".into()),
        destination: vec![MessageHeaderDestination {
            endpoint: "http://gateway.example.test/pmir/feed".into(),
            ..MessageHeaderDestination::default()
        }],
        sender: None,
        enterer: None,
        author: None,
        source: MessageHeaderSource {
            endpoint: format!("http://pmir.example.test{BASE}").into(),
            ..MessageHeaderSource::default()
        },
        responsible: None,
        reason: None,
        response: None,
        focus: vec![Reference {
            reference: Some("Bundle/history-1".into()),
            ..Reference::default()
        }],
        definition: None,
    };
    let message = Bundle {
        r#type: "message".into(),
        entry: vec![
            BundleEntry {
                full_url: Some(
                    format!("http://pmir.example.test{BASE}/MessageHeader/message-1").into(),
                ),
                resource: Some(Resource::MessageHeader(Box::new(header))),
                ..BundleEntry::default()
            },
            BundleEntry {
                full_url: Some(format!("http://pmir.example.test{BASE}/Bundle/history-1").into()),
                resource: Some(Resource::Bundle(Box::new(history))),
                ..BundleEntry::default()
            },
        ],
        ..Bundle::default()
    };
    serde_json::to_string(&message).map_err(RegistryError::Write)
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

/// Routes one request to its interaction.
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
        return Ok(outcome(StatusCode::BAD_REQUEST, "structure"));
    };
    let path = parts.uri.path();
    let id = path
        .strip_prefix(BASE)
        .and_then(|rest| rest.strip_prefix("/Subscription/"))
        .filter(|id| !id.is_empty() && !id.contains('/'));
    let answer = match (&parts.method, path, id) {
        (&Method::POST, "/fhir/Subscription", _) => create(&shared, &body).await,
        (&Method::GET, "/fhir/Subscription", _) => search(&shared, parts.uri.query()),
        (&Method::GET, _, Some(id)) => read(&shared, id),
        (&Method::DELETE, _, Some(id)) => delete(&shared, id),
        _ => outcome(StatusCode::NOT_FOUND, "not-supported"),
    };
    Ok(answer)
}

/// ITI-94: a `Subscription` that holds to the request profile is created
/// (§2:3.94.4.1.2.1).
async fn create(shared: &Shared, body: &[u8]) -> Response<Full<Bytes>> {
    let refusing = {
        let mut state = shared.lock();
        state.creates = state.creates.saturating_add(1);
        state.refusing
    };
    if let Some(status) = refusing {
        return outcome(status, "forbidden");
    }
    let Some(subscription) = decode(body) else {
        return outcome(StatusCode::BAD_REQUEST, "structure");
    };
    let channel = &subscription.channel;
    let text = |value: Option<&String>| value.cloned().unwrap_or_default();
    let held = Held {
        criteria: text(subscription.criteria.value.as_ref()),
        endpoint: text(channel.endpoint.as_ref().and_then(|url| url.value.as_ref())),
        payload: text(
            channel
                .payload
                .as_ref()
                .and_then(|code| code.value.as_ref()),
        ),
        status: String::from("active"),
    };
    let conformant = subscription.status.value.as_deref() == Some("requested")
        && channel.r#type.value.as_deref() == Some("message")
        && !held.endpoint.is_empty()
        && !held.payload.is_empty()
        && !held.criteria.is_empty()
        && channel.header.is_empty();
    if !conformant {
        return outcome(StatusCode::BAD_REQUEST, "invalid");
    }
    let (id, delay, mode) = {
        let mut state = shared.lock();
        state.next = state.next.saturating_add(1);
        let id = format!("subscription-{}", state.next);
        state.subscriptions.insert(id.clone(), held);
        (id, state.create_delay, state.location)
    };
    if let Some(delay) = delay {
        tokio::time::sleep(delay).await;
    }
    let mut answer = Response::new(Full::new(Bytes::new()));
    *answer.status_mut() = StatusCode::CREATED;
    let location = match mode {
        LocationMode::Relative => Some(format!("Subscription/{id}/_history/1")),
        LocationMode::Elsewhere => Some(format!(
            "http://elsewhere.example.test{BASE}/Subscription/{id}"
        )),
        LocationMode::Missing => None,
    };
    if let Some(location) = location.and_then(|text| HeaderValue::from_str(&text).ok()) {
        answer.headers_mut().insert(LOCATION, location);
    }
    answer
}

/// The `Subscription` resource of `held` with the id `id`.
fn resource(id: &str, held: &Held) -> Subscription {
    let mut subscription = Subscription {
        id: Some(id.to_owned()),
        status: held.status.as_str().into(),
        reason: "harness".into(),
        criteria: held.criteria.as_str().into(),
        ..Subscription::default()
    };
    subscription.channel.r#type = "message".into();
    subscription.channel.endpoint = Some(held.endpoint.as_str().into());
    subscription.channel.payload = Some(held.payload.as_str().into());
    subscription
}

/// FHIR R4 search: the subscriptions whose channel endpoint is the `url`
/// parameter, as a `searchset` Bundle, or `400` when search is refused.
fn search(shared: &Shared, query: Option<&str>) -> Response<Full<Bytes>> {
    let url = query.and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == "url")
            .map(|(_, value)| value.into_owned())
    });
    let state = shared.lock();
    if state.search_refused {
        return outcome(StatusCode::BAD_REQUEST, "not-supported");
    }
    let entry = state
        .subscriptions
        .iter()
        .filter(|(_, held)| url.as_deref().is_none_or(|url| held.endpoint == url))
        .map(|(id, held)| BundleEntry {
            full_url: Some(format!("http://pmir.example.test{BASE}/Subscription/{id}").into()),
            resource: Some(Resource::Subscription(Box::new(resource(id, held)))),
            ..BundleEntry::default()
        })
        .collect();
    drop(state);
    let bundle = Bundle {
        r#type: "searchset".into(),
        entry,
        ..Bundle::default()
    };
    match serde_json::to_vec(&bundle) {
        Ok(bytes) => {
            let mut answer = Response::new(Full::new(Bytes::from(bytes)));
            answer
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
            answer
        }
        Err(_unwritable) => outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception"),
    }
}

/// ITI-94: the `Subscription` with its current status (§2:3.94.4.3).
fn read(shared: &Shared, id: &str) -> Response<Full<Bytes>> {
    let Some(held) = shared.lock().subscriptions.get(id).cloned() else {
        return outcome(StatusCode::NOT_FOUND, "not-found");
    };
    match serde_json::to_vec(&resource(id, &held)) {
        Ok(bytes) => {
            let mut answer = Response::new(Full::new(Bytes::from(bytes)));
            answer
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
            answer
        }
        Err(_unwritable) => outcome(StatusCode::INTERNAL_SERVER_ERROR, "exception"),
    }
}

/// ITI-94: the `Subscription` is removed (§2:3.94.4.5).
fn delete(shared: &Shared, id: &str) -> Response<Full<Bytes>> {
    let mut state = shared.lock();
    if state.subscriptions.remove(id).is_none() {
        return outcome(StatusCode::NOT_FOUND, "not-found");
    }
    state.deleted = state.deleted.saturating_add(1);
    drop(state);
    let mut answer = Response::new(Full::new(Bytes::new()));
    *answer.status_mut() = StatusCode::NO_CONTENT;
    answer
}

/// The `Subscription` `body` holds, when it decodes as one.
fn decode(body: &[u8]) -> Option<Subscription> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let root = Path::root("Subscription");
    let object = expect_object(&value, &root).ok()?;
    if object.get("resourceType").and_then(Value::as_str) != Some("Subscription") {
        return None;
    }
    Subscription::from_json(object, &mut Path::root("Subscription")).ok()
}

/// An answer of `status` with an `OperationOutcome` of one `error` issue of
/// `code`.
fn outcome(status: StatusCode, code: &str) -> Response<Full<Bytes>> {
    let body = format!(
        r#"{{"resourceType":"OperationOutcome","issue":[{{"severity":"error","code":"{code}"}}]}}"#
    );
    let mut answer = Response::new(Full::new(Bytes::from(body)));
    *answer.status_mut() = status;
    answer
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
    answer
}
