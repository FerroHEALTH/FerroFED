// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Location` of every answer that names a stored query: the URL of the
//! Stored Query resource, `{base}/v1/definition/query/{name}/{version}`
//! (ITS-REST 1.1.0 Definition API, `200_StoredQuery_stored`,
//! `Location_Query`), whichever route answered.
//!
//! With `gateway.public_url` set it is that absolute URL. Unset, the gateway
//! does not know the scheme and host its clients use (§4.1, N28), so it is a
//! reference relative to the request URL, which resolves to the stored
//! query's URL on the client listener only (RFC 3986 §5.2); the admin
//! listener has no such reference and names no `Location`.

use ferrofed_registry::definition::StoredDefinition;
use http::HeaderValue;

use crate::config::public_url::{self, PublicUrl};
use crate::error::Code;
use crate::facade::stored::Refused;

/// The route whose request URL a relative `Location` resolves against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answered {
    /// `PUT {base}/v1/definition/query/{name}/{version}`.
    Versioned,
    /// `PUT {base}/v1/definition/query/{name}`.
    Unversioned,
    /// The admin listener's distribution of a held version, on another
    /// origin than the client listener.
    Admin,
}

/// The `Location` naming `stored`, from the request `answered` names, or
/// `None` where no reference from that request names the stored query.
///
/// With `public`, the absolute URL under it. Without, its version after a
/// path that ends in one, `./{name}/{version}` after a path that ends in the
/// name, the `./` keeping the `::` of a namespaced name from reading as a URI
/// scheme (RFC 3986 §4.2), and nothing on the admin listener.
///
/// # Errors
///
/// A `500` when the URL does not parse or is no valid header value, which a
/// name and version the registry admitted never make.
pub(super) fn location(
    stored: &StoredDefinition,
    answered: Answered,
    public: Option<&PublicUrl>,
) -> Result<Option<HeaderValue>, Refused> {
    let reference = match (public, answered) {
        (Some(public), _) => {
            let route = format!(
                "{}definition/query/{}/{}",
                crate::ITS_REST_PREFIX,
                stored.name(),
                stored.version()
            );
            public
                .route(public_url::KEY, &route)
                .map_err(|failure| {
                    tracing::error!(
                        error = %crate::chain(&failure),
                        "a stored query's URL under gateway.public_url does not parse"
                    );
                    Refused::fixed(Code::Internal)
                })?
                .to_string()
        }
        (None, Answered::Versioned) => stored.version().to_string(),
        (None, Answered::Unversioned) => format!("./{}/{}", stored.name(), stored.version()),
        (None, Answered::Admin) => return Ok(None),
    };
    HeaderValue::try_from(reference)
        .map(Some)
        .map_err(|_invalid| {
            tracing::error!("a stored query's location is not a valid header value");
            Refused::fixed(Code::Internal)
        })
}
