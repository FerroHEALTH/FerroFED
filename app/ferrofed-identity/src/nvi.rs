// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer: the [`Localizer`] seam over GF-Localization, the
//! Localization Service search of the Dutch Generic Functions (Annex B §B.1,
//! N4, §14.1).
//!
//! The patient is named by a pseudonymised BSN: a client presents it in the
//! `http://fhir.nl/fhir/NamingSystem/pseudo-bsn` namespace, or in a
//! namespace the configuration lists as standing for it. The gateway never
//! pseudonymises (Annex B §B.7), so a patient in any other namespace cannot
//! be localized and the localization is unavailable, which fails closed
//! (§14.1).
//!
//! The service answers with the care providers, by URA, whose records it
//! exposes to this requester; each URA maps, through the configuration, to
//! the registry member that holds that provider's data, and those members
//! are the candidates. The service has checked the requester's access at
//! each data holder before it answers (the IG's Localization page), so the
//! answer is consent-aware, and still only candidacy: each node checks
//! consent itself (§14.3, N27). A provider outside the federation names no
//! member and adds none.
//!
//! The pseudonym reaches the Localization Service only, inside the binding
//! crate's redacting types; nothing here logs it, and no error carries it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::header::{AUTHORIZATION, HeaderMap};
use nl_generic_functions::identification::{PSEUDO_BSN_SYSTEM, PseudoBsn, Ura};
use nl_generic_functions::nvi::NviClient;
use nl_generic_functions::nvi::error::{InvalidInput, NviError};
use openehr_its::rest::client::{Credentials, InvalidCredentials};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

use crate::localizer::{Localization, Localizer, LocalizerError};
use crate::patient::{IdentifierNamespace, PatientRef};

/// The naming systems of the BSN itself: the IG's `$bsn` system, and the OID
/// it is registered under, as a URN and dotted.
///
/// None of them may stand for the pseudonymised BSN, which is the only
/// identifier the NVI is keyed on (Annex B §B.1).
pub const BSN_SYSTEMS: [&str; 3] = [
    "http://fhir.nl/fhir/NamingSystem/bsn",
    "urn:oid:2.16.840.1.113883.2.4.6.3",
    "2.16.840.1.113883.2.4.6.3",
];

/// Returns whether `namespace` names the BSN itself, one of [`BSN_SYSTEMS`].
#[must_use]
pub fn is_bsn_system(namespace: &str) -> bool {
    BSN_SYSTEMS.contains(&namespace)
}

/// The NVI localizer as the configuration names it.
pub struct NviConfig {
    /// The Localization Service's FHIR base URL, which `Debug` shows without
    /// its userinfo.
    pub base: SecretUrl,
    /// How the gateway authenticates to the service, when the transport does
    /// not.
    pub credentials: Option<Credentials>,
    /// Each care provider, by its URA, mapped to the registry member that
    /// holds its data.
    pub custodians: BTreeMap<String, NodeId>,
    /// The client namespaces that stand for the pseudonymised BSN, beside
    /// [`PSEUDO_BSN_SYSTEM`] itself.
    pub namespaces: BTreeSet<IdentifierNamespace>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS.
    pub client_identity: Option<SecretString>,
    /// Trust roots the service's certificate chains to, beside the
    /// platform's, PEM.
    pub trust_roots: Option<String>,
}

impl fmt::Debug for NviConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NviConfig")
            .field("base", &self.base)
            .field("credentials", &self.credentials)
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .finish()
    }
}

/// An NVI localizer that cannot be built.
///
/// The errors name a URA, a member or a namespace, never a pseudonym or a
/// credential.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NviConfigError {
    /// The base URL does not parse as a URL.
    #[error("the Localization Service base URL is not a URL")]
    BaseUrl(#[source] url::ParseError),
    /// The base URL is not an `http(s)` URL without a query or a fragment.
    #[error(
        "the Localization Service base URL is not an http(s) URL without a query or a fragment"
    )]
    Base(#[source] InvalidInput),
    /// A namespace listed as standing for the pseudonymised BSN is a BSN
    /// system, so a BSN would reach the NVI labelled as a pseudonym.
    #[error("the namespace {0} is a BSN system and cannot stand for the pseudonymised BSN")]
    BsnAsPseudonym(IdentifierNamespace),
    /// A custodian key is empty.
    #[error("a custodian URA is empty")]
    EmptyUra,
    /// A custodian maps to a member the registry does not hold.
    #[error("custodian {ura} maps to member {member}, which is not in the registry")]
    UnknownMember {
        /// The URA as written, a care provider and never a patient value.
        ura: Ura,
        /// The member it names.
        member: NodeId,
    },
    /// A registry member is mapped from no custodian, so no localization
    /// could ever name it and every undirected query would leave it
    /// `not-localized`.
    #[error("registry member {0} is mapped from no custodian URA")]
    UnlocatedMember(NodeId),
    /// A credential does not form an `Authorization` value (RFC 7617 §2,
    /// RFC 6750 §2.1).
    #[error(
        "the credentials of the Localization Service cannot be sent in the Authorization header"
    )]
    Credentials(#[source] InvalidCredentials),
    /// The client certificate and key do not read as PEM.
    #[error("the Localization Service client certificate and key are not PEM")]
    Identity(#[source] reqwest::Error),
    /// The trust roots do not read as PEM certificates.
    #[error("the Localization Service trust roots are not PEM certificates")]
    Roots(#[source] reqwest::Error),
    /// The HTTP client could not be built.
    #[error("the HTTP client for the Localization Service could not be built")]
    Client(#[source] reqwest::Error),
}

/// Why the NVI localizer could not answer.
///
/// None carries a pseudonym or the service's free text.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NviLocalizeError {
    /// The patient's issuing namespace does not stand for the pseudonymised
    /// BSN, which is the only identifier the Localization Service is keyed on
    /// (Annex B §B.1).
    #[error("the namespace {0} is not the pseudonymised BSN the NVI is keyed on")]
    UnmappedNamespace(IdentifierNamespace),
    /// The Localization Service search failed.
    #[error("the Localization Service could not localize the patient")]
    Exchange(#[source] NviError),
}

/// The [`Localizer`] over the NVI Localization Service (Annex B §B.1).
pub struct NviLocalizer {
    client: NviClient,
    custodians: BTreeMap<Ura, NodeId>,
    namespaces: BTreeSet<IdentifierNamespace>,
}

impl NviLocalizer {
    /// Builds the localizer `config` describes over the members of
    /// `registry`.
    ///
    /// # Errors
    ///
    /// An [`NviConfigError`] for a BSN system listed as standing for the
    /// pseudonym, a base URL that is not one, a custodian with an empty URA or
    /// naming a member the registry does not hold, a member no custodian maps
    /// to, and TLS material or a credential that cannot be used.
    pub fn from_config(
        config: NviConfig,
        registry: &RegistrySnapshot,
    ) -> Result<Self, NviConfigError> {
        // NOTE: Annex B §B.1, N33: the NVI is keyed on the pseudonym, so a BSN system listed
        // as its alias would send a BSN there; refused here as well as at configuration load.
        if let Some(bsn) = config
            .namespaces
            .iter()
            .find(|namespace| is_bsn_system(namespace.as_str()))
        {
            return Err(NviConfigError::BsnAsPseudonym(bsn.clone()));
        }
        let base = Url::parse(config.base.expose()).map_err(NviConfigError::BaseUrl)?;
        let http = http_client(&config)?;
        let mut custodians = BTreeMap::new();
        for (ura, member) in config.custodians {
            let ura = Ura::new(ura).map_err(|_empty| NviConfigError::EmptyUra)?;
            if registry.node(&member).is_none() {
                return Err(NviConfigError::UnknownMember { ura, member });
            }
            custodians.insert(ura, member);
        }
        let located: BTreeSet<&NodeId> = custodians.values().collect();
        if let Some(unlocated) = registry
            .nodes()
            .map(ferrofed_registry::snapshot::Node::id)
            .find(|id| !located.contains(id))
        {
            return Err(NviConfigError::UnlocatedMember(unlocated.clone()));
        }
        let client = NviClient::new(base, http).map_err(NviConfigError::Base)?;
        Ok(Self {
            client,
            custodians,
            namespaces: config.namespaces,
        })
    }

    /// The localization of `patient` among `members`, or why there is none.
    async fn locate(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> Result<Localization, LocalizerError> {
        let namespace = patient.namespace();
        let keyed = namespace.as_str() == PSEUDO_BSN_SYSTEM || self.namespaces.contains(namespace);
        let unmapped = || {
            LocalizerError::Backend(Box::new(NviLocalizeError::UnmappedNamespace(
                namespace.clone(),
            )))
        };
        if !keyed {
            return Err(unmapped());
        }
        let pseudonym =
            PseudoBsn::new(SecretString::from(patient.value())).map_err(|_empty| unmapped())?;
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(LocalizerError::DeadlineExceeded);
        }
        let found = match self.client.localize(&pseudonym, timeout).await {
            Ok(found) => found,
            Err(NviError::Timeout) => return Err(LocalizerError::DeadlineExceeded),
            Err(NviError::Rejected { status }) => {
                return Err(LocalizerError::Answered {
                    status,
                    source: Box::new(NviLocalizeError::Exchange(NviError::Rejected { status })),
                });
            }
            Err(error) => {
                return Err(LocalizerError::Backend(Box::new(
                    NviLocalizeError::Exchange(error),
                )));
            }
        };
        let candidates: BTreeSet<NodeId> = found
            .custodians()
            .iter()
            .filter_map(|ura| self.custodians.get(ura))
            .filter(|member| members.contains(member))
            .cloned()
            .collect();
        Ok(if candidates.is_empty() {
            Localization::NoRecords
        } else {
            Localization::Candidates(candidates)
        })
    }
}

impl fmt::Debug for NviLocalizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NviLocalizer")
            .field("client", &self.client)
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .finish()
    }
}

#[async_trait]
impl Localizer for NviLocalizer {
    /// Names the members whose care providers the Localization Service
    /// returned for the patient.
    ///
    /// A service that does not answer, answers a failure, or answers outside
    /// the IG leaves the localization [`Localization::Unavailable`], so §14.1
    /// fail-closed applies.
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> Localization {
        match self.locate(patient, members, deadline).await {
            Ok(localization) => localization,
            Err(error) => Localization::Unavailable(error),
        }
    }
}

/// The HTTP client the service is asked through: no redirects, because the
/// request URL holds the pseudonym, the credential as a default header, and
/// the TLS material `config` names.
fn http_client(config: &NviConfig) -> Result<reqwest::Client, NviConfigError> {
    let mut headers = HeaderMap::new();
    if let Some(credentials) = &config.credentials {
        let header = credentials
            .header_value()
            .map_err(NviConfigError::Credentials)?;
        headers.insert(AUTHORIZATION, header);
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(headers);
    if let Some(identity) = &config.client_identity {
        let identity = reqwest::Identity::from_pem(identity.expose_secret().as_bytes())
            .map_err(NviConfigError::Identity)?;
        builder = builder.identity(identity);
    }
    if let Some(roots) = &config.trust_roots {
        let roots = reqwest::Certificate::from_pem_bundle(roots.as_bytes())
            .map_err(NviConfigError::Roots)?;
        builder = builder.tls_certs_merge(roots);
    }
    builder.build().map_err(NviConfigError::Client)
}
