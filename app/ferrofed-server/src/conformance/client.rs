// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway as a conformance run reaches it: a plain ITS-REST client
//! given the base URL and a caller's bearer token, and nothing else (§16.3
//! track 9, N28).
//!
//! [`Gateway`] is the seam the scenarios are written against. A run sends
//! through [`HttpGateway`]; the end-to-end suite sends through the router
//! itself and validates every answer against the vendored JSON Schemas
//! through [`Gateway::check_result_set`] and [`Gateway::check_options`]. What
//! the run reads of an answer is typed here: the columns, the rows as text,
//! and `meta.federation` (§9, N17).

use std::fmt;
use std::fmt::Write as _;
use std::future::Future;
use std::time::{Duration, Instant};

use http::{HeaderMap, Method, Request, StatusCode, header};
use openehr_its::rest::generated::query::AdhocQueryExecute;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::IgnoredAny;
use url::Url;

use crate::conformance::{Failure, ensure};

/// The largest answer a run reads, in bytes; a scenario's answers are a few
/// rows over the synthetic fixture.
const ANSWER_LIMIT: usize = 4 * 1024 * 1024;

/// A request to the gateway that got no answer to read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GatewayError {
    /// The HTTP client could not be built.
    #[error("the HTTP client could not be built")]
    Client(#[source] reqwest::Error),
    /// The request could not be sent, or its answer not read.
    #[error("the request could not be sent, or its answer not read")]
    Send(#[source] reqwest::Error),
    /// The answer is larger than a run reads.
    #[error("the answer is larger than {ANSWER_LIMIT} bytes")]
    TooLarge,
    /// The answer is not UTF-8 text.
    #[error("the answer is not UTF-8 text")]
    Text(#[source] std::string::FromUtf8Error),
    /// The caller's token is not a header value.
    #[error("the caller's token is not a valid Authorization value")]
    Token(#[source] header::InvalidHeaderValue),
    /// Another transport failed: the end-to-end suite's in-process router.
    #[error("the request could not be sent")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// The gateway a scenario asks.
///
/// [`Gateway::send`] carries the caller's credential, and
/// [`Gateway::send_anonymous`] carries none, for the scenario that holds the
/// gateway to authenticating its callers (§13.1, CP-17).
pub trait Gateway: Sync {
    /// Sends `request`, its path relative to the gateway's base, with the
    /// caller's credential, and reads the whole answer.
    fn send(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Reply, GatewayError>> + Send;

    /// Sends `request` as [`Gateway::send`] does, with no credential.
    fn send_anonymous(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Reply, GatewayError>> + Send;

    /// Checks a federated `RESULT_SET` body further than the typed read; a
    /// run checks nothing more, and the end-to-end suite validates it against
    /// `federated-result-set.schema.json`.
    ///
    /// # Errors
    ///
    /// Returns why the body does not hold.
    fn check_result_set(&self, _text: &str) -> Result<(), String> {
        Ok(())
    }

    /// Checks an `OPTIONS {base}/` body further than the typed read; a run
    /// checks nothing more, and the end-to-end suite validates it against
    /// `options-root.schema.json`.
    ///
    /// # Errors
    ///
    /// Returns why the body does not hold.
    fn check_options(&self, _text: &str) -> Result<(), String> {
        Ok(())
    }
}

/// The gateway at a base URL, over HTTP, with a caller's bearer token.
///
/// `Debug` never shows the token.
pub struct HttpGateway {
    base: Url,
    client: reqwest::Client,
    token: SecretString,
}

impl HttpGateway {
    /// Creates the client of the gateway at `base`, sending `token` as the
    /// caller's bearer credential (RFC 6750 §2.1).
    ///
    /// The client follows no redirect: a `3xx` is the answer read, so the
    /// token never travels to an origin the run was not given.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::Client`] when the HTTP client cannot be built.
    pub fn new(base: Url, token: SecretString) -> Result<Self, GatewayError> {
        // NOTE: RFC 9110 §15.4 lets a redirect name any origin, and a followed one
        // could carry the caller's token there.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(GatewayError::Client)?;
        Ok(Self {
            base,
            client,
            token,
        })
    }

    /// Returns the base URL the gateway is reached at.
    #[must_use]
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Sends `request` under the base, with the token when `authenticated`.
    async fn exchange(
        &self,
        request: Request<Vec<u8>>,
        authenticated: bool,
    ) -> Result<Reply, GatewayError> {
        let (parts, body) = request.into_parts();
        let relative = parts
            .uri
            .path_and_query()
            .map_or("/", http::uri::PathAndQuery::as_str);
        let url = format!("{}{relative}", self.base.as_str().trim_end_matches('/'));
        let mut headers = parts.headers;
        if authenticated && !headers.contains_key(header::AUTHORIZATION) {
            let bearer = format!("Bearer {}", self.token.expose_secret());
            let mut value = header::HeaderValue::from_str(&bearer).map_err(GatewayError::Token)?;
            value.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, value);
        }
        let started = Instant::now();
        let response = self
            .client
            .request(parts.method, url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(GatewayError::Send)?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(GatewayError::Send)?;
        if bytes.len() > ANSWER_LIMIT {
            return Err(GatewayError::TooLarge);
        }
        Ok(Reply {
            status,
            headers,
            text: String::from_utf8(bytes.to_vec()).map_err(GatewayError::Text)?,
            took: started.elapsed(),
        })
    }
}

impl fmt::Debug for HttpGateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpGateway")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

impl Gateway for HttpGateway {
    fn send(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Reply, GatewayError>> + Send {
        self.exchange(request, true)
    }

    fn send_anonymous(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Reply, GatewayError>> + Send {
        self.exchange(request, false)
    }
}

/// What the gateway answered, and how long it took.
///
/// `Debug` shows the status and the size, never the body.
pub struct Reply {
    /// The status.
    pub status: StatusCode,
    /// The header fields.
    pub headers: HeaderMap,
    /// The body, as text.
    pub text: String,
    /// The time from sending the request to reading the whole answer.
    pub took: Duration,
}

impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply")
            .field("status", &self.status)
            .field("body_bytes", &self.text.len())
            .finish_non_exhaustive()
    }
}

impl Reply {
    /// Returns the text of header field `name`, when present.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// Returns the answer read as a federated `RESULT_SET` whose cells are
    /// text (§9, N17).
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Read`] when the body is not one.
    pub fn federated(&self) -> Result<Federated, Failure> {
        serde_json::from_str(&self.text).map_err(|source| Failure::Read {
            what: "the federated RESULT_SET".to_owned(),
            source,
        })
    }

    /// Returns the stable code of an ITS-REST `Error` answer.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Read`] when the body carries no string `code`.
    pub fn code(&self) -> Result<String, Failure> {
        #[derive(Deserialize)]
        struct Coded {
            code: String,
        }
        serde_json::from_str::<Coded>(&self.text)
            .map(|coded| coded.code)
            .map_err(|source| Failure::Read {
                what: format!("the {} error body", self.status),
                source,
            })
    }

    /// Returns `Ok` when the status is `expected`, and a [`Failure::Check`]
    /// naming `what` and the body otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Check`] for any other status.
    pub fn expect(&self, expected: StatusCode, what: &str) -> Result<(), Failure> {
        ensure(self.status == expected, || {
            format!(
                "{what}: expected {expected}, got {}: {}",
                self.status,
                excerpt(&self.text)
            )
        })
    }
}

/// Returns `text` percent-encoded byte for byte, the unreserved set of RFC
/// 3986 §2.3 kept, as a query-string value.
#[must_use]
pub fn percent_encoded(text: &str) -> String {
    text.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _escaped = write!(out, "%{byte:02X}");
        }
        out
    })
}

/// At most the first 400 characters of `text`, for a failure message.
#[must_use]
pub fn excerpt(text: &str) -> String {
    let mut excerpt: String = text.chars().take(400).collect();
    if excerpt.len() < text.len() {
        excerpt.push_str("...");
    }
    excerpt
}

/// A federated `RESULT_SET` whose cells are all text.
#[derive(Debug, Deserialize)]
pub struct Federated {
    /// The ITS-REST `name` member, set on a stored query's answer.
    pub name: Option<String>,
    /// The columns.
    pub columns: Vec<Column>,
    /// The rows, each an ordered array.
    pub rows: Vec<Vec<String>>,
    /// The `meta` member.
    pub meta: Meta,
}

/// One column of a result set.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Column {
    /// The column name.
    pub name: String,
    /// The AQL path, when the column has one.
    pub path: Option<String>,
}

/// The `meta` member, with the flat members a pre-0.9.0 envelope carried
/// read so a scenario can hold that they are absent (CP-35).
#[derive(Debug, Deserialize)]
pub struct Meta {
    /// `meta.federation`.
    pub federation: FederationMeta,
    /// A flat `meta.complete`, which a conformant envelope never carries.
    pub complete: Option<IgnoredAny>,
    /// A flat `meta.endpoints`, which a conformant envelope never carries.
    pub endpoints: Option<IgnoredAny>,
}

/// `meta.federation`.
#[derive(Debug, Deserialize)]
pub struct FederationMeta {
    /// Whether every in-scope node answered.
    pub complete: bool,
    /// One record per member endpoint.
    pub endpoints: Vec<EndpointRecord>,
}

/// One `meta.federation.endpoints[]` record.
#[derive(Debug, Deserialize)]
pub struct EndpointRecord {
    /// The endpoint id.
    pub id: String,
    /// The §11.1 status.
    pub status: String,
    /// The rows the endpoint contributed.
    pub row_count: Option<u64>,
    /// The endpoint's latency on this request.
    pub latency_ms: Option<u64>,
    /// The error an endpoint that failed carries.
    pub error: Option<IgnoredAny>,
}

impl Federated {
    /// Returns each endpoint's id and status, in the order reported.
    #[must_use]
    pub fn statuses(&self) -> Vec<(&str, &str)> {
        self.meta
            .federation
            .endpoints
            .iter()
            .map(|record| (record.id.as_str(), record.status.as_str()))
            .collect()
    }

    /// Returns the status reported for `endpoint`, when it is reported.
    #[must_use]
    pub fn status_of(&self, endpoint: &str) -> Option<&str> {
        self.meta
            .federation
            .endpoints
            .iter()
            .find(|record| record.id == endpoint)
            .map(|record| record.status.as_str())
    }

    /// Returns the record of `endpoint`.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Check`] when the answer does not report it.
    pub fn endpoint(&self, endpoint: &str) -> Result<&EndpointRecord, Failure> {
        self.meta
            .federation
            .endpoints
            .iter()
            .find(|record| record.id == endpoint)
            .ok_or_else(|| Failure::Check(format!("{endpoint} is not reported")))
    }

    /// Returns the column names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect()
    }

    /// Returns the rows, sorted.
    #[must_use]
    pub fn sorted_rows(&self) -> Vec<Vec<String>> {
        let mut rows = self.rows.clone();
        rows.sort();
        rows
    }
}

/// A result set whose cells are counts.
#[derive(Debug, Deserialize)]
pub struct Counted {
    /// The rows, each an ordered array of counts.
    pub rows: Vec<Vec<u64>>,
}

/// Returns `POST /v1/query/aql` with `aql` and the header fields `fields`,
/// as an ITS-REST `AdhocQueryExecute`.
///
/// # Errors
///
/// Returns [`Failure::Check`] when a header field is not one.
pub fn post_aql(aql: &str, fields: &[(&str, &str)]) -> Result<Request<Vec<u8>>, Failure> {
    let body = AdhocQueryExecute {
        q: aql.to_owned(),
        offset: None,
        fetch: None,
        query_parameters: None,
        additional_properties: std::collections::BTreeMap::new(),
    };
    let body = serde_json::to_vec(&body).map_err(|source| Failure::Read {
        what: "the query body".to_owned(),
        source,
    })?;
    request(
        Method::POST,
        "/v1/query/aql",
        fields,
        Some(("application/json", body)),
    )
}

/// Returns a request of `method` to `path` under the base, with the header
/// fields `fields` and, when given, a body of its content type.
///
/// # Errors
///
/// Returns [`Failure::Check`] when the path or a header field is not one.
pub fn request(
    method: Method,
    path: &str,
    fields: &[(&str, &str)],
    body: Option<(&str, Vec<u8>)>,
) -> Result<Request<Vec<u8>>, Failure> {
    let mut builder = Request::builder().method(method).uri(path);
    for (name, value) in fields {
        builder = builder.header(*name, *value);
    }
    let bytes = match body {
        Some((content_type, bytes)) => {
            builder = builder.header(header::CONTENT_TYPE, content_type);
            bytes
        }
        None => Vec::new(),
    };
    builder.body(bytes).map_err(|error| {
        Failure::Check(format!(
            "the request to {path} could not be formed: {error}"
        ))
    })
}

/// Returns `GET path` under the base, with the header fields `fields`.
///
/// # Errors
///
/// Returns [`Failure::Check`] when the path or a header field is not one.
pub fn get(path: &str, fields: &[(&str, &str)]) -> Result<Request<Vec<u8>>, Failure> {
    request(Method::GET, path, fields, None)
}

/// Sends `request` through `gateway`, naming the request in a failure.
///
/// # Errors
///
/// Returns [`Failure::Gateway`] when the gateway gave no answer to read.
pub async fn ask<G: Gateway>(gateway: &G, request: Request<Vec<u8>>) -> Result<Reply, Failure> {
    let step = format!("{} {}", request.method(), request.uri().path());
    gateway
        .send(request)
        .await
        .map_err(|source| Failure::Gateway { step, source })
}

/// Sends `request` through `gateway` with no credential.
///
/// # Errors
///
/// Returns [`Failure::Gateway`] when the gateway gave no answer to read.
pub async fn ask_anonymous<G: Gateway>(
    gateway: &G,
    request: Request<Vec<u8>>,
) -> Result<Reply, Failure> {
    let step = format!("{} {}", request.method(), request.uri().path());
    gateway
        .send_anonymous(request)
        .await
        .map_err(|source| Failure::Gateway { step, source })
}

/// Sends `request` and returns the `200` answer read as a federated
/// `RESULT_SET`, held to [`Gateway::check_result_set`].
///
/// # Errors
///
/// Returns [`Failure`] when the gateway gave no `200` or no result set.
pub async fn answered<G: Gateway>(
    gateway: &G,
    request: Request<Vec<u8>>,
    what: &str,
) -> Result<(Reply, Federated), Failure> {
    let reply = ask(gateway, request).await?;
    reply.expect(StatusCode::OK, what)?;
    let federated = checked(gateway, &reply)?;
    Ok((reply, federated))
}

/// Returns `reply` read as a federated `RESULT_SET`, held to
/// [`Gateway::check_result_set`] (§9, N17, CP-35).
///
/// # Errors
///
/// Returns [`Failure`] when the body is not a result set or does not hold.
pub fn checked<G: Gateway>(gateway: &G, reply: &Reply) -> Result<Federated, Failure> {
    gateway
        .check_result_set(&reply.text)
        .map_err(|why| Failure::Check(format!("the RESULT_SET does not hold: {why}")))?;
    reply.federated()
}
