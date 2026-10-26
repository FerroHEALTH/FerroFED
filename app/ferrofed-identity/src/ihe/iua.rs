// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access token of an OAuth 2.0 grant, incorporated in each request to an
//! IHE FHIR server (IUA ITI-72 §3.72.4.2).
//!
//! An [`Authentication::Grant`] holds a token source that obtains the token
//! with the client-credentials grant (IUA ITI-71 §3.71.4.1.2.1) and renews
//! it before it expires. The crate's IHE adapters build the HTTP client of a
//! PIXm, PDQm, PMIR or mCSD client with no default `Authorization` header,
//! beside the `ihe-iti` [`TokenAuthorizer`] the client asks for the header
//! of each request. A `401` drops the refused token, and the request is
//! sent once more with a fresh one (§3.72.4.3).

use std::sync::Arc;

use ihe_iti::authorizer::Authorizer;
use ihe_iti::authorizer::smart::TokenAuthorizer;

use crate::fhir::{self, Authentication, ClientError, Tls};

/// Builds the HTTP client an IHE FHIR client is asked through, and the
/// authorizer that incorporates a grant's token in each of its requests.
///
/// A bearer token or basic credentials ride in the client's default header,
/// as [`fhir::http_client`] composes it, with no authorizer; a grant rides
/// in no default header, and the `ihe-iti` [`TokenAuthorizer`] over its
/// token source is returned beside the client.
///
/// # Errors
/// A [`ClientError`] for a credential that forms no `Authorization` value,
/// or a client that cannot be built.
pub(crate) fn client(
    auth: &Authentication,
    tls: &Tls,
) -> Result<(reqwest::Client, Option<Arc<dyn Authorizer>>), ClientError> {
    match auth {
        Authentication::Grant(source) => {
            let http = fhir::http_client(&Authentication::None, tls)?;
            let authorizer: Arc<dyn Authorizer> = Arc::new(TokenAuthorizer::new(source.clone()));
            Ok((http, Some(authorizer)))
        }
        _ => Ok((fhir::http_client(auth, tls)?, None)),
    }
}
