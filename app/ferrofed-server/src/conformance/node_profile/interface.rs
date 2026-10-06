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
use openehr_its::rest::generated::query::{AdhocQueryExecute, QueryParameters};
use openehr_query::ast::Primitive;
use openehr_query::bind::{BindError, Parameters, bind};
use openehr_query::parser::{ParseError, parse_str};
use openehr_query::printer::to_aql;
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
    /// A check's query template is no AQL the parser reads.
    #[error("a check's query template is no AQL")]
    Template(#[source] ParseError),
    /// A check's query template does not take the `ehr_id` as its one
    /// parameter, `$ehr_id`.
    #[error("a check's query template does not take the ehr_id as $ehr_id alone")]
    Parameter(#[source] BindError),
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

impl CheckError {
    /// Returns what happened to the check, worded for a finding: "the check
    /// reached no answer" only when a request was sent and none came back.
    #[must_use]
    pub fn outcome(&self) -> &'static str {
        match self {
            Self::NoClient(_) => "found no node client for the endpoint",
            Self::Unconveyed(_) => "could not convey the gateway to the node",
            Self::Clock => "could not set a deadline within the clock's range",
            Self::Body(_) => "could not write its query body",
            Self::Template(_) => "holds a query template that is no AQL",
            Self::Parameter(_) => "holds a query template that does not take the ehr_id alone",
            Self::Unanswered { .. } => "reached no answer",
        }
    }
}

/// The name of the parameter that carries the `ehr_id` of a scoped query.
const EHR_ID: &str = "ehr_id";

/// One AQL query a check sends, with the parameter values that travel beside
/// it.
#[derive(Debug, Clone)]
pub(super) struct NodeQuery {
    /// The query text.
    aql: String,
    /// The ITS-REST `query_parameters`, when the query takes any.
    parameters: Option<QueryParameters>,
}

impl NodeQuery {
    /// Creates the query `template` scoped to `ehr_id`: the template, parsed
    /// and printed with its `$ehr_id` parameter, and the `ehr_id` as that
    /// parameter's value.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::Template`] when `template` is no AQL, and
    /// [`CheckError::Parameter`] when it takes a parameter other than
    /// `$ehr_id`, or none.
    // NOTE: AQL §Parameters and ITS-REST Query API `query_parameters`: the ehr_id travels beside
    // the query as a parameter value, so no value is ever spliced into the AQL text.
    pub(super) fn scoped(template: &str, ehr_id: Uuid) -> Result<Self, CheckError> {
        let query = parse_str(template).map_err(CheckError::Template)?;
        let value = ehr_id.to_string();
        let mut parameters = Parameters::new();
        parameters.insert(EHR_ID, Primitive::String(value.clone()));
        bind(&mut query.clone(), &parameters).map_err(CheckError::Parameter)?;
        Ok(Self {
            aql: to_aql(&query),
            parameters: Some(QueryParameters::from([(EHR_ID.to_owned(), value.into())])),
        })
    }

    /// Creates the query `text` as written, for a check whose subject is a
    /// query no AQL grammar parses, so it takes no parameter.
    pub(super) fn unparsable(text: &'static str) -> Self {
        Self {
            aql: text.to_owned(),
            parameters: None,
        }
    }
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

    /// Sends `query` as `POST /query/aql`, its parameters as the body's
    /// `query_parameters`, as `principal` or the endpoint's onward
    /// credentials.
    pub(super) async fn query(
        &self,
        query: &NodeQuery,
        principal: Option<&Principal>,
    ) -> Result<Answer, CheckError> {
        let body = AdhocQueryExecute {
            q: query.aql.clone(),
            offset: None,
            fetch: None,
            query_parameters: query.parameters.clone(),
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::super::checks::{COMPOSITION_QUERY, EHR_PREDICATE_QUERY, EHR_QUERY};
    use super::{CheckError, NodeQuery};

    const EHR: Uuid = Uuid::from_u128(0x9393_9393_9393_4393_8393_9393_9393_9393);

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_scoped_query_carries_the_ehr_id_as_its_parameter_alone() -> Result<(), CheckError> {
        for template in [EHR_QUERY, EHR_PREDICATE_QUERY, COMPOSITION_QUERY] {
            let query = NodeQuery::scoped(template, EHR)?;
            assert!(query.aql.contains("$ehr_id"), "{}", query.aql);
            assert!(!query.aql.contains(&EHR.to_string()), "{}", query.aql);
            let parameters = query.parameters.unwrap_or_default();
            assert_eq!(vec!["ehr_id"], parameters.keys().collect::<Vec<_>>());
            assert_eq!(
                Some(EHR.to_string().as_str()),
                parameters.get("ehr_id").and_then(|value| value.as_str())
            );
        }
        Ok(())
    }

    #[test]
    fn a_template_that_does_not_take_the_ehr_id_as_ehr_id_is_refused() {
        for template in [
            "SELECT e/ehr_id/value FROM EHR e",
            "SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_id/value = $id",
            "SELECT e/ehr_id/value FROM EHR e \
             WHERE e/ehr_id/value = $ehr_id AND e/system_id/value = $system",
        ] {
            assert!(
                matches!(
                    NodeQuery::scoped(template, EHR),
                    Err(CheckError::Parameter(_))
                ),
                "{template}"
            );
        }
        assert!(matches!(
            NodeQuery::scoped("SELECT FROM EHR e WHERE", EHR),
            Err(CheckError::Template(_))
        ));
    }
}
