// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST interface of one member, as the node profile checks reach it.
//!
//! It is the endpoint's [`NodeClient`], with its onward credentials, its
//! outbound gate and the gateway's own identity conveyed, as every request
//! the gateway composes for a node goes out (§5.4.1, §13.1; N24, N25, N33).
//!
//! Each request leaves through [`NodeClient::forward`], which returns the
//! node's status and body as the node sent them, so a status ITS-REST does
//! not document for an operation is evidence about the node, never an error
//! of the check. A `401` the client reports as a refusal of the onward
//! credentials is read as the status it is.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, SharedCredentials};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError};
use ferrofed_registry::id::EndpointId;
use http::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use openehr_its::rest::generated::query::AdhocQueryExecute;
use uuid::Uuid;

use crate::conveyed::{self, Unconveyed};
use crate::federation::Federation;
use crate::onward::NodeTransport;

/// A principal a node's own policy decides for, with the credentials that
/// present it to the node.
///
/// `Debug` shows the name alone.
#[derive(Clone)]
pub struct Principal {
    /// The principal's name, for an evidence line.
    name: String,
    /// The credentials every request as this principal carries.
    credentials: SharedCredentials,
}

impl Principal {
    /// Creates the principal `name`, presented to the node by `credentials`.
    #[must_use]
    pub fn new(name: impl Into<String>, credentials: SharedCredentials) -> Self {
        Self {
            name: name.into(),
            credentials,
        }
    }

    /// Returns the principal's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Principal")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// A refusal the operator arranged at the node: an EHR the node holds,
/// served to the endpoint's onward credentials and refused to another
/// principal.
#[derive(Debug, Clone)]
pub struct Arrangement {
    /// The EHR.
    ehr_id: Uuid,
    /// The principal the node refuses it to.
    refused: Principal,
}

impl Arrangement {
    /// Creates the arrangement over `ehr_id`, served to the endpoint's
    /// onward credentials and refused to `refused`.
    #[must_use]
    pub fn new(ehr_id: Uuid, refused: Principal) -> Self {
        Self { ehr_id, refused }
    }

    /// Returns the EHR the arrangement is over.
    #[must_use]
    pub fn ehr_id(&self) -> Uuid {
        self.ehr_id
    }

    /// Returns the principal the node refuses the EHR to.
    #[must_use]
    pub fn refused(&self) -> &Principal {
        &self.refused
    }
}

/// A check could not observe the node at all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The endpoint has no node client in the federation.
    #[error("endpoint {0} has no node client in the federation")]
    NoClient(EndpointId),
    /// The gateway cannot convey itself to the node, so nothing is sent
    /// (§13.1, N24).
    #[error("the checks cannot convey the gateway to the node")]
    Unconveyed(#[source] Unconveyed),
    /// The per-node budget added to the current instant passes the range of
    /// the clock.
    #[error("the per-node budget passes the range of the clock")]
    Clock,
    /// The query body could not be written.
    #[error("the query body could not be written")]
    Body(#[source] serde_json::Error),
    /// A request reached no answer of the node's.
    #[error("{step} reached no answer of endpoint {endpoint}")]
    Unanswered {
        /// The request, as method and path.
        step: String,
        /// The endpoint.
        endpoint: EndpointId,
        /// What the node client reported.
        #[source]
        source: ForwardError,
    },
}

/// What the node answered one request.
#[derive(Debug)]
pub(super) struct Answer {
    /// The status.
    pub(super) status: StatusCode,
    /// The `Location` header, when there is one.
    pub(super) location: Option<String>,
    /// The body.
    pub(super) body: Vec<u8>,
}

/// The ITS-REST interface of one node, as the checks reach it.
#[derive(Debug, Clone)]
pub struct Interface {
    /// The endpoint's node client, with its onward credentials.
    client: NodeClient<NodeTransport>,
    /// The gateway's own identity, conveyed on every request.
    conveyance: Conveyance,
    /// The time each request has to answer.
    budget: Duration,
    /// The identifiers the outbound gate withholds from every request.
    withheld: Arc<Withheld>,
}

impl Interface {
    /// Creates the interface of `endpoint` in `federation`: its node client,
    /// the gateway's identity and the per-node budget.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::NoClient`] when the federation holds no client
    /// for `endpoint`, and [`CheckError::Unconveyed`] when it holds no
    /// signer.
    pub fn of(federation: &Federation, endpoint: &EndpointId) -> Result<Self, CheckError> {
        let client = federation
            .clients()
            .get(endpoint)
            .ok_or_else(|| CheckError::NoClient(endpoint.clone()))?
            .clone();
        Ok(Self {
            client,
            conveyance: conveyed::gateway(federation).map_err(CheckError::Unconveyed)?,
            budget: federation.budget().per_node(),
            withheld: Arc::new(Withheld::none()),
        })
    }

    /// Returns the interface with the outbound gate withholding `withheld`
    /// from every request, as a run withholds its patient (§5.4.1, N33).
    #[must_use]
    pub fn with_withheld(mut self, withheld: Arc<Withheld>) -> Self {
        self.withheld = withheld;
        self
    }

    /// Returns the endpoint the interface reaches.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointId {
        self.client.endpoint()
    }

    /// Returns the interface presenting `principal`'s credentials in place
    /// of the endpoint's onward credentials.
    fn as_principal(&self, principal: &Principal) -> Self {
        Self {
            client: self
                .client
                .clone()
                .with_credentials_provider(Arc::clone(&principal.credentials)),
            conveyance: self.conveyance.clone(),
            budget: self.budget,
            withheld: Arc::clone(&self.withheld),
        }
    }

    /// Sends `GET /{path}`, as `principal` or the endpoint's onward
    /// credentials.
    pub(super) async fn get(
        &self,
        path: &str,
        principal: Option<&Principal>,
    ) -> Result<Answer, CheckError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        self.send(
            Method::GET,
            format!("/{path}"),
            headers,
            Vec::new(),
            principal,
        )
        .await
    }

    /// Sends `POST /ehr` with no body, asking for the created EHR in the
    /// answer.
    pub(super) async fn create_ehr(&self) -> Result<Answer, CheckError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert("prefer", HeaderValue::from_static("return=representation"));
        self.send(Method::POST, "/ehr".to_owned(), headers, Vec::new(), None)
            .await
    }

    /// Sends `aql` as `POST /query/aql`, as `principal` or the endpoint's
    /// onward credentials.
    pub(super) async fn query(
        &self,
        aql: &str,
        principal: Option<&Principal>,
    ) -> Result<Answer, CheckError> {
        let body = AdhocQueryExecute {
            q: aql.to_owned(),
            offset: None,
            fetch: None,
            query_parameters: None,
            additional_properties: BTreeMap::new(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let body = serde_json::to_vec(&body).map_err(CheckError::Body)?;
        self.send(
            Method::POST,
            "/query/aql".to_owned(),
            headers,
            body,
            principal,
        )
        .await
    }

    /// Sends one request through the node client, as `principal` or the
    /// endpoint's onward credentials, and reads the node's answer.
    async fn send(
        &self,
        method: Method,
        path: String,
        headers: HeaderMap,
        body: Vec<u8>,
        principal: Option<&Principal>,
    ) -> Result<Answer, CheckError> {
        let presented;
        let interface = match principal {
            Some(principal) => {
                presented = self.as_principal(principal);
                &presented
            }
            None => self,
        };
        let step = format!("{method} {path}");
        let deadline = Instant::now()
            .checked_add(interface.budget)
            .ok_or(CheckError::Clock)?;
        let options = DispatchOptions::new(deadline, interface.conveyance.clone())
            .with_withheld(Arc::clone(&interface.withheld))
            .with_request_id(OutboundId::mint());
        let request = ClientRequest {
            method,
            path,
            query: None,
            headers,
            body,
        };
        match interface.client.forward(request, &options).await {
            Ok(forwarded) => {
                let (status, headers, body) = forwarded.into_parts();
                let location = headers
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                Ok(Answer {
                    status,
                    location,
                    body,
                })
            }
            // NOTE: no specification governs the checks: our own design; a 401 refuses the
            // principal the check presented, which is the node's answer and so its evidence.
            Err(ForwardError::Refused { status, .. }) => Ok(Answer {
                status,
                location: None,
                body: Vec::new(),
            }),
            Err(source) => Err(CheckError::Unanswered {
                step,
                endpoint: interface.endpoint().clone(),
                source,
            }),
        }
    }
}
