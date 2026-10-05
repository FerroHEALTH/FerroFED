// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access token of an OAuth 2.0 grant, incorporated in each request to an
//! IHE FHIR server (IUA ITI-72 §3.72.4.2).
//!
//! An [`Authentication::Grant`] holds a provider that obtains the token with
//! the client-credentials grant (IUA ITI-71 §3.71.4.1.2.1) and refreshes it
//! before it expires. [`client`] builds the HTTP client of a PIXm, PDQm,
//! PMIR or mCSD client with no default `Authorization` header and returns
//! the [`TokenAuthorizer`] the `ihe_iti` client asks for the header of each
//! request. A `401` drops the provider's token, and the request is sent once
//! more with a fresh one, never with the refused token (§3.72.4.3).

use std::sync::Arc;

use http::header::AUTHORIZATION;
use http::{HeaderMap, Method, StatusCode};
use ihe_iti::authorizer::{Authorized, Authorizer, AuthorizerError, Retry};
use openehr_its::rest::client::{Credentials, CredentialsProvider};
use url::Url;

use crate::fhir::{self, Authentication, ClientError, Tls};

/// The [`Authorizer`] that incorporates the token of a grant's provider in
/// each request as a bearer token (IUA ITI-72 §3.72.4.2).
///
/// `Debug` shows the provider by its own `Debug`, which names no token.
#[derive(Debug)]
pub struct TokenAuthorizer {
    provider: Arc<dyn CredentialsProvider>,
}

impl TokenAuthorizer {
    /// The authorizer that sends the tokens `provider` obtains.
    #[must_use]
    pub fn new(provider: Arc<dyn CredentialsProvider>) -> Self {
        Self { provider }
    }
}

/// A credential a grant's provider gave that is no bearer token, which IUA
/// ITI-72 §3.72.4.2 incorporates as the `Bearer` type alone.
#[derive(Debug, thiserror::Error)]
#[error("the grant gave a credential that is no bearer token")]
struct NotBearer;

impl Authorizer for TokenAuthorizer {
    fn authorize<'a>(&'a self, _method: &'a Method, _url: &'a Url) -> Authorized<'a> {
        Box::pin(async move {
            let credentials = self
                .provider
                .credentials()
                .await
                .map_err(AuthorizerError::new)?;
            if !matches!(credentials, Credentials::Bearer(_)) {
                return Err(AuthorizerError::new(NotBearer));
            }
            let mut value = credentials.header_value().map_err(AuthorizerError::new)?;
            value.set_sensitive(true);
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, value);
            Ok(headers)
        })
    }

    fn answered(&self, _url: &Url, status: StatusCode, _headers: &HeaderMap) -> Retry {
        // NOTE: IUA ITI-72 §3.72.4.3: a client answered 401 obtains a new token
        // before it retries, and never retries with the token that was refused.
        if status == StatusCode::UNAUTHORIZED {
            self.provider.refused();
            Retry::Resend
        } else {
            Retry::Done
        }
    }
}

/// Builds the HTTP client an IHE FHIR client is asked through, and the
/// authorizer that incorporates a grant's token in each of its requests.
///
/// A bearer token or basic credentials ride in the client's default header,
/// as [`fhir::http_client`] composes it, with no authorizer; a grant rides
/// in no default header, and its [`TokenAuthorizer`] is returned beside the
/// client.
///
/// # Errors
/// A [`ClientError`] for a credential that forms no `Authorization` value,
/// or a client that cannot be built.
pub(crate) fn client(
    auth: &Authentication,
    tls: &Tls,
) -> Result<(reqwest::Client, Option<Arc<dyn Authorizer>>), ClientError> {
    match auth {
        Authentication::Grant(provider) => {
            let http = fhir::http_client(&Authentication::None, tls)?;
            let authorizer: Arc<dyn Authorizer> =
                Arc::new(TokenAuthorizer::new(Arc::clone(provider)));
            Ok((http, Some(authorizer)))
        }
        _ => Ok((fhir::http_client(auth, tls)?, None)),
    }
}
