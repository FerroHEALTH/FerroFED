// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Single-node forwarding: one client request passed to one node once, and
//! the node's answer returned as the node sent it (§7a.1, §7a.3, N22, N31).
//!
//! The request leaves through `openehr-its`'s `Client::forward`, which sends
//! it once and classifies nothing. The method, the path and the body bytes are
//! the client's, unchanged: a commit body is clinical content the gateway has
//! no right to alter (§5.4 scope note, N33). Of the client's headers and query
//! string, only what the outbound gate admits travels
//! ([`hygiene::forwarded_headers`], [`hygiene::forwarded_query`]); the
//! endpoint's onward credentials set `Authorization`.
//!
//! The answer keeps its status, its body bytes, `Location` and `ETag`; only
//! the hop-by-hop fields are removed (RFC 9110 §7.6.1). A node's `404` or
//! `500` is a [`Forwarded`] answer like any other (§11.2). A `401` is the node
//! refusing the gateway's own onward credentials, so it is
//! [`ForwardError::Refused`], carrying the node's status and body, and never
//! a challenge to the client's credentials.

use std::fmt;

use crate::dispatch::{DispatchOptions, NodeClient, REQUEST_ID_HEADER};
use crate::hygiene::{self, Outbound, Part, UnlistedParameter};
use ferrofed_registry::id::EndpointId;
use http::header::{CONNECTION, CONTENT_LENGTH, TE, TRAILER, TRANSFER_ENCODING, UPGRADE};
use http::{HeaderMap, HeaderName, Method, StatusCode};
use openehr_its::rest::client::{ClientError, ErrorBody, Request, Transport, TransportError};

/// The hop-by-hop fields RFC 9110 §7.6.1 names besides `Connection` itself,
/// which an intermediary never forwards.
const HOP_BY_HOP: [&str; 2] = ["keep-alive", "proxy-connection"];

/// A client request to forward to one node, as the client sent it.
///
/// `Debug` shows the method and the sizes, never the path, a header or the
/// body.
pub struct ClientRequest {
    /// The request method.
    pub method: Method,
    /// The path relative to the ITS-REST base, as received and still
    /// percent-encoded (`/ehr/7d44…/composition`).
    pub path: String,
    /// The query string as received, without its `?`.
    pub query: Option<String>,
    /// Every header the client sent.
    pub headers: HeaderMap,
    /// The body bytes the client sent.
    pub body: Vec<u8>,
}

impl fmt::Debug for ClientRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientRequest")
            .field("method", &self.method)
            .field("headers", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}

/// What the node answered a forwarded request, as it answered it, less the
/// hop-by-hop fields.
///
/// `Debug` shows the status and the sizes, never a header value or the body.
pub struct Forwarded {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Forwarded {
    /// The node's status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The node's headers, `Location` and `ETag` untouched.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The node's body bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The status, the headers and the body.
    #[must_use]
    pub fn into_parts(self) -> (StatusCode, HeaderMap, Vec<u8>) {
        (self.status, self.headers, self.body)
    }
}

impl fmt::Debug for Forwarded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Forwarded")
            .field("status", &self.status)
            .field("headers", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// A forwarded request that has no answer of the node's to pass on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ForwardError {
    /// The client's query string carries a parameter the route does not
    /// forward, so nothing was sent (§5.4.1, N33).
    #[error(transparent)]
    QueryParameter(#[from] UnlistedParameter),
    /// The outbound gate found a withheld patient identifier in the request,
    /// so nothing was sent (§5.4.1, N33).
    #[error(
        "the request to endpoint {endpoint} would carry a patient identifier in {part}, so it was not sent"
    )]
    Withheld {
        /// The endpoint.
        endpoint: EndpointId,
        /// The part of the request that carried it; never the value.
        part: Part,
    },
    /// The credentials provider produced no credential for the onward grant.
    #[error("no credential could be obtained for endpoint {endpoint}")]
    Credentials {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// The request could not be composed from its parts.
    #[error("the request to endpoint {endpoint} could not be composed")]
    Compose {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// The node did not answer before the deadline (§11.2).
    #[error("endpoint {endpoint} did not answer before the deadline")]
    TimeOut {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// The node could not be reached: a refused connection or a broken
    /// stream (§11.2).
    #[error("endpoint {endpoint} could not be reached")]
    Unreachable {
        /// The endpoint.
        endpoint: EndpointId,
        /// What the client runtime reported.
        #[source]
        source: Box<ClientError>,
    },
    /// The node refused the gateway's onward credentials.
    #[error("endpoint {endpoint} refused the gateway's onward credentials with {status}")]
    Refused {
        /// The endpoint.
        endpoint: EndpointId,
        /// The node's status.
        status: StatusCode,
        /// The node's body, as received.
        body: ErrorBody,
    },
}

impl<T: Transport> NodeClient<T> {
    /// Forwards `request` to the node once and returns its answer.
    ///
    /// The deadline and the request id of `options` apply, and the outbound
    /// gate reads the URL and every header that is sent against the
    /// identifiers `options` withholds.
    ///
    /// # Errors
    ///
    /// Returns [`ForwardError::QueryParameter`] and [`ForwardError::Withheld`]
    /// with nothing sent, [`ForwardError::Credentials`] and
    /// [`ForwardError::Compose`] when the request could not leave,
    /// [`ForwardError::TimeOut`] and [`ForwardError::Unreachable`] when the
    /// node gave no answer, and [`ForwardError::Refused`] when it answered
    /// `401`.
    pub async fn forward(
        &self,
        request: ClientRequest,
        options: &DispatchOptions,
    ) -> Result<Forwarded, ForwardError> {
        let ClientRequest {
            method,
            path,
            query,
            headers,
            body,
        } = request;
        let mut outgoing = Request::new(method, path);
        if let Some(query) = query.as_deref() {
            outgoing.raw_query(hygiene::forwarded_query(query)?);
        }
        outgoing
            .headers_mut()
            .extend(hygiene::forwarded_headers(&headers));
        let call = options
            .call_options()
            .map_err(|source| ForwardError::Compose {
                endpoint: self.endpoint().clone(),
                source: Box::new(source),
            })?;
        outgoing.apply_options(&call);
        if !body.is_empty() {
            outgoing.raw_body(body, None);
        }
        self.gate_forward(&outgoing, options)?;
        match self.client().forward(outgoing).await {
            Ok(answer) if answer.status() == StatusCode::UNAUTHORIZED => {
                Err(ForwardError::Refused {
                    endpoint: self.endpoint().clone(),
                    status: answer.status(),
                    body: answer.error_body(),
                })
            }
            Ok(answer) => {
                let status = answer.status();
                let mut headers = answer.headers().clone();
                strip_hop_by_hop(&mut headers);
                Ok(Forwarded {
                    status,
                    headers,
                    body: answer.into_body(),
                })
            }
            Err(error) => Err(self.unanswered(error)),
        }
    }

    /// The outbound gate over a forwarded request: its URL and every header
    /// the gateway sends (§5.4.1, N33).
    fn gate_forward(
        &self,
        request: &Request,
        options: &DispatchOptions,
    ) -> Result<(), ForwardError> {
        let withheld = options.withheld();
        if withheld.is_empty() {
            return Ok(());
        }
        let mut url = format!(
            "{}{}",
            self.base().as_str().trim_end_matches('/'),
            request.path()
        );
        if !request.query_string().is_empty() {
            url.push('?');
            url.push_str(request.query_string());
        }
        let mut sent: Vec<(&'static str, String)> = Vec::new();
        for name in hygiene::FORWARDED_HEADERS
            .into_iter()
            .chain([REQUEST_ID_HEADER])
        {
            for value in request.headers().get_all(name) {
                sent.push((name, String::from_utf8_lossy(value.as_bytes()).into_owned()));
            }
        }
        let headers: Vec<(&'static str, &str)> = sent
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let outbound = Outbound {
            aql: "",
            scope: None,
            paging: &[],
            url: &url,
            headers: &headers,
        };
        match withheld.found_in(&outbound) {
            Some(part) => Err(ForwardError::Withheld {
                endpoint: self.endpoint().clone(),
                part,
            }),
            None => Ok(()),
        }
    }

    /// The error for a forwarded request that reached no answer.
    fn unanswered(&self, error: ClientError) -> ForwardError {
        let endpoint = self.endpoint().clone();
        match error {
            ClientError::DeadlineElapsed { .. }
            | ClientError::Transport {
                source: TransportError::Timeout { .. },
                ..
            } => ForwardError::TimeOut {
                endpoint,
                source: Box::new(error),
            },
            ClientError::Transport { .. } => ForwardError::Unreachable {
                endpoint,
                source: Box::new(error),
            },
            ClientError::Credentials { .. } => ForwardError::Credentials {
                endpoint,
                source: Box::new(error),
            },
            other => ForwardError::Compose {
                endpoint,
                source: Box::new(other),
            },
        }
    }
}

/// Removes the hop-by-hop fields of `headers`: `Connection`, every field it
/// names, and the fields RFC 9110 §7.6.1 lists, plus `Content-Length`, which
/// the gateway's own framing sets for the bytes it sends.
fn strip_hop_by_hop(headers: &mut HeaderMap) {
    let named: Vec<HeaderName> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|option| HeaderName::from_bytes(option.trim().as_bytes()).ok())
        .collect();
    for name in named {
        headers.remove(name);
    }
    for name in [
        CONNECTION,
        TE,
        TRAILER,
        TRANSFER_ENCODING,
        UPGRADE,
        CONTENT_LENGTH,
    ] {
        headers.remove(name);
    }
    for name in HOP_BY_HOP {
        headers.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::strip_hop_by_hop;
    use http::HeaderMap;

    #[test]
    fn the_hop_by_hop_fields_go_and_location_and_etag_stay() {
        let mut headers = HeaderMap::new();
        headers.insert("connection", "keep-alive, x-node-hop".parse().unwrap());
        headers.insert("keep-alive", "timeout=5".parse().unwrap());
        headers.insert("x-node-hop", "1".parse().unwrap());
        headers.insert("transfer-encoding", "chunked".parse().unwrap());
        headers.insert("content-length", "12".parse().unwrap());
        headers.insert(
            "location",
            "https://cdr-a.example.org/v1/ehr/e/composition/u::cdr-a.example.org::1"
                .parse()
                .unwrap(),
        );
        headers.insert("etag", "\"u::cdr-a.example.org::1\"".parse().unwrap());
        strip_hop_by_hop(&mut headers);
        let left: Vec<&str> = headers.keys().map(http::HeaderName::as_str).collect();
        assert_eq!(vec!["location", "etag"], left);
    }
}
