// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant, `[credentials."<endpoint id>".nuts]`: the onward
//! credential kind of the Dutch binding, the regional realisation of §13.3
//! (Annex B §B.4).
//!
//! The same table under `[nl_gf.nvi.credentials]` authenticates the gateway
//! to the NVI ([`super::nvi`]).
//!
//! The gateway presents its Verifiable Credentials, signed as a presentation
//! with its `did:web` key, and binds the token with `DPoP` (Nuts RFC021). No
//! specification governs the shape of the table: our own design.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::nuts::{NutsCredentials, NutsGrant};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::{Secret, SecretUrl};
use nl_generic_functions::nuts_auth::NutsClient;
use nl_generic_functions::nuts_auth::did_document::{DidDocument, MEDIA_TYPE};
use nl_generic_functions::nuts_auth::error::InvalidInput;
use nl_generic_functions::nuts_auth::holder::{Did, Holder, HolderKey};
use serde::Deserialize;

use crate::binding::seam::{OnwardGrant, Provided, PublicDocument};
use crate::config::error::Error;
use crate::config::secrets::{read_secret, secret};
use crate::config::settings::{Scheme, Settings};
use crate::config::transport::{Encryption, ProtectedSite};
use crate::federation::error::FederationError;

/// The table the grant is configured by, under `[credentials."<id>"]`.
pub const KEY: &str = "nuts";

/// The Nuts grant at one node's authorization server (Annex B §B.4).
///
/// The gateway presents its Verifiable Credentials, signed as a presentation
/// with its `did:web` key, and binds the token with `DPoP` (Nuts RFC021).
///
/// No field has a default but `client_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nuts {
    /// The authorization server's issuer identifier (RFC 8414 §2); `https`
    /// outside the development profile.
    pub authorization_server: Option<SecretUrl>,
    /// The scope the token is asked for, the one the authorization server
    /// maps to its Presentation Definition (Nuts RFC021 §5).
    pub scope: String,
    /// The `client_id` the token request carries (RFC 6749 §3.2.1), when the
    /// authorization server identifies its clients by one.
    pub client_id: Option<String>,
    /// The gateway's own `did:web` identifier, the holder of the credentials.
    /// The gateway serves its DID document at the path the DID resolves to
    /// (the did:web Method Specification, Read (Resolve)).
    pub did: String,
    /// The DID URL of the key the presentation is signed with, `<did>#<id>`,
    /// published in the holder's DID document.
    pub kid: String,
    /// A file holding that key, a P-256 or P-384 private key in PKCS#8 PEM,
    /// read at boot.
    pub key_file: Option<PathBuf>,
    /// The credentials the gateway presents, each with the input descriptor
    /// it answers (`[[credentials."<endpoint id>".nuts.credential]]`).
    pub credential: Vec<NutsCredential>,
    /// A file holding the private key the tokens are bound to with `DPoP`
    /// (RFC 9449), a P-256 or P-384 key in PKCS#8 PEM, read at boot.
    pub dpop_key_file: Option<PathBuf>,
}

/// One Verifiable Credential the gateway presents in the Nuts grant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NutsCredential {
    /// The input descriptor of the authorization server's Presentation
    /// Definition the credential answers (Presentation Exchange 2.0.0).
    pub input_descriptor: String,
    /// A file holding the credential, JWT-encoded (VC Data Model 1.1
    /// §6.3.1), read at boot.
    pub file: Option<PathBuf>,
}

/// The Nuts grant of one endpoint, resolved: the onward credential kind
/// [`OnwardGrant`] wires into the node client.
#[derive(Debug)]
pub struct NutsOnward(NutsGrant);

impl NutsOnward {
    /// Returns the grant.
    #[must_use]
    pub fn grant(&self) -> &NutsGrant {
        &self.0
    }
}

impl OnwardGrant for NutsOnward {
    fn key(&self) -> &'static str {
        KEY
    }

    // NOTE: Nuts RFC021 §7, every endpoint is TLS-protected; the token and
    // definition endpoints the metadata names are held to it by the client.
    fn site(&self, section: &str) -> (String, ProtectedSite) {
        (
            self.0.grant().authorization_server().to_owned(),
            ProtectedSite {
                url_key: format!("{section}.{KEY}.authorization_server"),
                payload: format!("{section}.{KEY} credentials and presentation"),
                requires: Encryption::Https,
            },
        )
    }

    fn provide(
        &self,
        endpoint: &EndpointId,
        settings: &Settings,
    ) -> Result<Provided, FederationError> {
        let credentials: SharedCredentials = Arc::new(NutsCredentials::new(
            endpoint.clone(),
            NutsGrant::clone(&self.0),
            client(endpoint)?,
            settings.federation.budget.per_node(),
            Arc::new(SystemClock),
        ));
        Ok(Provided {
            credentials,
            dpop: Some(Arc::clone(self.0.dpop())),
        })
    }
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
/// [`FederationError::DidDocument`] for two different keys a DID's grants
/// name by one DID URL, and [`FederationError::DidDocumentJson`] for a
/// document that cannot be written.
pub(super) fn documents(settings: &Settings) -> Result<Vec<PublicDocument>, FederationError> {
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
                DidDocument::new(did, keys).map_err(|source| FederationError::DidDocument {
                    did: did.to_string(),
                    source,
                })?;
            let body = serde_json::to_vec(&document).map_err(|source| {
                FederationError::DidDocumentJson {
                    did: did.to_string(),
                    source,
                }
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
fn client(endpoint: &EndpointId) -> Result<NutsClient, FederationError> {
    http_client().map_err(|source| FederationError::NutsClient {
        section: format!("credentials.{endpoint}.{KEY}"),
        source,
    })
}

/// The client a Nuts grant sends its token requests through.
///
/// It follows no redirect, since the token request carries the gateway's
/// credentials (no specification governs the client: our own design).
pub(super) fn http_client() -> Result<NutsClient, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map(NutsClient::new)
}

/// Returns the Nuts grant the `nuts` table at `section` describes (Annex B
/// §B.4, Nuts RFC021): the authorization server an issuer URL (RFC 8414
/// §2), the scope a list of RFC 6749 §3.3 scope tokens, the holder a
/// `did:web` DID whose key `kid` names and whose every credential is a JWT
/// credential issued to it, and a `DPoP` key the tokens are bound to.
pub(super) fn resolve(section: &str, nuts: &Nuts) -> Result<NutsOnward, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{section}.{name}"),
    };
    let server = nuts
        .authorization_server
        .as_ref()
        .ok_or_else(|| missing("authorization_server"))?;
    if nuts.scope.trim().is_empty() {
        return Err(missing("scope"));
    }
    let refused = |name: &str| {
        let key = format!("{section}.{name}");
        move |source| Error::NutsGrant { key, source }
    };
    let mut grant = nl_generic_functions::nuts_auth::Grant::new(server.expose(), &nuts.scope)
        .map_err(|source| match source {
            InvalidInput::Scope => refused("scope")(source),
            _ => refused("authorization_server")(source),
        })?;
    if let Some(client_id) = &nuts.client_id {
        grant = grant
            .with_client_id(client_id.clone())
            .map_err(refused("client_id"))?;
    }
    if nuts.did.is_empty() {
        return Err(missing("did"));
    }
    if nuts.kid.is_empty() {
        return Err(missing("kid"));
    }
    if nuts.credential.is_empty() {
        return Err(missing("credential"));
    }
    let holder_refused = |source| Error::NutsHolder {
        section: section.to_owned(),
        source,
    };
    let did = Did::new(nuts.did.clone()).map_err(holder_refused)?;
    let key_file = nuts
        .key_file
        .as_deref()
        .ok_or_else(|| missing("key_file"))?;
    let pem = secret::<Secret>(&format!("{section}.key"), None, Some(key_file))?
        .ok_or_else(|| missing("key_file"))?;
    let key =
        HolderKey::from_pem(&pem.to_secret_string(), &nuts.kid, &did).map_err(holder_refused)?;
    let mut credentials = Vec::with_capacity(nuts.credential.len());
    for (index, held) in nuts.credential.iter().enumerate() {
        let prefix = format!("{section}.credential[{index}]");
        let file = held.file.as_deref().ok_or_else(|| Error::Missing {
            key: format!("{prefix}.file"),
        })?;
        let jwt = read_secret(&format!("{prefix}.file"), file)?;
        credentials.push((held.input_descriptor.clone(), jwt));
    }
    let holder = Holder::new(did, key, credentials).map_err(holder_refused)?;
    let dpop_file = nuts
        .dpop_key_file
        .as_deref()
        .ok_or_else(|| missing("dpop_key_file"))?;
    let dpop_key = format!("{section}.dpop_key");
    let dpop_pem = secret::<Secret>(&dpop_key, None, Some(dpop_file))?
        .ok_or_else(|| missing("dpop_key_file"))?;
    let prover =
        Prover::from_pem(&dpop_pem.to_secret_string()).map_err(|source| Error::DpopKey {
            key: format!("{dpop_key}_file"),
            source,
        })?;
    Ok(NutsOnward(NutsGrant::new(
        grant,
        Arc::new(holder),
        Arc::new(prover),
    )))
}
