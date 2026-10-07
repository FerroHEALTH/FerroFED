// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's client of the FerroFED gateway: the gateway's public surface
//! over HTTP, and nothing else.
//!
//! Every call goes through the `openehr-its` client, sent as the signed-in
//! operator with the operator's own access token, so the gateway
//! authenticates the console's requests exactly as it does any other
//! client's. [`Gateway::its`] is the ITS-REST surface at `{base}/v1`, for the
//! generated group clients of `openehr_its::rest::generated`;
//! [`Gateway::self_description`] reads `OPTIONS {base}/` into the typed
//! federation body (§7a.2, N30), [`Gateway::dependencies`] the health of
//! each member and service into the report of `ferrofed_registry::health`,
//! and the operator reads the gateway's read-only operator surface into the
//! reports of `ferrofed_registry::operator`. A refusal or a failure is a
//! typed [`GatewayError`] carrying the gateway's status, never an empty
//! answer. [`Gateway::query`] runs an AQL or a stored query and reads the
//! federated `RESULT_SET` with its `meta.federation` (§9.1, §11.4).

use std::collections::BTreeMap;
use std::fmt;

use ferrofed_registry::health::DependencyReport;
use ferrofed_registry::operator::{CreatingSystemEntry, IncidentReport, Page, PageRequest};
use http::{Method, StatusCode};
use openehr_federation::error::WireError;
use openehr_federation::meta::FederationMeta;
use openehr_federation::options::OptionsRoot;
use openehr_its::rest::client::{
    CallOptions, Client, ClientError, Credentials, ErrorBody, Request, ReqwestTransport,
};
use openehr_its::rest::generated::definition::StoredQuery;
use openehr_its::rest::generated::query::client::{
    QueryClient, QueryExecuteAdhocQueryBodyOutcome, QueryExecuteStoredQueryBodyOutcome,
    QueryExecuteStoredQueryVersionBodyOutcome,
};
use openehr_its::rest::generated::query::{
    AdhocQueryExecute, Query, QueryExecuteAdhocQueryBodyParams, QueryExecuteStoredQueryBodyParams,
    QueryExecuteStoredQueryVersionBodyParams, QueryParameters, ResultSet,
};
use secrecy::SecretString;
use url::Url;

use crate::config::settings::GatewaySettings;

/// The operator's access token, sent to the gateway as a bearer credential.
#[derive(Clone)]
pub struct AccessToken(SecretString);

impl AccessToken {
    /// Wraps the token the OpenID Provider issued.
    #[must_use]
    pub fn new(token: SecretString) -> Self {
        Self(token)
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(..)")
    }
}

/// Why a call to the gateway gave no answer the console can use.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The HTTP client could not be built.
    #[error("the gateway client could not be built")]
    Transport {
        /// What the HTTP client refused.
        #[source]
        source: reqwest::Error,
    },
    /// The configured base URL cannot carry the ITS-REST path.
    #[error("the gateway base URL cannot carry the ITS-REST path")]
    Base {
        /// What the URL parser refused.
        #[source]
        source: url::ParseError,
    },
    /// The client refused the base URL, or the call failed on the way.
    #[error("the gateway call failed")]
    Call {
        /// What the client reported, the gateway's status among it.
        #[source]
        source: ClientError,
    },
    /// The gateway answered with a status the call does not expect.
    #[error("the gateway answered {status}")]
    Status {
        /// The status the gateway answered with.
        status: StatusCode,
        /// The body it sent with it.
        body: ErrorBody,
    },
    /// The gateway answered a result set whose `meta.federation` cannot be
    /// read, so whether the answer is complete is unknown (§9.1, N17).
    #[error("the gateway answered {status} with no readable meta.federation")]
    Envelope {
        /// The status the gateway answered with.
        status: StatusCode,
        /// What the federation reader refused.
        #[source]
        source: WireError,
    },
}

impl GatewayError {
    /// The status the gateway answered with and the stable error `code` its
    /// body carries, when the gateway answered at all.
    #[must_use]
    pub fn refusal(&self) -> Option<(StatusCode, Option<String>)> {
        let (status, body) = match self {
            Self::Status { status, body }
            | Self::Call {
                source:
                    ClientError::ServiceFailure { status, body, .. }
                    | ClientError::UndocumentedStatus { status, body, .. },
            } => (*status, body),
            Self::Call {
                source: ClientError::Unauthorized { body, .. },
            } => (StatusCode::UNAUTHORIZED, body),
            Self::Call {
                source: ClientError::Forbidden { body, .. },
            } => (StatusCode::FORBIDDEN, body),
            Self::Transport { .. }
            | Self::Base { .. }
            | Self::Call { .. }
            | Self::Envelope { .. } => return None,
        };
        Some((status, code_of(body)))
    }

    /// The status of a gateway answer whose body the console cannot read,
    /// such as a report of a shape this console does not know.
    #[must_use]
    pub fn unreadable(&self) -> Option<StatusCode> {
        match self {
            Self::Call {
                source: ClientError::Body { status, .. },
            }
            | Self::Envelope { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// What a query run through the gateway names: AQL text, or a stored query.
///
/// Its `Debug` output never prints the AQL text, which can name a patient.
#[derive(Clone, PartialEq, Eq)]
pub enum QueryTarget {
    /// An ad hoc AQL query, `POST {base}/v1/query/aql`.
    Aql(String),
    /// A stored query by its qualified name, at a version when one is named,
    /// `POST {base}/v1/query/{name}` or `/v1/query/{name}/{version}`.
    Stored {
        /// The qualified query name.
        name: String,
        /// The version, when the operator named one.
        version: Option<String>,
    },
}

impl fmt::Debug for QueryTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aql(_) => f.write_str("Aql(..)"),
            Self::Stored { name, version } => f
                .debug_struct("Stored")
                .field("name", name)
                .field("version", version)
                .finish(),
        }
    }
}

/// One query the console runs through the gateway's ITS-REST query surface,
/// as any client would send it.
///
/// Its `Debug` output names the shape of the call alone: the AQL text and the
/// parameter values can name a patient, so neither is ever printed (N33).
#[derive(Clone)]
pub struct QueryCall {
    /// What runs.
    pub target: QueryTarget,
    /// The ITS-REST `offset`, when the operator gave one.
    pub offset: Option<i64>,
    /// The ITS-REST `fetch`, when the operator gave one.
    pub fetch: Option<i64>,
    /// The `query_parameters`, a patient among them where the query names
    /// one; sent in the request body, never in the URL.
    pub parameters: QueryParameters,
    /// The federation request headers, each name with its value: the
    /// targeting (§8.4), the dedup mode (§10) and the completeness opt-in
    /// (§11.4).
    pub headers: Vec<(&'static str, String)>,
}

impl fmt::Debug for QueryCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<&str> = self.headers.iter().map(|(name, _)| *name).collect();
        f.debug_struct("QueryCall")
            .field("target", &self.target)
            .field("parameters", &self.parameters.len())
            .field("headers", &headers)
            .finish_non_exhaustive()
    }
}

/// A federated answer: the status the gateway answered with, its ITS-REST
/// `RESULT_SET`, and the `meta.federation` read out of it (§9.1).
///
/// A failing all-or-nothing answer, a `504` or a `424`, is one as well: it
/// carries the diagnostic envelope with no rows (§11.4, N37, CP-30).
#[derive(Clone)]
pub struct FederatedAnswer {
    /// The status the gateway answered with.
    pub status: StatusCode,
    /// The result set, its `meta` included.
    pub result_set: ResultSet,
    /// Its `meta.federation`.
    pub federation: FederationMeta,
}

impl fmt::Debug for FederatedAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FederatedAnswer")
            .field("status", &self.status)
            .field("rows", &self.result_set.rows.len())
            .field("complete", &self.federation.complete())
            .finish_non_exhaustive()
    }
}

impl FederatedAnswer {
    /// The answer `result_set` carries, answered with `status`.
    ///
    /// # Errors
    /// Returns [`GatewayError::Envelope`] when its `meta.federation` is
    /// missing or not a valid one.
    pub fn read(status: StatusCode, result_set: ResultSet) -> Result<Self, GatewayError> {
        let federation = result_set
            .meta
            .as_ref()
            .ok_or(WireError::MissingMember {
                object: "result set",
                member: "meta",
            })
            .and_then(openehr_federation::envelope::read)
            .map_err(|source| GatewayError::Envelope { status, source })?;
        Ok(Self {
            status,
            result_set,
            federation,
        })
    }
}

/// How a query call ended before its answer was read.
enum Unanswered {
    /// A status the operation documents, with its body.
    Status(StatusCode, ErrorBody),
    /// What the client reported.
    Client(ClientError),
}

impl Unanswered {
    /// The federated answer a failing response carries, or the error it is.
    ///
    /// A failing response whose body is a result set is the diagnostic
    /// envelope of §11.4; any other body is the gateway's ITS-REST error.
    fn into_answer(self) -> Result<FederatedAnswer, GatewayError> {
        let (status, body) = match &self {
            Self::Status(status, body)
            | Self::Client(
                ClientError::ServiceFailure { status, body, .. }
                | ClientError::UndocumentedStatus { status, body, .. },
            ) => (*status, body),
            Self::Client(_) => return Err(self.into_error()),
        };
        // NOTE: §11.4 failure-carries-envelope; a body that is no result set is
        // the ITS-REST error of a refusal, legitimately not an envelope.
        match serde_json::from_slice::<ResultSet>(body.raw()) {
            Ok(result_set) => FederatedAnswer::read(status, result_set),
            Err(_not_a_result_set) => Err(self.into_error()),
        }
    }

    /// The gateway error this is.
    fn into_error(self) -> GatewayError {
        match self {
            Self::Status(status, body) => GatewayError::Status { status, body },
            Self::Client(source) => GatewayError::Call { source },
        }
    }
}

/// The `code` member the gateway writes into its ITS-REST `Error` body.
#[derive(serde::Deserialize)]
struct Coded {
    code: Option<String>,
}

/// The stable error code `body` carries, if it is the gateway's error body.
fn code_of(body: &ErrorBody) -> Option<String> {
    // NOTE: no specification governs this: our own design; a body that is not
    // the gateway's error document carries no code, which is not a failure.
    serde_json::from_slice::<Coded>(body.raw())
        .ok()
        .and_then(|coded| coded.code)
}

/// The gateway the console is a client of.
#[derive(Debug, Clone)]
pub struct Gateway {
    transport: ReqwestTransport,
    base: Url,
    api: Url,
}

impl Gateway {
    /// A client of the gateway `settings` names.
    ///
    /// # Errors
    /// Returns [`GatewayError::Transport`] when the HTTP client cannot be
    /// built and [`GatewayError::Base`] when the base URL cannot carry
    /// `/v1`.
    pub fn new(settings: &GatewaySettings) -> Result<Self, GatewayError> {
        let transport = ReqwestTransport::with_timeout(settings.timeout)
            .map_err(|source| GatewayError::Transport { source })?;
        let mut base = settings.base.clone();
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let api = base
            .join("v1")
            .map_err(|source| GatewayError::Base { source })?;
        Ok(Self {
            transport,
            base,
            api,
        })
    }

    /// The gateway's `{base}`, ending in `/`.
    #[must_use]
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// A client of the gateway's ITS-REST surface at `{base}/v1`, sending
    /// `token`, for the generated group clients.
    ///
    /// # Errors
    /// Returns [`GatewayError::Call`] when the client refuses the URL.
    pub fn its(&self, token: &AccessToken) -> Result<Client<ReqwestTransport>, GatewayError> {
        self.client(self.api.clone(), token)
    }

    /// Reads the gateway's self-description, `OPTIONS {base}/` (§7a.2, N30).
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] for any answer but `200`, and
    /// [`GatewayError::Call`] when the call failed or the body is not a
    /// conformant `OPTIONS {base}/` body.
    pub async fn self_description(&self, token: &AccessToken) -> Result<OptionsRoot, GatewayError> {
        self.read(Method::OPTIONS, "/", token).await
    }

    /// Reads the last state the gateway observed of each member and service,
    /// `GET {base}/operator/dependencies`.
    ///
    /// # Errors
    /// As [`Gateway::self_description`].
    pub async fn dependencies(
        &self,
        token: &AccessToken,
    ) -> Result<DependencyReport, GatewayError> {
        self.read(Method::GET, "/operator/dependencies", token)
            .await
    }

    /// Reads the integrity incidents, `GET {base}/operator/incidents`.
    ///
    /// # Errors
    /// As [`Gateway::self_description`]; a `403` when the operator's token
    /// carries no operator scope.
    pub async fn incidents(&self, token: &AccessToken) -> Result<IncidentReport, GatewayError> {
        self.read(Method::GET, "/operator/incidents", token).await
    }

    /// Reads one page of the `creating_system_id` routing table,
    /// `GET {base}/operator/creating-systems`.
    ///
    /// # Errors
    /// As [`Gateway::incidents`].
    pub async fn creating_systems(
        &self,
        token: &AccessToken,
        page: PageRequest,
    ) -> Result<Page<CreatingSystemEntry>, GatewayError> {
        self.read_page("/operator/creating-systems", token, page)
            .await
    }

    /// Reads one page of the held stored-query versions, as ITS-REST
    /// `StoredQuery`s, `GET {base}/operator/stored-queries`.
    ///
    /// # Errors
    /// As [`Gateway::incidents`].
    pub async fn stored_queries(
        &self,
        token: &AccessToken,
        page: PageRequest,
    ) -> Result<Page<StoredQuery>, GatewayError> {
        self.read_page("/operator/stored-queries", token, page)
            .await
    }

    /// Runs `call` through the gateway's ITS-REST query surface as the
    /// operator, the query and its parameters in the request body.
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for a
    /// refusal or a failure that carries no federated result set,
    /// [`GatewayError::Envelope`] for a result set with no readable
    /// `meta.federation`, and [`GatewayError::Call`] when a header value is
    /// not legal on the wire.
    pub async fn query(
        &self,
        token: &AccessToken,
        call: &QueryCall,
    ) -> Result<FederatedAnswer, GatewayError> {
        let client = self.its(token)?;
        let mut options = CallOptions::default();
        for (name, value) in &call.headers {
            options = options
                .with_header(name, value)
                .map_err(|source| GatewayError::Call { source })?;
        }
        let group = QueryClient::new(&client).with_options(options);
        let parameters = (!call.parameters.is_empty()).then(|| call.parameters.clone());
        let answered = match &call.target {
            QueryTarget::Aql(q) => {
                let params = QueryExecuteAdhocQueryBodyParams {
                    accept: Some(json()),
                    content_type: Some(json()),
                };
                let body = AdhocQueryExecute {
                    q: q.clone(),
                    offset: call.offset,
                    fetch: call.fetch,
                    query_parameters: parameters,
                    additional_properties: BTreeMap::new(),
                };
                match group.query_execute_adhoc_query_body(&params, &body).await {
                    Ok(QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. }) => Ok(body),
                    Ok(QueryExecuteAdhocQueryBodyOutcome::BadRequest { body }) => {
                        Err(Unanswered::Status(StatusCode::BAD_REQUEST, body))
                    }
                    Ok(QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { body }) => {
                        Err(Unanswered::Status(StatusCode::REQUEST_TIMEOUT, body))
                    }
                    Err(source) => Err(Unanswered::Client(source)),
                }
            }
            QueryTarget::Stored { name, version } => {
                let body = Query {
                    offset: call.offset,
                    fetch: call.fetch,
                    query_parameters: parameters,
                    additional_properties: BTreeMap::new(),
                };
                match version {
                    None => stored(&group, name, &body).await,
                    Some(version) => stored_version(&group, name, version, &body).await,
                }
            }
        };
        match answered {
            Ok(result_set) => FederatedAnswer::read(StatusCode::OK, result_set),
            Err(unanswered) => unanswered.into_answer(),
        }
    }

    /// Reads the page `page` of the listing at `path`.
    async fn read_page<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: &AccessToken,
        page: PageRequest,
    ) -> Result<Page<T>, GatewayError> {
        let mut request = Request::new(Method::GET, path.to_owned());
        request.query("offset", page.offset);
        request.query("limit", page.limit);
        self.send(request, token).await
    }

    /// Sends `method` to `path` below `{base}` as the operator and decodes a
    /// `200` answer.
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: &AccessToken,
    ) -> Result<T, GatewayError> {
        self.send(Request::new(method, path.to_owned()), token)
            .await
    }

    /// Sends `request` below `{base}` as the operator and decodes a `200`
    /// answer.
    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        request: Request,
        token: &AccessToken,
    ) -> Result<T, GatewayError> {
        let client = self.client(self.base.clone(), token)?;
        let answer = client
            .execute(request)
            .await
            .map_err(|source| GatewayError::Call { source })?;
        if answer.status() != StatusCode::OK {
            return Err(GatewayError::Status {
                status: answer.status(),
                body: answer.error_body(),
            });
        }
        answer
            .json()
            .map_err(|source| GatewayError::Call { source })
    }

    /// A client rooted at `root`, sending `token`.
    fn client(
        &self,
        root: Url,
        token: &AccessToken,
    ) -> Result<Client<ReqwestTransport>, GatewayError> {
        Client::new(self.transport.clone(), root)
            .map(|client| client.with_credentials(Credentials::bearer(token.0.clone())))
            .map_err(|source| GatewayError::Call { source })
    }
}

/// The `Accept` and `Content-Type` every query call sends.
fn json() -> String {
    String::from("application/json")
}

/// Runs the stored query `name` at its latest version.
async fn stored(
    group: &QueryClient<'_, ReqwestTransport>,
    name: &str,
    body: &Query,
) -> Result<ResultSet, Unanswered> {
    let params = QueryExecuteStoredQueryBodyParams {
        qualified_query_name: name.to_owned(),
        accept: Some(json()),
        content_type: Some(json()),
    };
    match group.query_execute_stored_query_body(&params, body).await {
        Ok(QueryExecuteStoredQueryBodyOutcome::Ok { body, .. }) => Ok(body),
        Ok(QueryExecuteStoredQueryBodyOutcome::BadRequest { body }) => {
            Err(Unanswered::Status(StatusCode::BAD_REQUEST, body))
        }
        Ok(QueryExecuteStoredQueryBodyOutcome::NotFound { body }) => {
            Err(Unanswered::Status(StatusCode::NOT_FOUND, body))
        }
        Ok(QueryExecuteStoredQueryBodyOutcome::RequestTimeout { body }) => {
            Err(Unanswered::Status(StatusCode::REQUEST_TIMEOUT, body))
        }
        Err(source) => Err(Unanswered::Client(source)),
    }
}

/// Runs the stored query `name` at `version`.
async fn stored_version(
    group: &QueryClient<'_, ReqwestTransport>,
    name: &str,
    version: &str,
    body: &Query,
) -> Result<ResultSet, Unanswered> {
    let params = QueryExecuteStoredQueryVersionBodyParams {
        qualified_query_name: name.to_owned(),
        version: version.to_owned(),
        accept: Some(json()),
        content_type: Some(json()),
    };
    match group
        .query_execute_stored_query_version_body(&params, body)
        .await
    {
        Ok(QueryExecuteStoredQueryVersionBodyOutcome::Ok { body, .. }) => Ok(body),
        Ok(QueryExecuteStoredQueryVersionBodyOutcome::BadRequest { body }) => {
            Err(Unanswered::Status(StatusCode::BAD_REQUEST, body))
        }
        Ok(QueryExecuteStoredQueryVersionBodyOutcome::NotFound { body }) => {
            Err(Unanswered::Status(StatusCode::NOT_FOUND, body))
        }
        Ok(QueryExecuteStoredQueryVersionBodyOutcome::RequestTimeout { body }) => {
            Err(Unanswered::Status(StatusCode::REQUEST_TIMEOUT, body))
        }
        Err(source) => Err(Unanswered::Client(source)),
    }
}
