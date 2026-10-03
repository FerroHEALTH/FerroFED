// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Node dispatch: one ITS-REST client per registry endpoint, and the mapping
//! from what a node answered to exactly one §11.1 endpoint status (N16, N40).
//!
//! Every query to a node is the generated `query_execute_adhoc_query_body`
//! (`POST {base}/v1/query/aql`) of `openehr-its`'s `rest-client`, and every
//! stored-query definition sent to or read from a node is a generated call
//! too ([`definition`]), so the request line, the headers and the body are
//! composed by that runtime and nowhere in FerroFED (no specification governs
//! this: our own design). This
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
//! A `node-error` carries the node's own status and an excerpt of its
//! message ([`reported`], §9.5, §11.2), never folded into `offline`; a
//! refused connection still carries its reason. A
//! failure on the gateway's side before any request left (a credential the
//! provider could not produce, a body that would not serialize) is a
//! [`DispatchError`], never an endpoint status: nothing was sent to report on.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::hygiene::{Part, Withheld};
use crate::outbound_id::OutboundId;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::outcome::Outcome;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{
    CallOptions, Client, ClientError, CredentialsProvider, RetryPolicy, Transport,
};
use openehr_its::rest::generated::query::client::QueryClient;
use openehr_its::rest::generated::query::{
    AdhocQueryExecute, QueryExecuteAdhocQueryBodyParams, ResultSet,
};
use url::Url;

mod classify;
pub mod definition;
mod gate;
pub mod reported;

/// The API version segment ITS-REST 1.1.0 puts every path under
/// (`{baseUrl}/v1/...`), appended to the endpoint's base URL.
pub const API_VERSION_SEGMENT: &str = "v1";

/// The header that carries the gateway's [`OutboundId`] to a node.
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

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
    composed_ehr_id: Option<EhrId>,
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
            composed_ehr_id: None,
        }
    }

    /// These options naming the node's own `ehr_id`, which the gateway
    /// composed into a forwarded request's path as `/ehr/{ehr_id}` from a
    /// resolution or the `ehr_id` index, never from the client.
    ///
    /// The outbound gate masks that one path segment, as it masks the scope
    /// literal of a dispatched query (§5.4.1, N33).
    #[must_use]
    pub fn with_composed_ehr_id(mut self, ehr_id: EhrId) -> Self {
        self.composed_ehr_id = Some(ehr_id);
        self
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

    /// The identifiers no request may carry.
    pub(crate) fn withheld(&self) -> &Withheld {
        &self.withheld
    }

    /// The `ehr_id` the gateway composed into a forwarded request's path.
    pub(crate) fn composed_ehr_id(&self) -> Option<&EhrId> {
        self.composed_ehr_id.as_ref()
    }

    /// The `openehr-its` call options for these options.
    pub(crate) fn call_options(&self) -> Result<CallOptions, ClientError> {
        let options = CallOptions::default().with_deadline(self.deadline);
        match self.request_id {
            Some(id) => options.with_header(REQUEST_ID_HEADER, &id.to_string()),
            None => Ok(options),
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

    /// The ITS-REST client every request to the node is sent through.
    pub(crate) fn client(&self) -> &Client<T> {
        &self.client
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
        let latency_ms = classify::elapsed_ms(started);
        match answer {
            Ok(outcome) => Ok(classify::narrow(
                classify::answered(outcome, latency_ms, options.withheld()),
                query.width,
            )),
            Err(error) => classify::failed(&self.endpoint, error, latency_ms, options.withheld()),
        }
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

#[cfg(test)]
mod tests {
    use super::versioned_base;
    use url::Url;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

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
}
