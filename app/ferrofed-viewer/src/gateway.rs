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
//! answer.

use std::fmt;

use ferrofed_registry::health::DependencyReport;
use ferrofed_registry::operator::{CreatingSystemEntry, IncidentReport, Page, PageRequest};
use http::{Method, StatusCode};
use openehr_federation::options::OptionsRoot;
use openehr_its::rest::client::{
    Client, ClientError, Credentials, ErrorBody, Request, ReqwestTransport,
};
use openehr_its::rest::generated::definition::StoredQuery;
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
            Self::Transport { .. } | Self::Base { .. } | Self::Call { .. } => return None,
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
            } => Some(*status),
            _ => None,
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
    /// `GET {base}/health/dependencies`.
    ///
    /// # Errors
    /// As [`Gateway::self_description`].
    pub async fn dependencies(
        &self,
        token: &AccessToken,
    ) -> Result<DependencyReport, GatewayError> {
        self.read(Method::GET, "/health/dependencies", token).await
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
