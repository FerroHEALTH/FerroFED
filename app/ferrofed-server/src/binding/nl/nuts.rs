// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the Dutch binding builds from the Nuts grants `crate::config::nl::nuts`
//! resolves: the credentials each grant provides its node client, and the
//! DID documents its holders are resolved by.

use crate::binding::error::BindingError;
use crate::binding::seam::{ProvideError, Provided, PublicDocument};
use crate::config::nl::nuts::{KEY, NutsOnward};
use crate::config::settings::{Scheme, Settings};
use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::grant::nl::nuts::{NutsCredentials, NutsGrant};
use ferrofed_registry::id::EndpointId;
use nl_generic_functions::nuts_auth::NutsClient;
use nl_generic_functions::nuts_auth::did_document::{DidDocument, MEDIA_TYPE};
use nl_generic_functions::nuts_auth::holder::{Did, HolderKey};
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Returns the credentials the client of `endpoint` sends under the Nuts
/// grant `grant`, within the budgets of `settings`.
///
/// # Errors
///
/// [`ProvideError::NutsClient`] for a client that cannot be built.
pub(super) fn provide(
    grant: &NutsOnward,
    endpoint: &EndpointId,
    settings: &Settings,
) -> Result<Provided, ProvideError> {
    let credentials: SharedCredentials = Arc::new(NutsCredentials::new(
        endpoint.clone(),
        NutsGrant::clone(grant.grant()),
        client(endpoint)?,
        settings.federation.budget.per_node(),
        Arc::new(SystemClock),
    ));
    Ok(Provided {
        credentials,
        dpop: Some(Arc::clone(grant.grant().dpop())),
    })
}

/// Returns the DID document of every holder the Nuts grants of `settings`
/// present as, each served at its `did:web` location (the did:web Method
/// Specification, Read (Resolve)), so an authorization server resolves a
/// presentation's `kid` to the key that signed it (Nuts RFC021 §4.2 item 4).
///
/// Each document is built from the holder keys of those grants alone, so a
/// key change in the configuration changes the document with it.
///
/// # Errors
///
/// [`BindingError::DidDocument`] for two different keys a DID's grants
/// name by one DID URL, and [`BindingError::DidDocumentJson`] for a
/// document that cannot be written.
pub(super) fn documents(settings: &Settings) -> Result<Vec<PublicDocument>, BindingError> {
    let mut holders: BTreeMap<&str, (&Did, Vec<&HolderKey>)> = BTreeMap::new();
    for scheme in settings.credentials.values() {
        let Scheme::Binding(grant) = scheme else {
            continue;
        };
        let grant: &dyn Any = grant.as_ref();
        let Some(nuts) = grant.downcast_ref::<NutsOnward>() else {
            continue;
        };
        let holder = nuts.grant().holder();
        holders
            .entry(holder.did().as_str())
            .or_insert_with(|| (holder.did(), Vec::new()))
            .1
            .push(holder.key());
    }
    holders
        .into_values()
        .map(|(did, keys)| {
            let document =
                DidDocument::new(did, keys).map_err(|source| BindingError::DidDocument {
                    did: did.to_string(),
                    source,
                })?;
            let body =
                serde_json::to_vec(&document).map_err(|source| BindingError::DidDocumentJson {
                    did: did.to_string(),
                    source,
                })?;
            Ok(PublicDocument {
                path: did.document_path(),
                media_type: MEDIA_TYPE,
                body,
            })
        })
        .collect()
}

/// The client the Nuts grant of `endpoint` sends its requests through.
fn client(endpoint: &EndpointId) -> Result<NutsClient, ProvideError> {
    http_client()
        .map(NutsClient::new)
        .map_err(|source| ProvideError::NutsClient {
            section: format!("credentials.{endpoint}.{KEY}"),
            source,
        })
}

/// The HTTP client a Nuts grant sends its token requests through, which
/// [`NutsClient::new`] wraps.
///
/// It follows no redirect, since the token request carries the gateway's
/// credentials (no specification governs the client: our own design).
pub(super) fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[cfg(test)]
mod tests {
    use ferrofed_testkit::kx::KxPeer;
    use rustls::NamedGroup;
    use rustls::crypto::aws_lc_rs::kx_group;

    use super::http_client;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The group the Nuts client agreed on with `peer` in one call, which
    /// ends after the key exchange: the client trusts no root the peer's
    /// certificate chains to.
    async fn agreed_by_the_nuts_client(
        peer: &KxPeer,
    ) -> Result<Option<NamedGroup>, Box<dyn std::error::Error>> {
        let answer = http_client()?
            .get(format!("https://localhost:{}/", peer.address().port()))
            .send()
            .await;
        if answer.is_ok() {
            return Err("the peer's certificate is trusted by no root".into());
        }
        Ok(peer
            .agreed(0)
            .map_err(|seen| format!("the peer recorded {seen} connections"))?)
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn the_nuts_client_prefers_x25519mlkem768() -> TestResult {
        let peer = KxPeer::start(vec![kx_group::X25519, kx_group::X25519MLKEM768])?;
        assert_eq!(
            Some(NamedGroup::X25519MLKEM768),
            agreed_by_the_nuts_client(&peer).await?
        );
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn the_nuts_client_agrees_on_x25519_with_a_classical_only_peer() -> TestResult {
        let peer = KxPeer::start(vec![kx_group::X25519])?;
        assert_eq!(
            Some(NamedGroup::X25519),
            agreed_by_the_nuts_client(&peer).await?
        );
        Ok(())
    }
}
