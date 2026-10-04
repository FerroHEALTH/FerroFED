// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter: the [`ConsentPrefilter`] seam over GF-Consent,
//! the closed authorization question of the Dutch Generic Functions (Annex B
//! §B.6, N27a, §13.2.1, §14.3).
//!
//! For each candidate the gateway asks Mitz whether the member's care
//! provider, the data holder, may make the patient's data of the configured
//! categories available to the data user for the configured purpose. The data
//! user is the verified caller: the professional by UZI number and role and
//! the organisation by URA and type, as the caller's token states them
//! ([`Requester`]); a caller whose token does not is never asked about, and
//! nothing is filtered for it. A member whose holder Mitz denies for every
//! category is `consent-denied` and never asked. Every other member is asked,
//! and its node checks consent itself (N26, N27): the pre-filter is a filter
//! in front of the gate, not the gate (§14.3), and a `Permit` clears nothing.
//!
//! Mitz is asked by BSN (the Implementatiehandleiding Open en gesloten
//! autorisatievraag 3.8.2 §3.2.4.2), so the patient must be named in a BSN
//! system or in a namespace the configuration lists as standing for it. A
//! patient named by a pseudonymised BSN cannot be asked about, and the
//! pre-filter then carries no consent signal. The BSN reaches Mitz only,
//! inside the binding crate's redacting types; nothing here logs it, and no
//! error carries it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::header::{AUTHORIZATION, HeaderMap};
use nl_generic_functions::identification::{PSEUDO_BSN_SYSTEM, Ura};
use nl_generic_functions::mitz::error::{ClientError, InvalidInput, MitzError};
use nl_generic_functions::mitz::question::{
    Bsn, CareProviderType, ClosedQuestion, DataCategory, DataHolder, DataUser, MAX_CATEGORIES,
    ProfessionalId, Purpose, RoleCode,
};
use nl_generic_functions::mitz::{MitzClient, UZI_ROOT};
use openehr_its::rest::client::{Credentials, InvalidCredentials};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::task::JoinSet;
use url::Url;

use crate::consent::{ConsentDecision, ConsentError, ConsentPrefilter, Requester};
use crate::nvi::{NviConfigError, derived, is_bsn_system};
use crate::patient::{IdentifierNamespace, PatientRef};

/// The name `OPTIONS {base}/` declares the Mitz pre-filter under.
pub const MITZ_MODE: &str = "nl-gf-mitz";

/// Returns whether `namespace` is the pseudonymised BSN's naming system,
/// which cannot stand for the BSN Mitz is asked by (Annex B §B.1, §B.6).
#[must_use]
pub fn is_pseudonym_system(namespace: &str) -> bool {
    namespace == PSEUDO_BSN_SYSTEM
}

/// Returns whether `code` is a purpose the closed authorization question
/// takes, `TREAT` or `COC` (Implementatiehandleiding Open en gesloten
/// autorisatievraag 3.8.2 §3.2.4.2).
#[must_use]
pub fn is_purpose(code: &str) -> bool {
    Purpose::from_code(code).is_some()
}

/// One member's care provider, the data holder, as the configuration names
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolderConfig {
    /// The care provider's URA; taken from the NVI custodians or the
    /// directory when absent.
    pub ura: Option<String>,
    /// The care provider's category.
    pub kind: String,
}

/// The Mitz pre-filter as the configuration names it.
pub struct MitzConfig {
    /// The Mitz endpoint of the closed authorization question.
    pub endpoint: SecretUrl,
    /// Whether the deployment is marked for development, which alone admits
    /// a plain `http` endpoint.
    pub development: bool,
    /// How the gateway authenticates to Mitz beside mutual TLS, if at all.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM.
    pub client_identity: Option<SecretString>,
    /// Trust roots Mitz's certificate chains to, beside the platform's, PEM.
    pub trust_roots: Option<String>,
    /// The client namespaces that stand for the BSN, beside the BSN systems.
    pub namespaces: BTreeSet<IdentifierNamespace>,
    /// The Mitz data categories asked about.
    pub categories: Vec<String>,
    /// The purpose of use: `TREAT` or `COC`.
    pub purpose: String,
    /// Each member's data holder.
    pub holders: BTreeMap<NodeId, HolderConfig>,
    /// The NVI custodian table, each URA to the member holding its data,
    /// when `[nl_gf.nvi]` gives one.
    pub custodians: BTreeMap<String, NodeId>,
    /// How long one round of questions may take, within the request's own
    /// deadline.
    pub timeout: Duration,
}

impl fmt::Debug for MitzConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MitzConfig")
            .field("endpoint", &self.endpoint)
            .field("development", &self.development)
            .field("credentials", &self.credentials)
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .field("namespaces", &self.namespaces)
            .field("categories", &self.categories)
            .field("purpose", &self.purpose)
            .field("holders", &self.holders)
            .field("custodians", &self.custodians)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// A Mitz pre-filter that cannot be built.
///
/// The errors name a member, a URA, a namespace or a code, never a patient
/// value or a credential.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MitzConfigError {
    /// The endpoint does not parse as a URL.
    #[error("the Mitz endpoint is not a URL")]
    EndpointUrl(#[source] url::ParseError),
    /// The client refuses the endpoint, or its HTTP client cannot be built.
    #[error("the Mitz client cannot be built")]
    Client(#[source] ClientError),
    /// A namespace listed as standing for the BSN is the pseudonymised BSN.
    #[error("the namespace {0} is the pseudonymised BSN and cannot stand for the BSN")]
    PseudonymAsBsn(IdentifierNamespace),
    /// The data categories are refused: none, a duplicate, too many, or a
    /// code that is not one.
    #[error("the data categories are refused")]
    Categories(#[source] InvalidInput),
    /// The purpose is neither `TREAT` nor `COC`.
    #[error("the purpose {0} is neither TREAT nor COC")]
    Purpose(String),
    /// A holder names a member the registry does not hold.
    #[error("the holder of {0} names a member the registry does not hold")]
    UnknownMember(NodeId),
    /// A registry member has no holder, so Mitz could never be asked about
    /// it.
    #[error("registry member {0} has no data holder")]
    NoHolder(NodeId),
    /// A member's holder category or URA is refused.
    #[error("the data holder of {member} is refused")]
    Holder {
        /// The member.
        member: NodeId,
        /// What was refused.
        #[source]
        source: InvalidInput,
    },
    /// A member's care provider has no URA in the configuration, the NVI
    /// custodians or the directory.
    #[error("the data holder of {0} has no URA")]
    NoUra(NodeId),
    /// The configuration, the NVI custodians and the directory name
    /// different URAs for one member.
    #[error("the configuration, the NVI custodians and the directory disagree on the URA of {0}")]
    UraDisagrees(NodeId),
    /// An organisation the registry read from a directory carries URA
    /// identifiers the LRZa rules refuse.
    #[error("the directory's URAs are refused")]
    Directory(#[source] NviConfigError),
    /// A credential does not form an `Authorization` value.
    #[error("the credentials of Mitz cannot be sent in the Authorization header")]
    Credentials(#[source] InvalidCredentials),
    /// The client certificate and key do not read as PEM.
    #[error("the Mitz client certificate and key are not PEM")]
    Identity(#[source] reqwest::Error),
    /// The trust roots do not read as PEM certificates.
    #[error("the Mitz trust roots are not PEM certificates")]
    Roots(#[source] reqwest::Error),
}

/// Why the Mitz pre-filter could not answer for a candidate.
///
/// None carries a BSN or text Mitz wrote.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MitzPrefilterError {
    /// The closed authorization question had no decision for every
    /// category.
    #[error("Mitz gave no decision")]
    Question(#[source] MitzError),
    /// The closed authorization question cannot be asked.
    #[error("the closed authorization question cannot be asked")]
    Refused(#[source] InvalidInput),
    /// The caller's token states a professional, a role, an organisation or
    /// an organisation type the question does not take.
    #[error(
        "the caller's token states a requester the closed authorization question does not take"
    )]
    Requester(#[source] InvalidInput),
    /// A question's task stopped before it answered.
    #[error("a closed authorization question stopped before it answered")]
    Stopped,
}

/// The [`ConsentPrefilter`] over Mitz's closed authorization question
/// (Annex B §B.6, N27a).
pub struct MitzPrefilter {
    client: Arc<MitzClient>,
    namespaces: BTreeSet<IdentifierNamespace>,
    categories: Vec<DataCategory>,
    purpose: Purpose,
    holders: BTreeMap<NodeId, DataHolder>,
    timeout: Duration,
}

impl MitzPrefilter {
    /// Builds the pre-filter `config` describes over the members of
    /// `registry`.
    ///
    /// # Errors
    ///
    /// A [`MitzConfigError`] for an endpoint, a credential or TLS material
    /// that cannot be used, a pseudonym namespace listed as the BSN, a data
    /// category or purpose the question does not take, a holder
    /// naming no member, a member with no holder or no single URA, and a
    /// directory whose URAs the LRZa rules refuse.
    pub fn from_config(
        config: MitzConfig,
        registry: &RegistrySnapshot,
    ) -> Result<Self, MitzConfigError> {
        // NOTE: Annex B §B.1, §B.6: Mitz is asked by BSN and the NVI by pseudonym, so a
        // pseudonym listed as the BSN would reach Mitz labelled as a BSN.
        if let Some(pseudonym) = config
            .namespaces
            .iter()
            .find(|namespace| namespace.as_str() == PSEUDO_BSN_SYSTEM)
        {
            return Err(MitzConfigError::PseudonymAsBsn(pseudonym.clone()));
        }
        let categories = categories(&config.categories).map_err(MitzConfigError::Categories)?;
        let purpose = Purpose::from_code(&config.purpose)
            .ok_or_else(|| MitzConfigError::Purpose(config.purpose.clone()))?;
        let holders = holders(&config, registry)?;
        let client = client(&config)?;
        Ok(Self {
            client: Arc::new(client),
            namespaces: config.namespaces,
            categories,
            purpose,
            holders,
            timeout: config.timeout,
        })
    }

    /// Whether `namespace` names the BSN, the identifier Mitz is asked by.
    fn takes(&self, namespace: &IdentifierNamespace) -> bool {
        is_bsn_system(namespace.as_str()) || self.namespaces.contains(namespace)
    }

    /// The question about `patient`'s data at `holder`, asked for `user`.
    fn question(
        &self,
        patient: Bsn,
        holder: DataHolder,
        user: DataUser,
    ) -> Result<ClosedQuestion, InvalidInput> {
        ClosedQuestion::new(patient, holder, user, self.categories.clone(), self.purpose)
    }
}

impl fmt::Debug for MitzPrefilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MitzPrefilter")
            .field("client", &self.client)
            .field("namespaces", &self.namespaces)
            .field("categories", &self.categories)
            .field("purpose", &self.purpose)
            .field("holders", &self.holders)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

// NOTE: N27, §14.3 ("a filter in front of the gate, not the gate"): each node still
// checks consent itself before it releases data, whatever Mitz answered here.
#[async_trait]
impl ConsentPrefilter for MitzPrefilter {
    /// Denies each candidate whose data holder Mitz denies for every data
    /// category asked about.
    ///
    /// A holder Mitz gives no decision for leaves its members to their
    /// nodes, and the failure is reported; a candidate it permits is asked.
    async fn prefilter(
        &self,
        patient: &PatientRef,
        requester: Option<&Requester>,
        candidates: &[NodeId],
        deadline: Instant,
    ) -> ConsentDecision {
        // NOTE: Implementatiehandleiding §3.2.4.2 asks by BSN; a patient named otherwise,
        // such as by the pseudonym of Annex B §B.7, cannot be asked about, so no signal.
        if !self.takes(patient.namespace()) {
            return ConsentDecision::NoSignal;
        }
        // NOTE: Implementatiehandleiding §3.2.4.2, §13.4: the professional asked about is the
        // verified caller, so a token that does not name them asks nothing and filters no one.
        let Some(requester) = requester else {
            return ConsentDecision::NoSignal;
        };
        let user = match data_user(requester) {
            Ok(user) => user,
            Err(refused) => {
                return ConsentDecision::Unavailable(ConsentError::Backend(Box::new(
                    MitzPrefilterError::Requester(refused),
                )));
            }
        };
        let bsn = match Bsn::new(SecretString::from(patient.value())) {
            Ok(bsn) => bsn,
            Err(refused) => {
                return ConsentDecision::Unavailable(ConsentError::Backend(Box::new(
                    MitzPrefilterError::Refused(refused),
                )));
            }
        };
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(self.timeout);
        if timeout.is_zero() {
            return ConsentDecision::Unavailable(ConsentError::DeadlineExceeded);
        }
        let mut asked: BTreeMap<DataHolder, BTreeSet<NodeId>> = BTreeMap::new();
        for member in candidates {
            if let Some(holder) = self.holders.get(member) {
                asked
                    .entry(holder.clone())
                    .or_default()
                    .insert(member.clone());
            }
        }
        let mut tasks = JoinSet::new();
        let mut failure: Option<ConsentError> = None;
        for (holder, members) in asked {
            match self.question(bsn.clone(), holder, user.clone()) {
                Ok(question) => {
                    let client = Arc::clone(&self.client);
                    tasks.spawn(async move { (members, client.ask(&question, timeout).await) });
                }
                Err(refused) => {
                    failure.get_or_insert(ConsentError::Backend(Box::new(
                        MitzPrefilterError::Refused(refused),
                    )));
                }
            }
        }
        let mut denied = BTreeSet::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                // NOTE: §14.3, N27a: a holder is ruled out only when Mitz denies every
                // category asked; a permit for any leaves the node to decide (N27).
                Ok((members, Ok(answer))) if answer.denies_all() => denied.extend(members),
                Ok((_, Ok(_))) => {}
                Ok((_, Err(error))) => {
                    failure.get_or_insert(consent_error(error));
                }
                Err(_stopped) => {
                    failure.get_or_insert(ConsentError::Backend(Box::new(
                        MitzPrefilterError::Stopped,
                    )));
                }
            }
        }
        match (denied.is_empty(), failure) {
            (true, None) => ConsentDecision::NoSignal,
            (false, None) => ConsentDecision::Denied(denied),
            (true, Some(failure)) => ConsentDecision::Unavailable(failure),
            (false, Some(failure)) => ConsentDecision::Partial { denied, failure },
        }
    }

    fn mode(&self) -> &'static str {
        MITZ_MODE
    }
}

/// The consent error a closed authorization question's failure is.
fn consent_error(error: MitzError) -> ConsentError {
    match error {
        MitzError::Timeout => ConsentError::DeadlineExceeded,
        error => match error.status() {
            Some(status) => ConsentError::Answered {
                status,
                source: Box::new(MitzPrefilterError::Question(error)),
            },
            None => ConsentError::Backend(Box::new(MitzPrefilterError::Question(error))),
        },
    }
}

/// The data user the verified caller is: its organisation by URA and type,
/// and its professional by UZI number and role (§3.2.4.2).
fn data_user(requester: &Requester) -> Result<DataUser, InvalidInput> {
    let organisation = Ura::new(requester.organisation()).map_err(|_empty| InvalidInput::Code)?;
    Ok(DataUser::new(
        organisation,
        CareProviderType::new(requester.organisation_type())?,
        ProfessionalId::new(UZI_ROOT, requester.professional())?,
        RoleCode::new(requester.role())?,
    ))
}

/// The data categories `codes` name: at least one, each once, at most
/// [`MAX_CATEGORIES`].
fn categories(codes: &[String]) -> Result<Vec<DataCategory>, InvalidInput> {
    if codes.is_empty() {
        return Err(InvalidInput::NoCategory);
    }
    if codes.len() > MAX_CATEGORIES {
        return Err(InvalidInput::TooManyCategories {
            limit: MAX_CATEGORIES,
        });
    }
    let mut categories: Vec<DataCategory> = Vec::with_capacity(codes.len());
    for code in codes {
        let category = DataCategory::new(code.as_str())?;
        if categories.contains(&category) {
            return Err(InvalidInput::DuplicateCategory(category));
        }
        categories.push(category);
    }
    Ok(categories)
}

/// Each registry member's data holder: its category from the configuration,
/// and its one URA from the configuration, the NVI custodians and the
/// directory, which must agree where more than one names it.
fn holders(
    config: &MitzConfig,
    registry: &RegistrySnapshot,
) -> Result<BTreeMap<NodeId, DataHolder>, MitzConfigError> {
    if let Some(unknown) = config
        .holders
        .keys()
        .find(|member| registry.node(member).is_none())
    {
        return Err(MitzConfigError::UnknownMember(unknown.clone()));
    }
    let mut named: BTreeMap<NodeId, BTreeSet<String>> = BTreeMap::new();
    for (ura, member) in &config.custodians {
        named.entry(member.clone()).or_default().insert(ura.clone());
    }
    for (ura, members) in derived(registry).map_err(MitzConfigError::Directory)? {
        for member in members {
            named
                .entry(member)
                .or_default()
                .insert(ura.as_str().to_owned());
        }
    }
    let mut holders = BTreeMap::new();
    for node in registry.nodes() {
        let member = node.id();
        let holder = config
            .holders
            .get(member)
            .ok_or_else(|| MitzConfigError::NoHolder(member.clone()))?;
        let mut uras = named.remove(member).unwrap_or_default();
        uras.extend(holder.ura.iter().cloned());
        let mut uras = uras.into_iter();
        let ura = match (uras.next(), uras.next()) {
            (Some(ura), None) => ura,
            (Some(_), Some(_)) => return Err(MitzConfigError::UraDisagrees(member.clone())),
            (None, _) => return Err(MitzConfigError::NoUra(member.clone())),
        };
        let refused = |source| MitzConfigError::Holder {
            member: member.clone(),
            source,
        };
        let ura = Ura::new(ura).map_err(|_empty| refused(InvalidInput::Code))?;
        let kind = CareProviderType::new(holder.kind.as_str()).map_err(refused)?;
        holders.insert(member.clone(), DataHolder::new(ura, kind));
    }
    Ok(holders)
}

/// The client Mitz is asked through: mutual TLS and trust roots as
/// configured, the credential as a default header, and no redirects.
fn client(config: &MitzConfig) -> Result<MitzClient, MitzConfigError> {
    let endpoint = Url::parse(config.endpoint.expose()).map_err(MitzConfigError::EndpointUrl)?;
    let mut headers = HeaderMap::new();
    if let Some(credentials) = &config.credentials {
        let header = credentials
            .header_value()
            .map_err(MitzConfigError::Credentials)?;
        headers.insert(AUTHORIZATION, header);
    }
    let mut builder = reqwest::Client::builder().default_headers(headers);
    if let Some(identity) = &config.client_identity {
        let identity = reqwest::Identity::from_pem(identity.expose_secret().as_bytes())
            .map_err(MitzConfigError::Identity)?;
        builder = builder.identity(identity);
    }
    if let Some(roots) = &config.trust_roots {
        let roots = reqwest::Certificate::from_pem_bundle(roots.as_bytes())
            .map_err(MitzConfigError::Roots)?;
        builder = builder.tls_certs_merge(roots);
    }
    let client = if config.development {
        MitzClient::unencrypted_for_development(endpoint, builder)
    } else {
        MitzClient::new(endpoint, builder)
    };
    client.map_err(MitzConfigError::Client)
}
