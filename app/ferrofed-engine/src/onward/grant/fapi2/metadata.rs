// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discovery of a FAPI 2.0 grant's authorization server from its issuer
//! (RFC 8414; FAPI 2.0 Security Profile §5.3.3.1).
//!
//! The client "shall only use authorization server metadata … retrieved
//! from the metadata document" and "shall ensure that this issuer URL and
//! the issuer value in the obtained metadata match" (FAPI 2.0 §5.3.3.1).
//! The document is read once from the issuer's well-known URL (RFC 8414
//! §3.1) over the node transport, which follows no redirect, and held to
//! what the grant will send before anything else is sent:
//!
//! - an `application/json` answer of at most [`RESPONSE_LIMIT`] bytes whose
//!   objects repeat no name, its `issuer` identical to the grant's (RFC 8414
//!   §3.2, §3.3), and a `token_endpoint` on the issuer's origin; a grant
//!   that uses mutual TLS takes the `token_endpoint` of
//!   `mtls_endpoint_aliases` in preference where one is named (RFC 8705
//!   §5), held to the same origin or to an `https` host the grant names
//!   ([`Fapi2Grant::mtls_alias_hosts`]);
//! - `token_endpoint_auth_methods_supported` naming the client
//!   authentication the grant uses (FAPI 2.0 §5.3.2.1): `private_key_jwt`,
//!   with `token_endpoint_auth_signing_alg_values_supported` present and
//!   naming `ES256` (RFC 8414 §2: the list is required beside
//!   `private_key_jwt`, and an omitted method list means
//!   `client_secret_basic` alone), or `tls_client_auth` or
//!   `self_signed_tls_client_auth` (RFC 8705 §2.1.1, §2.2.1);
//! - `tls_client_certificate_bound_access_tokens` true for a grant whose
//!   tokens are bound to its certificate (RFC 8705 §3.3: omitted, it is
//!   false);
//! - `grant_types_supported` naming every grant the grant sends:
//!   `client_credentials`, and token exchange for a grant that exchanges
//!   (RFC 8414 §2: an omitted list means `authorization_code` and
//!   `implicit` alone);
//! - `dpop_signing_alg_values_supported`, when listed, naming `ES256` (RFC
//!   9449 §5.1);
//! - `authorization_details_types_supported` present and naming every type
//!   the grant's `authorization_details` use, for a grant that has some (RFC
//!   9396 §10).
//!
//! No error of this module carries a credential or a token; the metadata
//! the server wrote is quoted only by the algorithm, grant type or detail
//! type the gateway itself asked for.

use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};
use oauth_server_metadata::EndpointError;
use openehr_its::rest::client::{RequestTimeout, Transport, TransportError};
use serde::Deserialize;
use url::Url;

use crate::onward::grant::fapi2::{ALGORITHM_NAME, Fapi2Grant};
use crate::onward::token::{GRANT_TYPE, TOKEN_EXCHANGE};
use crate::onward::{ClientAuthentication, GrantKind};

/// The longest metadata document the gateway reads (no specification
/// governs this: our own design).
pub const RESPONSE_LIMIT: usize = 64 * 1024;

/// The client authentication method of a JWT client assertion signed with
/// the client's private key (RFC 8414 §2; FAPI 2.0 Security Profile
/// §5.3.2.1).
pub const PRIVATE_KEY_JWT: &str = "private_key_jwt";

/// The media type of a metadata document (RFC 8414 §3.2).
const JSON: &str = "application/json";

/// Why the authorization server's metadata gave the grant no token endpoint
/// it may use, so nothing was sent to it.
///
/// No variant carries a credential.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiscoveryError {
    /// The metadata request could not be composed.
    #[error("the metadata request could not be composed")]
    Compose(#[source] http::Error),
    /// The metadata did not arrive before the timeout.
    #[error("the authorization server metadata did not arrive in time")]
    TimeOut(#[source] TransportError),
    /// The authorization server could not be reached.
    #[error("the authorization server could not be reached for its metadata")]
    Unreachable(#[source] TransportError),
    /// The metadata URL answered another status than `200` (RFC 8414 §3.2).
    #[error("the authorization server metadata answered {status}")]
    Status {
        /// The status it answered.
        status: StatusCode,
    },
    /// The metadata is not `application/json` (RFC 8414 §3.2).
    #[error("the authorization server metadata is not application/json (RFC 8414 §3.2)")]
    MediaType,
    /// The metadata is longer than [`RESPONSE_LIMIT`] bytes.
    #[error("the authorization server metadata is longer than {limit} bytes")]
    TooLarge {
        /// The limit.
        limit: usize,
    },
    /// The metadata is no JSON object of the RFC 8414 §2 members, or an
    /// object in it repeats a name.
    #[error("the authorization server metadata is not an RFC 8414 §2 document")]
    Body(#[source] serde_json::Error),
    /// The metadata names another issuer than the grant asked of (RFC 8414
    /// §3.3; FAPI 2.0 Security Profile §5.3.3.1).
    #[error("the metadata names another issuer (RFC 8414 §3.3)")]
    Issuer,
    /// The metadata names no `token_endpoint`, or one that is no URL (RFC
    /// 8414 §2).
    #[error("the metadata names no token_endpoint URL")]
    TokenEndpoint,
    /// The `token_endpoint` is a URL the gateway does not send its
    /// credentials to.
    #[error("the metadata's token_endpoint is not one the gateway sends to")]
    Endpoint(#[source] EndpointError),
    /// The token endpoint does not take the client authentication the grant
    /// uses: `private_key_jwt`, `tls_client_auth` or
    /// `self_signed_tls_client_auth` (RFC 8414 §2, RFC 8705 §2; FAPI 2.0
    /// Security Profile §5.3.2.1).
    #[error("the token endpoint does not take the client authentication the grant uses")]
    ClientAuthentication,
    /// The server does not state that it issues certificate-bound tokens,
    /// which a grant bound to its certificate takes alone (RFC 8705 §3.3).
    #[error(
        "the authorization server does not state tls_client_certificate_bound_access_tokens (RFC 8705 §3.3)"
    )]
    CertificateBinding,
    /// The token endpoint does not list `ES256` for client assertions (RFC
    /// 8414 §2).
    #[error("the token endpoint does not list ES256 for client assertions (RFC 8414 §2)")]
    SigningAlgorithm,
    /// The server lists `DPoP` algorithms without `ES256` (RFC 9449 §5.1).
    #[error("the metadata's dpop_signing_alg_values_supported does not list ES256")]
    ProofAlgorithm,
    /// The server does not support a grant type the grant sends (RFC 8414
    /// §2).
    #[error("the authorization server does not support the grant type {grant_type}")]
    GrantType {
        /// The grant type.
        grant_type: &'static str,
    },
    /// The server does not list an authorization details type the grant
    /// asks for (RFC 9396 §10).
    #[error("the authorization server does not list the authorization details type {kind:?}")]
    AuthorizationDetailsType {
        /// The type the grant asks for.
        kind: String,
    },
}

/// The metadata members the grant reads (RFC 8414 §2, RFC 9449 §5.1, RFC
/// 9396 §10).
#[derive(Deserialize)]
struct Raw {
    issuer: Option<String>,
    token_endpoint: Option<String>,
    grant_types_supported: Option<Vec<String>>,
    token_endpoint_auth_methods_supported: Option<Vec<String>>,
    token_endpoint_auth_signing_alg_values_supported: Option<Vec<String>>,
    dpop_signing_alg_values_supported: Option<Vec<String>>,
    authorization_details_types_supported: Option<Vec<String>>,
    tls_client_certificate_bound_access_tokens: Option<bool>,
    mtls_endpoint_aliases: Option<Aliases>,
}

/// The endpoint aliases a client that uses mutual TLS sends to (RFC 8705
/// §5), the one the grant reads.
#[derive(Deserialize)]
struct Aliases {
    token_endpoint: Option<String>,
}

/// Reads the metadata of `grant`'s issuer over `transport`, waiting at most
/// `timeout`, and returns the token endpoint it names once the metadata
/// admits everything the grant sends.
///
/// # Errors
///
/// Returns a [`DiscoveryError`] for a request that could not be sent or was
/// not answered with a metadata document, and for a document that does not
/// admit the grant.
pub(crate) async fn discover<T: Transport>(
    grant: &Fapi2Grant,
    transport: &T,
    timeout: Duration,
) -> Result<Url, DiscoveryError> {
    let mut request = http::Request::builder()
        .method(Method::GET)
        .uri(grant.issuer().metadata_url().as_str())
        .header(ACCEPT, HeaderValue::from_static(JSON))
        .body(Vec::new())
        .map_err(DiscoveryError::Compose)?;
    request.extensions_mut().insert(RequestTimeout(timeout));
    let answer = transport.send(request).await.map_err(|error| match error {
        TransportError::Timeout { .. } => DiscoveryError::TimeOut(error),
        TransportError::Send { .. } => DiscoveryError::Unreachable(error),
    })?;
    if answer.status() != StatusCode::OK {
        return Err(DiscoveryError::Status {
            status: answer.status(),
        });
    }
    let json = answer
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case(JSON));
    if !json {
        return Err(DiscoveryError::MediaType);
    }
    if answer.body().len() > RESPONSE_LIMIT {
        return Err(DiscoveryError::TooLarge {
            limit: RESPONSE_LIMIT,
        });
    }
    read(answer.body(), grant)
}

/// Reads `body`, the metadata of `grant`'s issuer, as [`discover`] holds it.
fn read(body: &[u8], grant: &Fapi2Grant) -> Result<Url, DiscoveryError> {
    oauth_server_metadata::repeats_no_name(body).map_err(DiscoveryError::Body)?;
    let raw: Raw = serde_json::from_slice(body).map_err(DiscoveryError::Body)?;
    let issuer = grant.issuer();
    if !raw
        .issuer
        .as_deref()
        .is_some_and(|named| issuer.is_identical_to(named))
    {
        return Err(DiscoveryError::Issuer);
    }
    // NOTE: RFC 8705 §5, a client that does mutual TLS MUST use the alias of an
    // endpoint in mtls_endpoint_aliases, when present, over the top-level one.
    let alias = raw
        .mtls_endpoint_aliases
        .as_ref()
        .and_then(|aliases| aliases.token_endpoint.as_deref())
        .filter(|_| grant.uses_mutual_tls());
    let token_endpoint = match alias {
        Some(alias) => issuer.mtls_alias(alias, grant.mtls_alias_hosts()),
        None => issuer.endpoint(
            raw.token_endpoint
                .as_deref()
                .ok_or(DiscoveryError::TokenEndpoint)?,
        ),
    }
    .map_err(|refused| match refused {
        EndpointError::NotAUrl => DiscoveryError::TokenEndpoint,
        other => DiscoveryError::Endpoint(other),
    })?;
    let lists = |list: Option<&Vec<String>>, value: &str| {
        list.is_some_and(|list| list.iter().any(|item| item == value))
    };
    let method = grant.client_authentication();
    if !lists(
        raw.token_endpoint_auth_methods_supported.as_ref(),
        method.as_str(),
    ) {
        return Err(DiscoveryError::ClientAuthentication);
    }
    if method == ClientAuthentication::PrivateKeyJwt
        && !lists(
            raw.token_endpoint_auth_signing_alg_values_supported
                .as_ref(),
            ALGORITHM_NAME,
        )
    {
        return Err(DiscoveryError::SigningAlgorithm);
    }
    if grant.certificate().is_some() && raw.tls_client_certificate_bound_access_tokens != Some(true)
    {
        return Err(DiscoveryError::CertificateBinding);
    }
    let exchanges = grant.kind() == GrantKind::TokenExchange;
    for grant_type in [Some(GRANT_TYPE), exchanges.then_some(TOKEN_EXCHANGE)]
        .into_iter()
        .flatten()
    {
        if !lists(raw.grant_types_supported.as_ref(), grant_type) {
            return Err(DiscoveryError::GrantType { grant_type });
        }
    }
    if grant.dpop().is_some()
        && raw
            .dpop_signing_alg_values_supported
            .as_ref()
            .is_some_and(|supported| !supported.iter().any(|alg| alg == ALGORITHM_NAME))
    {
        return Err(DiscoveryError::ProofAlgorithm);
    }
    if let Some(details) = grant.authorization_details()
        && let Some(kind) = details.types().iter().find(|kind| {
            !lists(
                raw.authorization_details_types_supported.as_ref(),
                kind.as_str(),
            )
        })
    {
        return Err(DiscoveryError::AuthorizationDetailsType { kind: kind.clone() });
    }
    Ok(token_endpoint)
}
