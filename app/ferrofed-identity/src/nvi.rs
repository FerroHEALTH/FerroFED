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
//! exposes to this requester; each URA maps to the registry members that hold
//! that provider's data, and those members are the candidates. The map comes
//! from the registry when a directory gave it, each member organisation's URA
//! read by the LRZa rules (Annex B §B.2), or else from the configuration; when
//! both give one they must agree, and every member needs a URA either way. The service has checked the requester's access at
//! each data holder before it answers (the IG's Localization page), so the
//! answer is consent-aware, and still only candidacy: each node checks
//! consent itself (§14.3, N27). A provider outside the federation names no
//! member and adds none.
//!
//! The pseudonym reaches the Localization Service only, inside the binding
//! crate's redacting types; nothing here logs it, and no error carries it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use ferrofed_registry::id::{NodeId, OrganisationId};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use nl_generic_functions::identification::{PSEUDO_BSN_SYSTEM, PseudoBsn, Ura};
use nl_generic_functions::lrza::{self, LrzaError};
use nl_generic_functions::nvi::NviClient;
use nl_generic_functions::nvi::authorizer::Authorizer;
use nl_generic_functions::nvi::error::{InvalidInput, NviError};
use secrecy::SecretString;
use thiserror::Error;
use url::Url;

use crate::behalf::OnBehalfOf;
use crate::fhir::{self, Authentication, ClientError, Tls};
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
    /// How the gateway authenticates to the service with a fixed header.
    pub auth: Authentication,
    /// The authorizer that makes the headers of each request, such as the
    /// Nuts grant's `DPoP`-bound token and its proof (the IG's GFI-005); set
    /// with [`Authentication::None`] alone.
    pub authorizer: Option<Arc<dyn Authorizer>>,
    /// Each care provider, by its URA, mapped to the registry member that
    /// holds its data; empty when the registry's directory gives the map.
    pub custodians: BTreeMap<String, NodeId>,
    /// The client namespaces that stand for the pseudonymised BSN, beside
    /// [`PSEUDO_BSN_SYSTEM`] itself.
    pub namespaces: BTreeSet<IdentifierNamespace>,
    /// The TLS material: a client identity for mutual TLS and trust roots
    /// beside the platform's.
    pub tls: Tls,
}

impl fmt::Debug for NviConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NviConfig")
            .field("base", &self.base)
            .field("auth", &self.auth)
            .field("authorizer", &self.authorizer)
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .field("tls", &self.tls)
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
    /// An organisation the registry read from a directory carries URA
    /// identifiers the LRZa rules refuse (Annex B §B.2).
    #[error("organisation {organisation} carries URA identifiers the LRZa rules refuse")]
    Directory {
        /// The organisation's registry id.
        organisation: OrganisationId,
        /// What the LRZa rules reported.
        #[source]
        source: LrzaError,
    },
    /// The configured custodian map and the one the directory gives disagree
    /// on this URA; the gateway never merges them.
    #[error(
        "nl_gf.nvi.custodians and the directory disagree on custodian {0}: remove the table, or make it agree"
    )]
    CustodiansDisagree(Ura),
    /// A registry member is mapped from no custodian, so no localization
    /// could ever name it and every undirected query would leave it
    /// `not-localized`.
    #[error("registry member {0} is mapped from no custodian URA")]
    UnlocatedMember(NodeId),
    /// Both a fixed credential and an authorizer are configured, so a request
    /// would carry two credentials.
    #[error("the Localization Service takes a fixed credential or an authorizer, not both")]
    TwoCredentials,
    /// The HTTP client could not be built: a credential that forms no
    /// `Authorization` value (RFC 7617 §2, RFC 6750 §2.1), or a client the
    /// platform refuses.
    #[error("the HTTP client for the Localization Service could not be built")]
    Client(#[source] ClientError),
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
    custodians: BTreeMap<Ura, BTreeSet<NodeId>>,
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
    /// naming a member the registry does not hold, a member organisation whose
    /// URAs the LRZa rules refuse, a configured map that disagrees with the
    /// directory's, a member no custodian maps to, and TLS material or a
    /// credential that cannot be used, or a fixed credential beside an
    /// authorizer.
    pub fn from_config(
        config: NviConfig,
        registry: &RegistrySnapshot,
    ) -> Result<Self, NviConfigError> {
        if config.authorizer.is_some() && !matches!(config.auth, Authentication::None) {
            return Err(NviConfigError::TwoCredentials);
        }
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
        let mut written: BTreeMap<Ura, BTreeSet<NodeId>> = BTreeMap::new();
        for (ura, member) in config.custodians {
            let ura = Ura::new(ura).map_err(|_empty| NviConfigError::EmptyUra)?;
            if registry.node(&member).is_none() {
                return Err(NviConfigError::UnknownMember { ura, member });
            }
            written.entry(ura).or_default().insert(member);
        }
        let derived = derived(registry)?;
        let custodians = match (written.is_empty(), derived.is_empty()) {
            (_, true) => written,
            (true, false) => derived,
            (false, false) => {
                if let Some(ura) = disagreement(&written, &derived) {
                    return Err(NviConfigError::CustodiansDisagree(ura));
                }
                derived
            }
        };
        let located: BTreeSet<&NodeId> = custodians.values().flatten().collect();
        if let Some(unlocated) = registry
            .nodes()
            .map(ferrofed_registry::snapshot::Node::id)
            .find(|id| !located.contains(id))
        {
            return Err(NviConfigError::UnlocatedMember(unlocated.clone()));
        }
        let mut client = NviClient::new(base, http).map_err(NviConfigError::Base)?;
        if let Some(authorizer) = config.authorizer {
            client = client.with_authorizer(authorizer);
        }
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
        // NOTE: §14.3, N27a, report T179 on #212: the NVI applies consent and never says whom
        // it dropped, so a member it leaves out is `not-localized`, never `consent-denied`
        // (server test `nl_gf::a_member_the_nvi_leaves_out_is_not_localized_and_complete_holds`).
        let candidates: BTreeSet<NodeId> = found
            .custodians()
            .iter()
            .filter_map(|ura| self.custodians.get(ura))
            .flatten()
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
        _on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Localization {
        match self.locate(patient, members, deadline).await {
            Ok(localization) => localization,
            Err(error) => Localization::Unavailable(error),
        }
    }
}

/// The custodian map the registry gives: each URA its member organisations
/// carry, by the LRZa rules (Annex B §B.2), mapped to the members those
/// organisations operate. It is empty for a registry no directory gave.
pub(crate) fn derived(
    registry: &RegistrySnapshot,
) -> Result<BTreeMap<Ura, BTreeSet<NodeId>>, NviConfigError> {
    let mut derived: BTreeMap<Ura, BTreeSet<NodeId>> = BTreeMap::new();
    for node in registry.nodes() {
        let Some(organisation) = registry.organisation(node.organisation()) else {
            continue;
        };
        let identifiers = organisation
            .identifiers()
            .iter()
            .map(|identifier| (Some(identifier.system()), Some(identifier.value())));
        let ura = lrza::ura_in(identifiers).map_err(|source| NviConfigError::Directory {
            organisation: organisation.id().clone(),
            source,
        })?;
        // NOTE: Annex B §B.2: an organisation with no URA is not a top-level care
        // provider, so it names no custodian and its members need one elsewhere.
        if let Some(ura) = ura {
            derived.entry(ura).or_default().insert(node.id().clone());
        }
    }
    Ok(derived)
}

/// The first URA on which the configured map `written` and the derived map
/// `derived` disagree, if any.
fn disagreement(
    written: &BTreeMap<Ura, BTreeSet<NodeId>>,
    derived: &BTreeMap<Ura, BTreeSet<NodeId>>,
) -> Option<Ura> {
    written
        .keys()
        .chain(derived.keys())
        .find(|ura| written.get(*ura) != derived.get(*ura))
        .cloned()
}

/// The HTTP client the service is asked through: no redirects, because the
/// request URL holds the pseudonym, the credential as a default header, and
/// the TLS material `config` names.
fn http_client(config: &NviConfig) -> Result<reqwest::Client, NviConfigError> {
    fhir::http_client(&config.auth, &config.tls).map_err(NviConfigError::Client)
}
