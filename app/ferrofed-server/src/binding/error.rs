// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a binding cannot build what it adds to the federation.
//!
//! The federation wraps it ([`crate::federation::error::FederationError`]),
//! so a refusal reads as it did when these were the federation's own. No
//! specification governs this: our own design.

#[cfg(feature = "binding-ihe")]
use ferrofed_identity::ihe::pdqm::PdqmConfigError;
#[cfg(feature = "binding-ihe")]
use ferrofed_identity::ihe::pixm::PixmConfigError;
#[cfg(feature = "binding-nl")]
use ferrofed_identity::nl::mitz::MitzConfigError;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrofed_identity::role::patient::PatientRefError;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrofed_registry::error::IdError;

#[cfg(feature = "binding-ihe")]
use crate::binding::ihe::mcsd::registry::DirectoryFailure;
use crate::config::service::{GrantRefused, TlsRefused};

/// What a binding cannot build.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingError {
    /// The PMIR identity feed cannot be built.
    #[error(transparent)]
    #[cfg(feature = "binding-ihe")]
    IdentityFeed(#[from] crate::binding::ihe::pmir::IdentityFeedError),
    /// The registry could not be read from the directory of `[registry.mcsd]`
    /// (§15.1, §15.2, N19, N20).
    #[error("the registry could not be read from the care services directory")]
    #[cfg(feature = "binding-ihe")]
    Directory(#[source] Box<DirectoryFailure>),
    /// The cross-reference a test injected refuses the registry, its reason
    /// in the test kit's words (`test-seam` builds only).
    #[cfg(feature = "test-seam")]
    #[error("the injected cross-reference cannot be enabled: {0}")]
    Injected(String),
    /// `[pixm]` is set but no registry is, neither `registry.document` nor
    /// `[registry.mcsd]`, so it names members that do not exist.
    #[error(
        "the [pixm] resolver needs a registry, registry.document or [registry.mcsd], whose members it names"
    )]
    #[cfg(feature = "binding-ihe")]
    PixmWithoutRegistry,
    /// A key of `[nl_gf.mitz.holders]` or `[nl_gf.nvi.custodians]` is not a
    /// node id.
    #[error("{key} is not a node id")]
    #[cfg(feature = "binding-nl")]
    MitzMember {
        /// The key, a member or a URA and never a patient value.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// A `nl_gf.mitz.namespaces` entry is empty.
    #[error("nl_gf.mitz.namespaces has an empty namespace")]
    #[cfg(feature = "binding-nl")]
    MitzNamespace(#[source] PatientRefError),
    /// The Mitz consent pre-filter refuses its configuration.
    #[error("the [nl_gf.mitz] consent pre-filter cannot be enabled")]
    #[cfg(feature = "binding-nl")]
    Mitz(#[source] MitzConfigError),
    /// A `[pixm]` member key is not a node id.
    #[error("pixm.manager[{manager}].members.{key:?} is not a node id")]
    #[cfg(feature = "binding-ihe")]
    PixmMember {
        /// The Manager's index.
        manager: usize,
        /// The key that was given.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// A `[pixm.namespaces]` key is not a namespace.
    #[error("pixm.namespaces has an empty namespace")]
    #[cfg(feature = "binding-ihe")]
    PixmNamespace(#[source] PatientRefError),
    /// The PIXm resolver refuses its Managers or members.
    #[error("the [pixm] resolver cannot be enabled")]
    #[cfg(feature = "binding-ihe")]
    Pixm(#[source] PixmConfigError),
    /// `[pdqm]` is set but no cross-reference resolver is, so the master
    /// identity it finds could never be resolved (Annex A §A.2, §5.2).
    #[error(
        "the [pdqm] demographics step needs a cross-reference resolver, {}, to resolve the master identity it finds (Annex A §A.2, §5.2)",
        crate::binding::resolver_list()
    )]
    #[cfg(feature = "binding-ihe")]
    PdqmWithoutResolver,
    /// A `[pdqm.namespaces]` key is not a namespace.
    #[error("pdqm.namespaces has an empty namespace")]
    #[cfg(feature = "binding-ihe")]
    PdqmNamespace(#[source] PatientRefError),
    /// The PDQm demographics step refuses its Supplier or its domains.
    #[error("the [pdqm] demographics step cannot be enabled")]
    #[cfg(feature = "binding-ihe")]
    Pdqm(#[source] PdqmConfigError),
    /// The `[audit]` trail the IHE binding's BALP records go through cannot
    /// start.
    #[error("the [audit] trail cannot start")]
    Audit(#[source] crate::audit::AuditTrailError),
    /// A grant that cannot be used: one in the credentials of a service that
    /// takes none, or a node's with no `[signing]` key (§13.1, N25).
    #[error(
        "{section} names a grant it cannot use: a node takes one with [signing], an IHE FHIR service an oauth2 client-credentials grant"
    )]
    Grant {
        /// The credentials section.
        section: String,
    },
    /// The HTTP client an IHE FHIR service's grant sends its token requests
    /// through could not be built.
    #[error("the HTTP client of the token endpoint of {section} could not be built")]
    TokenTransport {
        /// The credentials section.
        section: String,
        /// Why it could not be built; it names no secret.
        #[source]
        source: Box<ferrohealth_smart::client::TokenError>,
    },
    /// The TLS material of an identity, localization, consent or audit
    /// service does not read.
    #[error("the TLS material of a service cannot be used")]
    Tls(#[source] TlsRefused),
    /// The DID document of a Nuts grant's holder cannot be built from the
    /// holder keys of the grants that present as it (Nuts RFC021 §4.2).
    #[error("the DID document of {did} cannot be built from its Nuts grants' keys")]
    #[cfg(feature = "binding-nl")]
    DidDocument {
        /// The holder's DID, public.
        did: String,
        /// Why it cannot be built; it carries no key material.
        #[source]
        source: nl_generic_functions::nuts_auth::did_document::DocumentError,
    },
    /// The DID document of a Nuts grant's holder cannot be written as JSON.
    #[error("the DID document of {did} cannot be written")]
    #[cfg(feature = "binding-nl")]
    DidDocumentJson {
        /// The holder's DID, public.
        did: String,
        /// What the writer reported.
        #[source]
        source: serde_json::Error,
    },
}

impl From<GrantRefused> for BindingError {
    fn from(refused: GrantRefused) -> Self {
        Self::Grant {
            section: refused.section,
        }
    }
}

#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
impl From<crate::config::service::ServiceAuthError> for BindingError {
    fn from(error: crate::config::service::ServiceAuthError) -> Self {
        match error {
            crate::config::service::ServiceAuthError::Refused(refused) => refused.into(),
            crate::config::service::ServiceAuthError::TokenTransport { section, source } => {
                Self::TokenTransport { section, source }
            }
        }
    }
}
