// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP calls client authentication makes: a key set fetch and an RFC
//! 7662 introspection call, each bounded in time and in size.
//!
//! A call that does not answer, answers with anything but `200`, or answers
//! more than [`MAX_BODY`] bytes is a [`FetchError`], which the gate answers
//! `503`: the gateway cannot tell whether the caller is who it claims, so it
//! admits no one (§13.1, N25). No specification governs the bounds: our own
//! design.

use std::time::Duration;

use http::StatusCode;
use tokio::sync::OnceCell;
use url::Url;

use crate::config::auth::Introspection;

/// The largest key set or introspection answer the gateway reads.
pub const MAX_BODY: usize = 256 * 1024;

/// Why a key set or an introspection answer could not be had.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    /// The HTTP client could not be built.
    #[error("the HTTP client of client authentication could not be built")]
    Client(#[source] reqwest::Error),
    /// The call did not complete: no connection, or no answer in time.
    #[error("the call did not complete")]
    Call(#[source] reqwest::Error),
    /// The call was answered with a status other than `200`.
    #[error("the call was answered {0}")]
    Status(StatusCode),
    /// The answer is larger than [`MAX_BODY`].
    #[error("the answer is larger than {MAX_BODY} bytes")]
    TooLarge,
    /// The key set file could not be read.
    #[error("the key set file could not be read")]
    File(#[source] std::io::Error),
    /// The last attempt failed less than the refetch interval ago, so none
    /// was made.
    #[error("the last attempt failed less than the refetch interval ago")]
    Backoff,
    /// The answer is not the JSON document it should be.
    #[error("the answer is not the document it should be")]
    Malformed(#[source] serde_json::Error),
}

/// The HTTP client of client authentication, built at its first use.
#[derive(Debug)]
pub(super) struct Fetcher {
    /// The client, once built.
    client: OnceCell<reqwest::Client>,
    /// How long one call may take.
    timeout: Duration,
}

impl Fetcher {
    /// Returns a fetcher whose calls each take at most `timeout`.
    pub(super) fn new(timeout: Duration) -> Self {
        Self {
            client: OnceCell::new(),
            timeout,
        }
    }

    /// Fetches the document at `url` with `GET`.
    pub(super) async fn get(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
        let response = self
            .client()
            .await?
            .get(url.clone())
            .header(http::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(FetchError::Call)?;
        read(response).await
    }

    /// Asks `endpoint` about `token` (RFC 7662 §2.1), authenticating with the
    /// gateway's client credentials as RFC 6749 §2.3.1 encodes them.
    pub(super) async fn introspect(
        &self,
        endpoint: &Introspection,
        token: &str,
    ) -> Result<Vec<u8>, FetchError> {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", token)
            .append_pair("token_type_hint", "access_token")
            .finish();
        let encoded =
            |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
        let response = self
            .client()
            .await?
            .post(endpoint.endpoint.clone())
            .basic_auth(
                encoded(&endpoint.client_id),
                Some(encoded(endpoint.client_secret.expose())),
            )
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
            .body(form)
            .send()
            .await
            .map_err(FetchError::Call)?;
        read(response).await
    }

    /// The client, built on first use; a build that fails is tried again on
    /// the next call.
    async fn client(&self) -> Result<&reqwest::Client, FetchError> {
        self.client
            .get_or_try_init(|| async {
                // NOTE: no specification governs this: our own design; a key set
                // or an introspection answer is read where it was asked, never
                // from where a redirect points.
                reqwest::Client::builder()
                    .timeout(self.timeout)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(FetchError::Client)
            })
            .await
    }
}

/// Reads a `200` answer's body, at most [`MAX_BODY`] bytes of it.
async fn read(mut response: reqwest::Response) -> Result<Vec<u8>, FetchError> {
    let status = response.status();
    if status != StatusCode::OK {
        return Err(FetchError::Status(status));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(FetchError::Call)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
