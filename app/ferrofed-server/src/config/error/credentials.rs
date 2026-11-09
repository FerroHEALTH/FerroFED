// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of the outbound credentials, the signing keys and the grants.

use ferrofed_registry::error::IdError;
use openehr_its::rest::client::InvalidCredentials;

use crate::config::error::basic::BasicFault;

/// Why a `[signing]` key is refused; no variant quotes any part of a key.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SigningKeyError {
    /// The file holds no private key in PEM that `ferrohealth-smart` reads,
    /// or an EC key on another curve than P-256 or P-384.
    #[error("the key cannot be read as a signing key")]
    Key(#[source] ferrohealth_smart::client::assertion::AssertionError),
    /// The key is no EC key: the `[signing]` ring signs with ES256 or ES384
    /// alone (RFC 7518 §3.4).
    #[error("the key is no P-256 or P-384 key, the curves ES256 and ES384 sign with")]
    NotEc,
    /// The previous key is the current key, or the next key is the current
    /// or the previous key.
    #[error("the key is already in the ring")]
    Ring(#[source] ferrohealth_smart::client::keys::KeyRingError),
}

/// A credential, a signing key or a grant the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CredentialsFault {
    /// A credentials section is keyed by something that is not an endpoint id.
    #[error("credentials.{key:?} is not an endpoint id")]
    EndpointId {
        /// The key that was given.
        key: String,
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// A credentials section names more than one scheme: a basic user, an
    /// OAuth 2.0 grant.
    #[error("{section} names more than one credentials scheme; set one")]
    Scheme {
        /// The credentials section.
        section: String,
    },
    /// A service's credentials section names an onward lane, which only a
    /// node's takes.
    #[error("{section}.onward applies to a node's credentials alone; remove it")]
    OnwardOnService {
        /// The credentials section.
        section: String,
    },
    /// A credentials section opts its onward lane in to another party's
    /// organisation, and has no onward table of kind `federation`, the only
    /// issuer that exchanges an organisation's token.
    #[error(
        "{section}.onward_organisations applies to an onward table of kind \"federation\" alone; remove it or set the lane's kind"
    )]
    OnwardOrganisations {
        /// The credentials section.
        section: String,
    },
    /// A credentials section names no scheme at all.
    #[error("{section} names no credentials; remove the section or set one scheme")]
    NoScheme {
        /// The credentials section.
        section: String,
    },
    /// A credential does not form the `Authorization` value the node client
    /// sends.
    ///
    /// The source is the node client's refusal, which quotes nothing.
    #[error("{key} cannot be sent in the Authorization header")]
    Authorization {
        /// The key the credential was read from: the inline key or its
        /// `_file` sibling.
        key: String,
        /// What the node client reported.
        #[source]
        source: InvalidCredentials,
    },
    /// A basic user or password holds a character RFC 7617 §2 forbids.
    #[error("{key} cannot be sent: it holds {fault}, which RFC 7617 §2 forbids")]
    Basic {
        /// The key the value was read from: the inline key or its `_file`
        /// sibling.
        key: String,
        /// The kind of character found, never the character's position.
        fault: BasicFault,
        /// What the node client reported, which quotes nothing.
        #[source]
        source: InvalidCredentials,
    },
    /// A signing key cannot be used: it is no P-256 or P-384 private key in
    /// PKCS#8 PEM, the previous key is the current one, or the next key is
    /// the current or the previous one.
    #[error("{key} is not a usable signing key")]
    SigningKey {
        /// The key the file was named by.
        key: String,
        /// Why the key is refused; it quotes no part of the key.
        #[source]
        source: SigningKeyError,
    },
    /// The next signing key is on another curve than the algorithm it is
    /// meant to sign with: `signing.next_key_algorithm`, or the current key's
    /// when that is unset (RFC 7518 §3.4).
    #[error(
        "{key} holds a key that signs {found}, and the next key is meant to sign {intended}; set signing.next_key_algorithm when the rotation changes the algorithm"
    )]
    NextKeyAlgorithm {
        /// The key the file was named by.
        key: String,
        /// The algorithm the key's curve signs with.
        found: &'static str,
        /// The algorithm the next key is meant to sign with.
        intended: &'static str,
    },
    /// A `DPoP` key cannot be used: it is no P-256 or P-384 private key in
    /// PKCS#8 PEM (RFC 9449).
    #[error("{key} is not a usable DPoP key")]
    DpopKey {
        /// The key the file was named by.
        key: String,
        /// Why the key is refused; it quotes no part of the key.
        #[source]
        source: ferrohealth_smart::client::dpop::DpopKeyError,
    },
    /// A Nuts grant cannot be built: its authorization server is no issuer
    /// URL without userinfo, query or fragment, its scope holds a character
    /// RFC 6749 §3.3 does not admit, or its `client_id` is empty (RFC 8414
    /// §2).
    #[error("{key} is not usable in a Nuts grant")]
    #[cfg(feature = "binding-nl")]
    NutsGrant {
        /// The key of the refused value.
        key: String,
        /// Why it is refused.
        #[source]
        source: nl_generic_functions::nuts_auth::error::InvalidInput,
    },
    /// The holder of a Nuts grant cannot present: its DID is no `did:web`
    /// DID, its key is not one of the DID's or no P-256 or P-384 key, or a
    /// credential is no JWT credential issued to it (Nuts RFC021 §4.2).
    #[error("{section} names a holder that cannot present")]
    #[cfg(feature = "binding-nl")]
    NutsHolder {
        /// The section of the holder.
        section: String,
        /// Why it is refused; it quotes no key and no credential.
        #[source]
        source: nl_generic_functions::nuts_auth::holder::HolderError,
    },
    /// `signing.assertion_lifetime_s` is zero or longer than the five minutes
    /// a client assertion may live.
    #[error(
        "signing.assertion_lifetime_s is {seconds}; a client assertion lives 1 to {max} seconds"
    )]
    AssertionLifetime {
        /// The lifetime given.
        seconds: u64,
        /// The longest lifetime allowed.
        max: u64,
    },
    /// The rotation overlap is shorter than an assertion's lifetime plus the
    /// time a node caches the JWK Set, so a node could meet an assertion signed
    /// by a key the set no longer publishes.
    #[error(
        "signing.rotation_overlap_s ({overlap_s}) must be at least signing.assertion_lifetime_s ({lifetime_s}) plus signing.node_jwks_cache_s ({cache_s})"
    )]
    RotationOverlap {
        /// The overlap window.
        overlap_s: u64,
        /// The assertion lifetime.
        lifetime_s: u64,
        /// How long a node caches the JWK Set.
        cache_s: u64,
    },
    /// A scope is not one the gateway may request onward.
    #[error("{key} is not a SMART on openEHR system scope")]
    Scope {
        /// The key that holds it.
        key: String,
        /// What the scope grammar refused.
        #[source]
        source: ferrofed_engine::onward::ScopeError,
    },
    /// An identity service's credentials section names a Nuts grant.
    #[error(
        "{section} takes basic credentials or the oauth client credentials grant, not a nuts grant"
    )]
    ServiceGrantNotHere {
        /// The section.
        section: String,
    },
    /// A credentials section that takes basic credentials names an OAuth
    /// 2.0 grant, a Nuts grant or a FAPI 2.0 grant.
    #[error("{section} takes basic credentials, not an oauth or nuts grant")]
    GrantNotHere {
        /// The section.
        section: String,
    },
}
