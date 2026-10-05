// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's client of the FerroFED gateway: the gateway's public surface
//! over HTTP, and nothing else.
//!
//! Every call goes through the `openehr-its` client, sent as the signed-in
//! operator with the operator's own access token, so the gateway
//! authenticates the console's requests exactly as it does any other
//! client's. [`Gateway::its`] is the ITS-REST surface at `{base}/v1`, for the
//! generated group clients of `openehr_its::rest::generated`, and
//! [`Gateway::self_description`] reads `OPTIONS {base}/` into the typed
//! federation body (§7a.2, N30). A refusal or a failure is a typed
//! [`GatewayError`] carrying the gateway's status, never an empty answer.

use std::fmt;

use http::{Method, StatusCode};
use openehr_federation::options::OptionsRoot;
use openehr_its::rest::client::{
    Client, ClientError, Credentials, ErrorBody, Request, ReqwestTransport,
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
        let client = self.client(self.base.clone(), token)?;
        let answer = client
            .execute(Request::new(Method::OPTIONS, String::from("/")))
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
