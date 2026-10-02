// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Node dispatch: one ITS-REST client per registry endpoint, and the mapping
//! from what a node answered to exactly one §11.1 endpoint status (N16, N40).
//!
//! Every request to a node is the generated `query_execute_adhoc_query_body`
//! (`POST {base}/v1/query/aql`) of `openehr-its`'s `rest-client`, so the
//! request line, the headers and the body are composed by that runtime and
//! nowhere in FerroFED (no specification governs this: our own design). This
//! module adds the per-endpoint client, the call's deadline and the gateway's
//! [`OutboundId`], and the classification of the answer. Every header a node
//! request carries is listed in [`crate::outbound_id`]:
//!
//! | The node | Status |
//! |---|---|
//! | answered `200` with a result set | `active` |
//! | was not reachable: a refused connection or a broken stream | `offline` |
//! | did not answer before the deadline | `time-out` |
//! | answered with a failure: a documented error, an undocumented status, a body that is not a result set, rows shorter than the query selects | `node-error` |
//!
//! A `node-error` carries the node's own status and message (§11.2), never
//! folded into `offline`; a refused connection still carries its reason. A
//! failure on the gateway's side before any request left (a credential the
//! provider could not produce, a body that would not serialize) is a
//! [`DispatchError`], never an endpoint status: nothing was sent to report on.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::hygiene::{Outbound, Part, Withheld};
use crate::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{
    CallOptions, Client, ClientError, CredentialsProvider, ErrorBody, RetryPolicy, Transport,
    TransportError,
};
use openehr_its::rest::generated::query::client::{QueryClient, QueryExecuteAdhocQueryBodyOutcome};
use openehr_its::rest::generated::query::{
    AdhocQueryExecute, QueryExecuteAdhocQueryBodyParams, ResultSet,
};
use url::Url;

/// The API version segment ITS-REST 1.1.0 puts every path under
/// (`{baseUrl}/v1/...`), appended to the endpoint's base URL.
pub const API_VERSION_SEGMENT: &str = "v1";

/// The header that carries the gateway's [`OutboundId`] to a node.
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

/// The longest node message a `node-error` copies into `error`, in characters.
const MESSAGE_LIMIT: usize = 512;

/// A shared credentials provider for one endpoint's onward grant.
pub type SharedCredentials = Arc<dyn CredentialsProvider>;

/// The query one node receives: standard AQL already scoped to that node's
/// own `ehr_id` by the rewrite (§7.1), and the page the gateway asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeQuery {
    aql: String,
    offset: Option<u32>,
    fetch: Option<u32>,
    scope: Option<String>,
    width: usize,
}

impl NodeQuery {
    /// A query of `aql` with no page bounds.
    #[must_use]
    pub fn new(aql: impl Into<String>) -> Self {
        Self {
            aql: aql.into(),
            offset: None,
            fetch: None,
            scope: None,
            width: 0,
        }
    }

    /// This query reading `width` cells of every row the node answers.
    ///
    /// A node row with fewer cells is an answer the gateway cannot use, so
    /// the endpoint is `node-error` (§11.1).
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// This query as scoped by the rewrite to the node's own `ehr_id`, which
    /// the outbound gate reads past (§7.1).
    #[must_use]
    pub fn with_scope(mut self, ehr_id: &HierObjectId) -> Self {
        self.scope = Some(ehr_id.value().to_owned());
        self
    }

    /// This query starting at row `offset` of the node's answer.
    #[must_use]
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// This query asking the node for at most `fetch` rows.
    #[must_use]
    pub fn with_fetch(mut self, fetch: u32) -> Self {
        self.fetch = Some(fetch);
        self
    }

    /// The AQL text the node receives.
    #[must_use]
    pub fn aql(&self) -> &str {
        &self.aql
    }

    /// The ITS-REST request body for this query.
    fn body(&self) -> AdhocQueryExecute {
        AdhocQueryExecute {
            q: self.aql.clone(),
            offset: self.offset.map(i64::from),
            fetch: self.fetch.map(i64::from),
            query_parameters: None,
            additional_properties: BTreeMap::new(),
        }
    }
}

/// The per-call options of one dispatch: the instant the node must have
/// answered by, the gateway's [`OutboundId`], and the identifiers no request
/// may carry.
#[derive(Debug, Clone)]
pub struct DispatchOptions {
    deadline: Instant,
    request_id: Option<OutboundId>,
    withheld: Arc<Withheld>,
}

impl DispatchOptions {
    /// Options with `deadline` as the instant the node must have answered by
    /// (§11.5).
    #[must_use]
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            request_id: None,
            withheld: Arc::new(Withheld::none()),
        }
    }

    /// These options refusing to send a request that carries one of the
    /// identifiers `withheld` (§5.4.1, N33).
    #[must_use]
    pub fn with_withheld(mut self, withheld: Arc<Withheld>) -> Self {
        self.withheld = withheld;
        self
    }

    /// These options sending `request_id` to the node in
    /// [`REQUEST_ID_HEADER`].
    ///
    /// The id is one the gateway minted, never a client value, so no client
    /// text reaches a node in this header (§5.4.1, N33).
    #[must_use]
    pub fn with_request_id(mut self, request_id: OutboundId) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// The instant the node must have answered by.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// The `openehr-its` call options for these options.
    fn call_options(&self) -> Result<CallOptions, ClientError> {
        let options = CallOptions::default().with_deadline(self.deadline);
        match self.request_id {
            Some(id) => options.with_header(REQUEST_ID_HEADER, &id.to_string()),
            None => Ok(options),
        }
    }
}

impl<T: Transport> NodeClient<T> {
    /// The outbound gate: refuses `query` when the request it composes would
    /// carry a withheld identifier (§5.4.1, N33).
    fn gate(&self, query: &NodeQuery, options: &DispatchOptions) -> Result<(), DispatchError> {
        if options.withheld.is_empty() {
            return Ok(());
        }
        let url = format!("{}/query/aql", self.client.base());
        let paging: Vec<String> = [query.offset, query.fetch]
            .into_iter()
            .flatten()
            .map(|number| number.to_string())
            .collect();
        let request_id = options.request_id.map(|id| id.to_string());
        let headers: Vec<(&'static str, &str)> = request_id
            .as_deref()
            .map(|id| (REQUEST_ID_HEADER, id))
            .into_iter()
            .collect();
        let outbound = Outbound {
            aql: query.aql(),
            scope: query.scope.as_deref(),
            paging: &paging,
            url: &url,
            headers: &headers,
        };
        match options.withheld.found_in(&outbound) {
            Some(part) => Err(DispatchError::Withheld {
                endpoint: self.endpoint.clone(),
                part,
            }),
            None => Ok(()),
        }
    }
}

/// What one node made of one dispatched query.
#[derive(Debug, Clone)]
pub enum NodeReply {
    /// The node answered with a result set: the endpoint is `active`.
    Answered {
        /// The node's answer, as the ITS-REST `RESULT_SET` it sent.
        result_set: Box<ResultSet>,
        /// The gateway's measurement of the request, in milliseconds.
        latency_ms: u64,
    },
    /// The node was asked and gave no result set; the outcome is `offline`,
    /// `time-out` or `node-error`, always with its `error`.
    Failed {
        /// The endpoint outcome, carrying the error and the latency.
        outcome: Outcome,
    },
}

impl NodeReply {
    /// The endpoint outcome this reply reports in `meta.federation`.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Answered { latency_ms, .. } => Outcome::Active {
                latency_ms: *latency_ms,
            },
            Self::Failed { outcome } => outcome.clone(),
        }
    }

    /// The §11.1 status this reply reports.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        match self {
            Self::Answered { .. } => EndpointStatus::Active,
            Self::Failed { outcome } => outcome.status(),
        }
    }
}

/// A client could not be built for an endpoint of the registry snapshot.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SetupError {
    /// The endpoint's base URL cannot carry the ITS-REST paths.
    #[error("the base URL of endpoint {endpoint} cannot carry the ITS-REST paths")]
    BaseUrl {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// Credentials were configured for an endpoint the snapshot does not hold.
    #[error("credentials are configured for {endpoint}, which is not an endpoint of the registry")]
    UnknownEndpoint {
        /// The endpoint id the credentials were keyed by.
        endpoint: EndpointId,
    },
}

/// A dispatch failed on the gateway's side before any request reached the
/// node, so there is no endpoint status to report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DispatchError {
    /// The credentials provider produced no credential for the onward grant.
    #[error("no credential could be obtained for endpoint {endpoint}")]
    Credentials {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// The outbound gate found a withheld patient identifier in the request
    /// the gateway composed, so the request was not sent (§5.4.1, N33).
    #[error(
        "the request to endpoint {endpoint} would carry a patient identifier in {part}, so it was not sent"
    )]
    Withheld {
        /// The endpoint.
        endpoint: EndpointId,
        /// The part of the request that carried it; never the value.
        part: Part,
    },
    /// The request could not be composed: a body that would not serialize, or
    /// another request the client runtime refuses to build.
    #[error("the request to endpoint {endpoint} could not be composed")]
    Compose {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
}

/// The ITS-REST client of one registry endpoint.
#[derive(Debug, Clone)]
pub struct NodeClient<T> {
    endpoint: EndpointId,
    client: Client<T>,
}

impl<T: Transport> NodeClient<T> {
    /// The client of `endpoint` over `transport`, rooted at the endpoint's
    /// base URL with the ITS-REST version segment under it.
    ///
    /// The base URL is used as the registry holds it, with no prefix assumed
    /// (N28): `https://cdr.example.org/openehr` addresses
    /// `https://cdr.example.org/openehr/v1/query/aql`. The client sends a
    /// request once; there is no retry inside the client's budget (no
    /// specification governs this: our own design).
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::BaseUrl`] when the base URL cannot carry a path.
    pub fn new(endpoint: &Endpoint, transport: T) -> Result<Self, SetupError> {
        let base = versioned_base(endpoint.url());
        let client = Client::new(transport, base)
            .map_err(|source| SetupError::BaseUrl {
                endpoint: endpoint.id().clone(),
                source: Box::new(source),
            })?
            .with_retry(RetryPolicy {
                max_attempts: 1,
                ..RetryPolicy::default()
            });
        Ok(Self {
            endpoint: endpoint.id().clone(),
            client,
        })
    }

    /// This client asking `provider` for the onward credentials of every
    /// request.
    #[must_use]
    pub fn with_credentials_provider(mut self, provider: SharedCredentials) -> Self {
        self.client = self.client.with_credentials_provider(provider);
        self
    }

    /// The endpoint this client dispatches to.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointId {
        &self.endpoint
    }

    /// The ITS-REST service root every path is resolved under.
    #[must_use]
    pub fn base(&self) -> &Url {
        self.client.base()
    }

    /// Sends `query` to the node and classifies the answer.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when the request could not leave the gateway:
    /// a withheld identifier in the request ([`DispatchError::Withheld`], with
    /// nothing sent), no credential, or a body the client runtime refuses. Every answer, and every failure to reach the node, is a
    /// [`NodeReply`].
    pub async fn query(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<NodeReply, DispatchError> {
        self.gate(query, options)?;
        let call = options
            .call_options()
            .map_err(|source| DispatchError::Compose {
                endpoint: self.endpoint.clone(),
                source: Box::new(source),
            })?;
        let params = QueryExecuteAdhocQueryBodyParams {
            accept: None,
            content_type: None,
        };
        let started = Instant::now();
        let answer = QueryClient::new(&self.client)
            .with_options(call)
            .query_execute_adhoc_query_body(&params, &query.body())
            .await;
        let latency_ms = elapsed_ms(started);
        match answer {
            Ok(outcome) => Ok(narrow(answered(outcome, latency_ms), query.width)),
            Err(error) => failed(&self.endpoint, error, latency_ms),
        }
    }
}

/// `reply`, or a `node-error` when one of its rows has fewer than `width`
/// cells (§11.1).
fn narrow(reply: NodeReply, width: usize) -> NodeReply {
    let NodeReply::Answered {
        result_set,
        latency_ms,
    } = reply
    else {
        return reply;
    };
    match result_set
        .rows
        .iter()
        .map(Vec::len)
        .find(|found| *found < width)
    {
        Some(found) => NodeReply::Failed {
            outcome: Outcome::NodeError {
                latency_ms,
                error: text(format!(
                    "the node answered a row with {found} cells where the dispatched query selects {width}"
                )),
            },
        },
        None => NodeReply::Answered {
            result_set,
            latency_ms,
        },
    }
}

/// One [`NodeClient`] per endpoint of a registry snapshot.
#[derive(Debug, Clone)]
pub struct NodeClients<T> {
    clients: BTreeMap<EndpointId, NodeClient<T>>,
}

impl<T: Transport + Clone> NodeClients<T> {
    /// A client for every endpoint of `snapshot`, each over a clone of
    /// `transport` (one connection pool), with the onward credentials of
    /// `credentials` where an endpoint has them.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::BaseUrl`] when an endpoint's base URL cannot carry
    /// a path, and [`SetupError::UnknownEndpoint`] when `credentials` names an
    /// endpoint the snapshot does not hold.
    pub fn from_snapshot(
        snapshot: &RegistrySnapshot,
        transport: &T,
        credentials: &BTreeMap<EndpointId, SharedCredentials>,
    ) -> Result<Self, SetupError> {
        if let Some(stray) = credentials
            .keys()
            .find(|endpoint| snapshot.endpoint(endpoint).is_none())
        {
            return Err(SetupError::UnknownEndpoint {
                endpoint: stray.clone(),
            });
        }
        let mut clients = BTreeMap::new();
        for endpoint in snapshot.endpoints() {
            let mut client = NodeClient::new(endpoint, transport.clone())?;
            if let Some(provider) = credentials.get(endpoint.id()) {
                client = client.with_credentials_provider(Arc::clone(provider));
            }
            clients.insert(endpoint.id().clone(), client);
        }
        Ok(Self { clients })
    }

    /// The client of `endpoint`, when the snapshot holds it.
    #[must_use]
    pub fn get(&self, endpoint: &EndpointId) -> Option<&NodeClient<T>> {
        self.clients.get(endpoint)
    }

    /// Every client, in endpoint id order.
    pub fn iter(&self) -> impl Iterator<Item = &NodeClient<T>> {
        self.clients.values()
    }

    /// The number of clients.
    #[must_use]
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    /// Whether the snapshot held no endpoint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// `base` with the ITS-REST version segment appended to its path, the base
/// itself otherwise unchanged (N28).
fn versioned_base(base: &Url) -> Url {
    let mut versioned = base.clone();
    let root = base.path().trim_end_matches('/');
    versioned.set_path(&format!("{root}/{API_VERSION_SEGMENT}"));
    versioned
}

/// The milliseconds since `started`, saturating at `u64::MAX`.
fn elapsed_ms(started: Instant) -> u64 {
    // NOTE: §9.5 reports latency in whole milliseconds; a duration past
    // u64::MAX ms cannot occur inside any budget, so saturating loses nothing.
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The reply for a documented answer of `POST /query/aql`.
fn answered(outcome: QueryExecuteAdhocQueryBodyOutcome, latency_ms: u64) -> NodeReply {
    match outcome {
        QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => NodeReply::Answered {
            result_set: Box::new(body),
            latency_ms,
        },
        QueryExecuteAdhocQueryBodyOutcome::BadRequest { body } => {
            node_error(latency_ms, "400 Bad Request", &body)
        }
        QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { body } => {
            node_error(latency_ms, "408 Request Timeout", &body)
        }
    }
}

/// The reply, or the gateway-side error, for a call that reached no
/// documented answer.
fn failed(
    endpoint: &EndpointId,
    error: ClientError,
    latency_ms: u64,
) -> Result<NodeReply, DispatchError> {
    let failure = |outcome| Ok(NodeReply::Failed { outcome });
    match error {
        ClientError::DeadlineElapsed { .. } => failure(Outcome::TimeOut {
            latency_ms,
            error: text("no answer before the deadline, which passed before the request was sent"),
        }),
        ClientError::Transport {
            source: TransportError::Timeout { source },
            ..
        } => failure(Outcome::TimeOut {
            latency_ms,
            error: text(format!(
                "no answer before the deadline: {}",
                chain(&*source)
            )),
        }),
        ClientError::Transport {
            source: TransportError::Send { source },
            ..
        } => failure(Outcome::Offline {
            latency_ms,
            error: text(format!(
                "the node could not be reached: {}",
                chain(&*source)
            )),
        }),
        ClientError::Unauthorized { body, .. } => {
            Ok(node_error(latency_ms, "401 Unauthorized", &body))
        }
        ClientError::Forbidden { body, .. } => Ok(node_error(latency_ms, "403 Forbidden", &body)),
        ClientError::ServiceFailure { status, body, .. }
        | ClientError::UndocumentedStatus { status, body, .. } => {
            Ok(node_error(latency_ms, &status.to_string(), &body))
        }
        ClientError::Body { status, source, .. } => failure(Outcome::NodeError {
            latency_ms,
            error: text(format!(
                "the node answered {status} with a body that is not an ITS-REST RESULT_SET (at `{}`, a {:?} defect)",
                source.path(),
                source.inner().classify(),
            )),
        }),
        credentials @ ClientError::Credentials { .. } => Err(DispatchError::Credentials {
            endpoint: endpoint.clone(),
            source: Box::new(credentials),
        }),
        other => Err(DispatchError::Compose {
            endpoint: endpoint.clone(),
            source: Box::new(other),
        }),
    }
}

/// A `node-error` reply carrying the node's `status` and its own message.
fn node_error(latency_ms: u64, status: &str, body: &ErrorBody) -> NodeReply {
    let message = match body.message().or_else(|| body.text()) {
        Some(said) if !said.trim().is_empty() => {
            format!("the node answered {status}: {}", bounded(said.trim()))
        }
        _ => format!("the node answered {status}"),
    };
    NodeReply::Failed {
        outcome: Outcome::NodeError {
            latency_ms,
            error: text(message),
        },
    }
}

/// An `error` message; every message here starts with fixed text, so it is
/// never empty.
fn text(message: impl Into<String>) -> ErrorDetail {
    ErrorDetail::Text(message.into())
}

/// `error` and its causes, joined, so the reason a node was unreachable is
/// kept (the engine reports it, never only "offline").
fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = error.to_string();
    let mut next = error.source();
    while let Some(cause) = next {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        next = cause.source();
    }
    out
}

/// At most [`MESSAGE_LIMIT`] characters of `said`, cut on a character
/// boundary.
fn bounded(said: &str) -> String {
    let mut kept: String = said.chars().take(MESSAGE_LIMIT).collect();
    if said.chars().nth(MESSAGE_LIMIT).is_some() {
        kept.push('…');
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::{DispatchError, MESSAGE_LIMIT, bounded, failed, versioned_base};
    use ferrofed_registry::id::EndpointId;
    use http::Method;
    use openehr_its::rest::client::ClientError;
    use url::Url;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Whether the classification of `error` is a gateway-side compose
    /// failure, the one place a call no node answered can land.
    fn is_compose(error: ClientError) -> Result<bool, Box<dyn std::error::Error>> {
        let endpoint = EndpointId::new("node-a-pub")?;
        Ok(matches!(
            failed(&endpoint, error, 0),
            Err(DispatchError::Compose { .. })
        ))
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn the_client_errors_no_node_can_cause_are_compose_failures() -> TestResult {
        let base = Url::parse("data:text/plain,not-a-base")?;
        assert!(is_compose(ClientError::BaseUrl { base })?);
        let build = http::Request::builder().uri("http://[::1").body(());
        let Err(source) = build else {
            return Err("an unparsable URI built a request".into());
        };
        assert!(is_compose(ClientError::Build {
            method: Method::POST,
            path: "/query/aql".to_owned(),
            source,
        })?);
        let Err(source) = http::HeaderName::from_bytes(b"not a name") else {
            return Err("an illegal header name parsed".into());
        };
        assert!(is_compose(ClientError::HeaderName {
            header: "not a name".to_owned(),
            source,
        })?);
        let Err(source) = http::HeaderValue::from_str("line\nbreak") else {
            return Err("a line break parsed as a header value".into());
        };
        assert!(is_compose(ClientError::HeaderValue {
            header: "X-Request-Id".to_owned(),
            source,
        })?);
        assert!(is_compose(ClientError::UnsupportedMediaType {
            requested: "application/xml".to_owned(),
        })?);
        let Err(source) = serde_json::from_str::<u8>("not json") else {
            return Err("an illegal JSON text parsed".into());
        };
        assert!(is_compose(ClientError::Serialize { source })?);
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn the_version_segment_is_appended_once() -> TestResult {
        assert_eq!(
            versioned_base(&Url::parse("https://cdr.example.org/openehr")?).as_str(),
            "https://cdr.example.org/openehr/v1"
        );
        assert_eq!(
            versioned_base(&Url::parse("https://cdr.example.org/openehr/")?).as_str(),
            "https://cdr.example.org/openehr/v1"
        );
        assert_eq!(
            versioned_base(&Url::parse("https://cdr.example.org")?).as_str(),
            "https://cdr.example.org/v1"
        );
        Ok(())
    }

    #[test]
    fn a_long_node_message_is_cut_on_a_character_boundary() {
        let said = "é".repeat(MESSAGE_LIMIT + 10);
        let kept = bounded(&said);
        assert_eq!(kept.chars().count(), MESSAGE_LIMIT + 1);
        assert!(kept.ends_with('…'));
        assert_eq!(bounded("short"), "short");
    }
}
